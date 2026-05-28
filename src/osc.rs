use parking_lot::Mutex;
use rosc::{OscMessage, OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;

use crate::config::ConfigManager;

const VRC_HOST: &str = "127.0.0.1";
const VRC_PORT: u16 = 9000;
const SERVER_PORT: u16 = 9001;
const PERSISTENT_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
pub enum OscCommand {
    SendChatbox(String),
    ClearChatbox,
    RefreshDisplay,
}

#[derive(Debug, Clone)]
pub enum MediaCommand {
    TogglePlayPause,
    SkipNext,
    SkipPrevious,
}

pub struct OscHandle {
    pub cmd_tx: mpsc::UnboundedSender<OscCommand>,
    #[allow(dead_code)]
    pub media_tx: mpsc::UnboundedSender<MediaCommand>,
    pub online: Arc<AtomicBool>,
}

pub fn start_osc(
    config: ConfigManager,
    media_tx: mpsc::UnboundedSender<MediaCommand>,
) -> OscHandle {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let online = Arc::new(AtomicBool::new(false));
    let online_clone = online.clone();
    let media_tx_clone = media_tx.clone();

    let handle = OscHandle {
        cmd_tx,
        media_tx,
        online,
    };

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build OSC tokio runtime");
        rt.block_on(run_osc(cmd_rx, config, media_tx_clone, online_clone));
    });

    handle
}

async fn run_osc(
    mut cmd_rx: mpsc::UnboundedReceiver<OscCommand>,
    config: ConfigManager,
    media_tx: mpsc::UnboundedSender<MediaCommand>,
    online: Arc<AtomicBool>,
) {
    // --- Client socket (sends to VRChat on :9000) ---
    let client = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => {
            let _ = s.connect(format!("{}:{}", VRC_HOST, VRC_PORT));
            Arc::new(Mutex::new(s))
        }
        Err(e) => {
            log::error!("[OSC] Failed to bind client socket: {}", e);
            online.store(false, Ordering::Relaxed);
            while cmd_rx.recv().await.is_some() {}
            return;
        }
    };

    // --- Server socket (listens for VRChat on :9001) ---
    let server_socket = match bind_server_socket(VRC_HOST, SERVER_PORT) {
        Ok(async_sock) => {
            online.store(true, Ordering::Relaxed);
            log::info!("[OSC] Server listening on {}:{}", VRC_HOST, SERVER_PORT);
            async_sock
        }
        Err(e) => {
            log::error!(
                "[OSC] Failed to bind OSC server on {}:{}. {}",
                VRC_HOST,
                SERVER_PORT,
                e
            );
            online.store(false, Ordering::Relaxed);
            while cmd_rx.recv().await.is_some() {}
            return;
        }
    };

    let server_media_tx = media_tx.clone();

    tokio::spawn(async move {
        run_udp_server(server_socket, server_media_tx).await;
    });

    let chatbox_state: Arc<Mutex<Option<ChatboxState>>> = Arc::new(Mutex::new(None));

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            OscCommand::SendChatbox(text) => {
                handle_send_chatbox(&client, &config, &chatbox_state, text).await;
            }
            OscCommand::ClearChatbox => {
                handle_clear_chatbox(&client, &chatbox_state);
            }
            OscCommand::RefreshDisplay => {
                handle_refresh_display(&client, &config, &chatbox_state).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Socket creation with SO_REUSEADDR
// ---------------------------------------------------------------------------

fn bind_server_socket(host: &str, port: u16) -> std::io::Result<tokio::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let addr: std::net::SocketAddr = (host.parse::<std::net::IpAddr>().unwrap(), port).into();
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    // Allow re-binding to a port that was recently used
    sock.set_reuse_address(true)?;

    sock.bind(&addr.into())?;
    let std_sock: std::net::UdpSocket = sock.into();
    std_sock.set_nonblocking(true)?;
    tokio::net::UdpSocket::from_std(std_sock)
}

// ---------------------------------------------------------------------------
// Chatbox helpers
// ---------------------------------------------------------------------------

struct ChatboxState {
    persistent_handle: Option<tokio::task::JoinHandle<()>>,
    timed_clear_handle: Option<tokio::task::JoinHandle<()>>,
    text: String,
}

async fn handle_send_chatbox(
    client: &Arc<Mutex<UdpSocket>>,
    config: &ConfigManager,
    state: &Arc<Mutex<Option<ChatboxState>>>,
    text: String,
) {
    if !config.get_chatbox_enabled() {
        return;
    }

    cancel_existing(state);

    send_osc_message(
        client,
        "/chatbox/input",
        vec![OscType::String(text.clone()), OscType::Bool(true)],
    );

    let mode = config.get_display_mode();
    let new_state = if mode == "persistent" {
        let c = client.clone();
        let txt = text.clone();
        let cfg = config.clone();
        let handle = tokio::spawn(async move {
            loop {
                time::sleep(PERSISTENT_INTERVAL).await;
                if cfg.get_chatbox_enabled() && cfg.get_display_mode() == "persistent" {
                    send_osc_message(
                        &c,
                        "/chatbox/input",
                        vec![OscType::String(txt.clone()), OscType::Bool(true)],
                    );
                } else {
                    break;
                }
            }
        });
        Some(ChatboxState {
            persistent_handle: Some(handle),
            timed_clear_handle: None,
            text,
        })
    } else {
        let c = client.clone();
        let duration = config.get_display_duration() as u64;
        let handle = tokio::spawn(async move {
            time::sleep(Duration::from_secs(duration)).await;
            send_osc_message(
                &c,
                "/chatbox/input",
                vec![OscType::String(String::new()), OscType::Bool(true)],
            );
        });
        Some(ChatboxState {
            persistent_handle: None,
            timed_clear_handle: Some(handle),
            text,
        })
    };

    *state.lock() = new_state;
}

fn handle_clear_chatbox(client: &Arc<Mutex<UdpSocket>>, state: &Arc<Mutex<Option<ChatboxState>>>) {
    cancel_existing(state);
    send_osc_message(
        client,
        "/chatbox/input",
        vec![OscType::String(String::new()), OscType::Bool(true)],
    );
}

async fn handle_refresh_display(
    client: &Arc<Mutex<UdpSocket>>,
    config: &ConfigManager,
    state: &Arc<Mutex<Option<ChatboxState>>>,
) {
    let current_text = state.lock().as_ref().map(|s| s.text.clone());
    if let Some(text) = current_text {
        handle_send_chatbox(client, config, state, text).await;
    }
}

fn cancel_existing(state: &Arc<Mutex<Option<ChatboxState>>>) {
    // Release the lock before calling abort() to avoid holding it unnecessarily
    let maybe = state.lock().take();
    if let Some(s) = maybe {
        if let Some(h) = s.persistent_handle {
            h.abort();
        }
        if let Some(h) = s.timed_clear_handle {
            h.abort();
        }
    }
}

fn send_osc_message(client: &Arc<Mutex<UdpSocket>>, addr: &str, args: Vec<OscType>) {
    let packet = OscPacket::Message(OscMessage {
        addr: addr.to_string(),
        args,
    });
    match rosc::encoder::encode(&packet) {
        Ok(buf) => {
            if let Err(e) = client.lock().send(&buf) {
                log::error!("[OSC] Send error: {}", e);
            }
        }
        Err(e) => log::error!("[OSC] Encode error: {}", e),
    }
}

// ---------------------------------------------------------------------------
// UDP server (tokio async)
// ---------------------------------------------------------------------------

async fn run_udp_server(
    socket: tokio::net::UdpSocket,
    media_tx: mpsc::UnboundedSender<MediaCommand>,
) {
    let mut buf = vec![0u8; 4096];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, _src)) => {
                let data = &buf[..size];
                match rosc::decoder::decode_udp(data) {
                    Ok((_remaining, packet)) => {
                        dispatch_packet(&packet, &media_tx);
                    }
                    Err(e) => {
                        log::debug!("[OSC] Decode error: {:?}", e);
                    }
                }
            }
            Err(e) => {
                log::error!("[OSC] Server recv error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OSC dispatch (Messages + Bundles)
// ---------------------------------------------------------------------------

fn dispatch_packet(packet: &OscPacket, media_tx: &mpsc::UnboundedSender<MediaCommand>) {
    match packet {
        OscPacket::Message(msg) => dispatch_osc_message(msg, media_tx),
        OscPacket::Bundle(bundle) => {
            for pkt in &bundle.content {
                dispatch_packet(pkt, media_tx);
            }
        }
    }
}

fn dispatch_osc_message(msg: &OscMessage, media_tx: &mpsc::UnboundedSender<MediaCommand>) {
    let first_arg_bool = msg.args.first().and_then(|a| match a {
        OscType::Bool(b) => Some(*b),
        OscType::Int(i) => Some(*i != 0),
        // Use threshold instead of exact equality for float parameters
        OscType::Float(f) => Some(*f > 0.5),
        _ => None,
    });

    let new_val = first_arg_bool.unwrap_or(false);
    match msg.addr.as_str() {
        "/avatar/parameters/Media_PlayPause" => {
            log::debug!("[OSC] Recv Media_PlayPause: {:?}", first_arg_bool);
            let old = MEDIA_PLAYPAUSE_STATE.swap(new_val, Ordering::Relaxed);
            if new_val && !old {
                let _ = media_tx.send(MediaCommand::TogglePlayPause);
            }
        }
        "/avatar/parameters/Media_Next" => {
            log::debug!("[OSC] Recv Media_Next: {:?}", first_arg_bool);
            let old = MEDIA_NEXT_STATE.swap(new_val, Ordering::Relaxed);
            if new_val && !old {
                let _ = media_tx.send(MediaCommand::SkipNext);
            }
        }
        "/avatar/parameters/Media_Prev" => {
            log::debug!("[OSC] Recv Media_Prev: {:?}", first_arg_bool);
            let old = MEDIA_PREV_STATE.swap(new_val, Ordering::Relaxed);
            if new_val && !old {
                let _ = media_tx.send(MediaCommand::SkipPrevious);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Edge-triggered state (False -> True) - mirrors Python behaviour
// ---------------------------------------------------------------------------

static MEDIA_PLAYPAUSE_STATE: AtomicBool = AtomicBool::new(false);
static MEDIA_NEXT_STATE: AtomicBool = AtomicBool::new(false);
static MEDIA_PREV_STATE: AtomicBool = AtomicBool::new(false);
