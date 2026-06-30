use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartRateDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartRateStatus {
    Disabled,
    Scanning,
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum HeartRateEvent {
    Devices(Vec<HeartRateDevice>),
    Status(HeartRateStatus),
    Measurement(u16),
}

#[derive(Debug)]
pub enum HeartRateCommand {
    Scan,
    Connect(String),
    Disconnect,
}

pub struct HeartRateHandle {
    pub cmd_tx: mpsc::UnboundedSender<HeartRateCommand>,
}

pub fn start_heart_rate_monitoring(
    event_tx: mpsc::UnboundedSender<HeartRateEvent>,
) -> HeartRateHandle {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    #[cfg(windows)]
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build heart-rate tokio runtime");
        runtime.block_on(windows_ble::run(cmd_rx, event_tx));
    });

    #[cfg(not(windows))]
    std::thread::spawn(move || {
        let mut cmd_rx = cmd_rx;
        let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Error(
            "BLE heart-rate monitoring is only supported on Windows".to_string(),
        )));
        while cmd_rx.blocking_recv().is_some() {}
    });

    HeartRateHandle { cmd_tx }
}

/// Parses the Bluetooth SIG Heart Rate Measurement characteristic.
/// Bit 0 selects an 8-bit or 16-bit heart-rate value. Remaining optional
/// fields follow the value and do not affect BPM extraction.
pub fn parse_heart_rate_measurement(data: &[u8]) -> Result<u16, &'static str> {
    let flags = *data.first().ok_or("missing flags")?;
    if flags & 0x01 == 0 {
        data.get(1)
            .copied()
            .map(u16::from)
            .ok_or("missing 8-bit bpm")
    } else {
        let bytes = data.get(1..3).ok_or("missing 16-bit bpm")?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
}

#[cfg(windows)]
mod windows_ble {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use windows::core::GUID;
    use windows::Devices::Bluetooth::Advertisement::{
        BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
        BluetoothLEScanningMode,
    };
    use windows::Devices::Bluetooth::BluetoothLEDevice;
    use windows::Devices::Bluetooth::GenericAttributeProfile::{
        GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
        GattCommunicationStatus, GattDeviceService, GattSession, GattSessionStatus,
        GattSessionStatusChangedEventArgs,
    };
    use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
    use windows::Storage::Streams::{DataReader, IBuffer};

    const HEART_RATE_SERVICE: GUID = GUID::from_u128(0x0000180d_0000_1000_8000_00805f9b34fb);
    const HEART_RATE_MEASUREMENT: GUID = GUID::from_u128(0x00002a37_0000_1000_8000_00805f9b34fb);

    struct Connection {
        characteristic: GattCharacteristic,
        value_changed_token: EventRegistrationToken,
        session: GattSession,
        session_status_token: EventRegistrationToken,
        _service: GattDeviceService,
    }

    impl Drop for Connection {
        fn drop(&mut self) {
            let _ = self
                .characteristic
                .RemoveValueChanged(self.value_changed_token);
            let _ = self
                .session
                .RemoveSessionStatusChanged(self.session_status_token);
            let _ = self.session.SetMaintainConnection(false);
        }
    }

    pub async fn run(
        mut cmd_rx: mpsc::UnboundedReceiver<HeartRateCommand>,
        event_tx: mpsc::UnboundedSender<HeartRateEvent>,
    ) {
        let mut connection: Option<Connection> = None;
        let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Disabled));

        while let Some(command) = cmd_rx.recv().await {
            match command {
                HeartRateCommand::Scan => {
                    let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Scanning));
                    match scan_devices().await {
                        Ok(devices) => {
                            let _ = event_tx.send(HeartRateEvent::Devices(devices));
                            let status = if connection.is_some() {
                                HeartRateStatus::Connected
                            } else {
                                HeartRateStatus::Disconnected
                            };
                            let _ = event_tx.send(HeartRateEvent::Status(status));
                        }
                        Err(error) => {
                            let _ = event_tx
                                .send(HeartRateEvent::Status(HeartRateStatus::Error(error)));
                        }
                    }
                }
                HeartRateCommand::Connect(device_id) => {
                    connection = None;
                    let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Connecting));
                    match connect(&device_id, event_tx.clone()).await {
                        Ok(active) => {
                            connection = Some(active);
                            let _ =
                                event_tx.send(HeartRateEvent::Status(HeartRateStatus::Connected));
                        }
                        Err(error) => {
                            let _ = event_tx
                                .send(HeartRateEvent::Status(HeartRateStatus::Error(error)));
                        }
                    }
                }
                HeartRateCommand::Disconnect => {
                    connection = None;
                    let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Disabled));
                }
            }
        }
    }

    async fn scan_devices() -> Result<Vec<HeartRateDevice>, String> {
        let watcher = BluetoothLEAdvertisementWatcher::new()
            .map_err(|e| format!("Failed to create BLE watcher: {}", format_error(e)))?;
        watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)
            .map_err(format_error)?;

        let addresses: Arc<Mutex<BTreeSet<u64>>> = Arc::new(Mutex::new(BTreeSet::new()));
        let addresses_clone = addresses.clone();

        let handler = TypedEventHandler::<
            BluetoothLEAdvertisementWatcher,
            BluetoothLEAdvertisementReceivedEventArgs,
        >::new(move |_watcher, args| {
            if let Some(args) = args {
                if let Ok(addr) = args.BluetoothAddress() {
                    addresses_clone.lock().unwrap().insert(addr);
                }
            }
            Ok(())
        });

        let token = watcher.Received(&handler).map_err(format_error)?;
        watcher.Start().map_err(format_error)?;
        tokio::time::sleep(Duration::from_secs(8)).await;
        watcher.Stop().map_err(format_error)?;
        let _ = watcher.RemoveReceived(token);

        let mut devices = Vec::new();
        let addrs: Vec<u64> = addresses.lock().unwrap().iter().copied().collect();
        for addr in addrs {
            // Use FromBluetoothAddressAsync to get a BluetoothLEDevice, but only
            // to resolve the name. The connect path uses the raw address and
            // GetGattServicesForUuidAsync, avoiding GattDeviceService::FromIdAsync
            // which requires a different device-ID format.
            let device = match BluetoothLEDevice::FromBluetoothAddressAsync(addr) {
                Ok(op) => match op.get() {
                    Ok(d) => d,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            let name = device.Name().map(|n| n.to_string()).unwrap_or_default();
            devices.push(HeartRateDevice {
                id: addr.to_string(),
                name: if name.is_empty() {
                    "Heart Rate Sensor".to_string()
                } else {
                    name
                },
            });
        }

        devices.sort_by(|a, b| a.name.cmp(&b.name));
        devices.dedup_by(|a, b| a.id == b.id);
        Ok(devices)
    }

    async fn connect(
        device_id: &str,
        event_tx: mpsc::UnboundedSender<HeartRateEvent>,
    ) -> Result<Connection, String> {
        let addr: u64 = device_id
            .parse()
            .map_err(|_| format!("Invalid Bluetooth address: {}", device_id))?;

        let device = BluetoothLEDevice::FromBluetoothAddressAsync(addr)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;

        let services_result = device
            .GetGattServicesForUuidAsync(HEART_RATE_SERVICE)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        if services_result.Status().map_err(format_error)? != GattCommunicationStatus::Success {
            return Err("Device does not expose Heart Rate service".to_string());
        }
        let services = services_result.Services().map_err(format_error)?;
        let service = services
            .GetAt(0)
            .map_err(|_| "Heart Rate service was not found on device".to_string())?;

        let result = service
            .GetCharacteristicsForUuidAsync(HEART_RATE_MEASUREMENT)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        if result.Status().map_err(format_error)? != GattCommunicationStatus::Success {
            return Err("Unable to access Heart Rate Measurement".to_string());
        }
        let characteristics = result.Characteristics().map_err(format_error)?;
        let characteristic = characteristics
            .GetAt(0)
            .map_err(|_| "Heart Rate Measurement characteristic was not found".to_string())?;

        let session = service.Session().map_err(format_error)?;
        if session.CanMaintainConnection().unwrap_or(false) {
            session.SetMaintainConnection(true).map_err(format_error)?;
        }
        let status_tx = event_tx.clone();
        let session_handler =
            TypedEventHandler::<GattSession, GattSessionStatusChangedEventArgs>::new(
                move |_session, args| {
                    if let Some(args) = args {
                        match args.Status()? {
                            GattSessionStatus::Active => {
                                let _ = status_tx
                                    .send(HeartRateEvent::Status(HeartRateStatus::Connected));
                            }
                            GattSessionStatus::Closed => {
                                let _ = status_tx
                                    .send(HeartRateEvent::Status(HeartRateStatus::Disconnected));
                            }
                            _ => {}
                        }
                    }
                    Ok(())
                },
            );
        let session_status_token = session
            .SessionStatusChanged(&session_handler)
            .map_err(format_error)?;

        let handler = TypedEventHandler::<
            GattCharacteristic,
            windows::Devices::Bluetooth::GenericAttributeProfile::GattValueChangedEventArgs,
        >::new(move |_sender, args| {
            if let Some(args) = args {
                if let Ok(buffer) = args.CharacteristicValue() {
                    if let Ok(bytes) = read_buffer(&buffer) {
                        match parse_heart_rate_measurement(&bytes) {
                            Ok(bpm) => {
                                let _ = event_tx.send(HeartRateEvent::Measurement(bpm));
                            }
                            Err(error) => log::warn!("[HeartRate] Invalid measurement: {}", error),
                        }
                    }
                }
            }
            Ok(())
        });
        let token = characteristic
            .ValueChanged(&handler)
            .map_err(format_error)?;
        let status = characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            )
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        if status != GattCommunicationStatus::Success {
            let _ = characteristic.RemoveValueChanged(token);
            return Err("The device rejected heart-rate notifications".to_string());
        }

        Ok(Connection {
            characteristic,
            value_changed_token: token,
            session,
            session_status_token,
            _service: service,
        })
    }

    fn read_buffer(buffer: &IBuffer) -> windows::core::Result<Vec<u8>> {
        let reader = DataReader::FromBuffer(buffer)?;
        let mut bytes = vec![0; reader.UnconsumedBufferLength()? as usize];
        reader.ReadBytes(&mut bytes)?;
        Ok(bytes)
    }

    fn format_error(error: windows::core::Error) -> String {
        error.message().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_heart_rate_measurement;

    #[test]
    fn parses_8_bit_measurement_with_optional_data() {
        assert_eq!(
            parse_heart_rate_measurement(&[0x10, 82, 0x34, 0x12]),
            Ok(82)
        );
    }

    #[test]
    fn parses_16_bit_measurement() {
        assert_eq!(parse_heart_rate_measurement(&[0x01, 0x2c, 0x01]), Ok(300));
    }

    #[test]
    fn rejects_truncated_measurements() {
        assert!(parse_heart_rate_measurement(&[]).is_err());
        assert!(parse_heart_rate_measurement(&[0x00]).is_err());
        assert!(parse_heart_rate_measurement(&[0x01, 0x20]).is_err());
    }
}
