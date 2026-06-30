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
    const BODY_SENSOR_LOCATION: GUID = GUID::from_u128(0x00002a38_0000_1000_8000_00805f9b34fb);

    struct Connection {
        characteristic: GattCharacteristic,
        value_changed_token: EventRegistrationToken,
        session: GattSession,
        session_status_token: EventRegistrationToken,
        _service: GattDeviceService,
        _device: BluetoothLEDevice,
    }

    impl Drop for Connection {
        fn drop(&mut self) {
            log::info!("[HeartRate] Connection dropped – cleaning up");
            let _ = self
                .characteristic
                .RemoveValueChanged(self.value_changed_token);
            let _ = self
                .session
                .RemoveSessionStatusChanged(self.session_status_token);
            let _ = self.session.SetMaintainConnection(false);
            // Close the device handle so Windows knows we are done with it.
            // Without a live BluetoothLEDevice reference the OS will
            // automatically disconnect after a short timeout.
            self._device.Close().ok();
        }
    }

    pub async fn run(
        mut cmd_rx: mpsc::UnboundedReceiver<HeartRateCommand>,
        event_tx: mpsc::UnboundedSender<HeartRateEvent>,
    ) {
        let mut connection: Option<Connection> = None;
        let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Disabled));
        log::info!("[HeartRate] BLE runtime started");

        while let Some(command) = cmd_rx.recv().await {
            match command {
                HeartRateCommand::Scan => {
                    // Scanning while connected can disrupt the active BLE
                    // link.  Skip the request — the caller will retry
                    // when the connection drops.
                    if connection.is_some() {
                        log::debug!("[HeartRate] Scan skipped – already connected");
                        continue;
                    }
                    log::info!("[HeartRate] Scan requested");
                    let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Scanning));
                    match scan_devices().await {
                        Ok(devices) => {
                            log::info!(
                                "[HeartRate] Scan complete – {} device(s) found",
                                devices.len()
                            );
                            let _ = event_tx.send(HeartRateEvent::Devices(devices));
                            let status = if connection.is_some() {
                                HeartRateStatus::Connected
                            } else {
                                HeartRateStatus::Disconnected
                            };
                            let _ = event_tx.send(HeartRateEvent::Status(status));
                        }
                        Err(error) => {
                            log::error!("[HeartRate] Scan failed: {}", error);
                            let _ = event_tx
                                .send(HeartRateEvent::Status(HeartRateStatus::Error(error)));
                        }
                    }
                }
                HeartRateCommand::Connect(device_id) => {
                    log::info!("[HeartRate] Connect requested (addr={})", device_id);
                    connection = None;
                    let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Connecting));
                    match connect(&device_id, event_tx.clone()).await {
                        Ok(active) => {
                            connection = Some(active);
                            log::info!("[HeartRate] Connected successfully");
                            let _ =
                                event_tx.send(HeartRateEvent::Status(HeartRateStatus::Connected));
                        }
                        Err(error) => {
                            log::error!("[HeartRate] Connect failed: {}", error);
                            let _ = event_tx
                                .send(HeartRateEvent::Status(HeartRateStatus::Error(error)));
                        }
                    }
                }
                HeartRateCommand::Disconnect => {
                    log::info!("[HeartRate] Disconnect requested");
                    connection = None;
                    let _ = event_tx.send(HeartRateEvent::Status(HeartRateStatus::Disabled));
                }
            }
        }
    }

    async fn scan_devices() -> Result<Vec<HeartRateDevice>, String> {
        log::debug!("[HeartRate] Creating BLE advertisement watcher");
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
        log::debug!("[HeartRate] Starting BLE scan (8 s)");
        watcher.Start().map_err(format_error)?;
        tokio::time::sleep(Duration::from_secs(8)).await;
        watcher.Stop().map_err(format_error)?;
        let _ = watcher.RemoveReceived(token);

        let mut devices = Vec::new();
        let addrs: Vec<u64> = addresses.lock().unwrap().iter().copied().collect();
        log::debug!(
            "[HeartRate] Scan saw {} raw address(es), resolving names…",
            addrs.len()
        );
        for addr in addrs {
            // Use FromBluetoothAddressAsync to get a BluetoothLEDevice, but only
            // to resolve the name. The connect path uses the raw address and
            // GetGattServicesForUuidAsync, avoiding GattDeviceService::FromIdAsync
            // which requires a different device-ID format.
            let device = match BluetoothLEDevice::FromBluetoothAddressAsync(addr) {
                Ok(op) => match op.get() {
                    Ok(d) => d,
                    Err(e) => {
                        log::debug!(
                            "[HeartRate] FromBluetoothAddressAsync({:#x}) get failed: {}",
                            addr,
                            format_error(e)
                        );
                        continue;
                    }
                },
                Err(e) => {
                    log::debug!(
                        "[HeartRate] FromBluetoothAddressAsync({:#x}) failed: {}",
                        addr,
                        format_error(e)
                    );
                    continue;
                }
            };
            let name = device.Name().map(|n| n.to_string()).unwrap_or_default();
            log::debug!("[HeartRate] Resolved {:#x} → \"{}\"", addr, name);
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

        log::debug!("[HeartRate] connect: resolving device {:#x}", addr);
        let device = BluetoothLEDevice::FromBluetoothAddressAsync(addr)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;

        // Log pairing status — Pixel Watch may require pairing for a
        // stable long-lived connection.
        match device.DeviceInformation() {
            Ok(info) => match info.Pairing() {
                Ok(pairing) => {
                    let paired = pairing.IsPaired().unwrap_or(false);
                    log::info!(
                        "[HeartRate] Device paired: {}, can-pair: {}",
                        paired,
                        pairing.CanPair().unwrap_or(false)
                    );
                    if !paired {
                        log::warn!(
                            "[HeartRate] Device is NOT paired. Pixel Watch may  \
                                 require pairing to maintain the HR broadcast \
                                 connection."
                        );
                    }
                }
                Err(e) => log::debug!("[HeartRate] Pairing info unavailable: {}", format_error(e)),
            },
            Err(e) => log::debug!(
                "[HeartRate] DeviceInformation unavailable: {}",
                format_error(e)
            ),
        }

        // Subscribe to device-level connection changes for diagnostics.
        {
            let addr_str = format!("{:#x}", addr);
            device
                .ConnectionStatusChanged(&TypedEventHandler::new(move |_dev, _args| {
                    log::info!(
                        "[HeartRate] BluetoothLEDevice({}) ConnectionStatus changed",
                        addr_str
                    );
                    Ok(())
                }))
                .ok();
        }

        log::debug!("[HeartRate] connect: discovering HR service");
        let services_result = device
            .GetGattServicesForUuidAsync(HEART_RATE_SERVICE)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        if services_result.Status().map_err(format_error)? != GattCommunicationStatus::Success {
            log::warn!("[HeartRate] connect: HR service not found");
            return Err("Device does not expose Heart Rate service".to_string());
        }
        let services = services_result.Services().map_err(format_error)?;
        let service = services
            .GetAt(0)
            .map_err(|_| "Heart Rate service was not found on device".to_string())?;

        // Read Body Sensor Location (mandatory per Bluetooth HR Service spec).
        // Some devices — including Pixel Watch — expect this read and will
        // disconnect if the client skips it.
        log::info!("[HeartRate] connect: reading Body Sensor Location");
        let location_result = service
            .GetCharacteristicsForUuidAsync(BODY_SENSOR_LOCATION)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        match location_result.Status() {
            Ok(GattCommunicationStatus::Success) => match location_result.Characteristics() {
                Ok(characteristics) => match characteristics.GetAt(0) {
                    Ok(location_char) => match location_char.ReadValueAsync() {
                        Ok(op) => match op.get() {
                            Ok(read_result)
                                if read_result.Status().map_err(format_error).ok()
                                    == Some(GattCommunicationStatus::Success) =>
                            {
                                if let Ok(buf) = read_result.Value() {
                                    let val = read_buffer(&buf)
                                        .unwrap_or_default()
                                        .first()
                                        .copied()
                                        .unwrap_or(0);
                                    log::info!(
                                        "[HeartRate] Body Sensor Location = {} ({})",
                                        val,
                                        body_sensor_location_name(val)
                                    );
                                }
                            }
                            _ => log::info!(
                                "[HeartRate] Body Sensor Location read returned non-Success"
                            ),
                        },
                        Err(e) => log::info!(
                            "[HeartRate] Body Sensor Location ReadValueAsync failed: {}",
                            format_error(e)
                        ),
                    },
                    Err(_) => {
                        log::info!("[HeartRate] Body Sensor Location characteristic list empty")
                    }
                },
                Err(e) => log::info!(
                    "[HeartRate] Body Sensor Location characteristics unavailable: {}",
                    format_error(e)
                ),
            },
            Ok(status) => log::info!(
                "[HeartRate] Body Sensor Location discovery status: {:?}",
                status
            ),
            Err(e) => log::info!(
                "[HeartRate] Body Sensor Location status error: {}",
                format_error(e)
            ),
        }

        log::debug!("[HeartRate] connect: discovering HR Measurement characteristic");
        let result = service
            .GetCharacteristicsForUuidAsync(HEART_RATE_MEASUREMENT)
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        if result.Status().map_err(format_error)? != GattCommunicationStatus::Success {
            log::warn!("[HeartRate] connect: HR Measurement not found");
            return Err("Unable to access Heart Rate Measurement".to_string());
        }
        let characteristics = result.Characteristics().map_err(format_error)?;
        let characteristic = characteristics
            .GetAt(0)
            .map_err(|_| "Heart Rate Measurement characteristic was not found".to_string())?;

        log::debug!("[HeartRate] connect: configuring GATT session");
        let session = service.Session().map_err(format_error)?;
        let can_maintain = session.CanMaintainConnection().unwrap_or(false);
        log::debug!(
            "[HeartRate] GattSession.CanMaintainConnection = {}",
            can_maintain
        );
        if can_maintain {
            session.SetMaintainConnection(true).map_err(format_error)?;
            log::info!("[HeartRate] SetMaintainConnection(true)");
        }
        let status_tx = event_tx.clone();
        let session_handler =
            TypedEventHandler::<GattSession, GattSessionStatusChangedEventArgs>::new(
                move |_session, args| {
                    if let Some(args) = args {
                        match args.Status()? {
                            GattSessionStatus::Active => {
                                log::info!("[HeartRate] GattSession → Active");
                                let _ = status_tx
                                    .send(HeartRateEvent::Status(HeartRateStatus::Connected));
                            }
                            GattSessionStatus::Closed => {
                                log::info!("[HeartRate] GattSession → Closed");
                                let _ = status_tx
                                    .send(HeartRateEvent::Status(HeartRateStatus::Disconnected));
                            }
                            other => {
                                log::debug!("[HeartRate] GattSession → {:?}", other);
                            }
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
                                log::info!("[HeartRate] Measurement: {} bpm", bpm);
                                let _ = event_tx.send(HeartRateEvent::Measurement(bpm));
                            }
                            Err(error) => log::warn!(
                                "[HeartRate] Invalid measurement (len={}): {}",
                                bytes.len(),
                                error
                            ),
                        }
                    }
                }
            }
            Ok(())
        });
        let token = characteristic
            .ValueChanged(&handler)
            .map_err(format_error)?;
        log::info!("[HeartRate] connect: writing CCCD (Notify)");
        let status = characteristic
            .WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            )
            .map_err(format_error)?
            .get()
            .map_err(format_error)?;
        if status != GattCommunicationStatus::Success {
            log::error!("[HeartRate] connect: CCCD write rejected ({:?})", status);
            let _ = characteristic.RemoveValueChanged(token);
            return Err("The device rejected heart-rate notifications".to_string());
        }
        log::info!("[HeartRate] CCCD Notify enabled – waiting for measurements");

        Ok(Connection {
            characteristic,
            value_changed_token: token,
            session,
            session_status_token,
            _service: service,
            _device: device,
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

    /// Human-readable name for Body Sensor Location enum values
    /// (Bluetooth SIG Assigned Numbers, Heart Rate Service).
    fn body_sensor_location_name(val: u8) -> &'static str {
        match val {
            0 => "Other",
            1 => "Chest",
            2 => "Wrist",
            3 => "Finger",
            4 => "Hand",
            5 => "Ear Lobe",
            6 => "Foot",
            _ => "Reserved",
        }
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
