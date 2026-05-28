use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
};

use crate::osc::MediaCommand;

/// Track info reported by Windows SMTC
#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// Commands from media task to GUI (track updates)
#[derive(Debug, Clone)]
pub enum TrackEvent {
    Update(Option<TrackInfo>),
}

pub fn start_media_monitoring(
    track_tx: mpsc::UnboundedSender<TrackEvent>,
    mut media_rx: mpsc::UnboundedReceiver<MediaCommand>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("Failed to build media tokio runtime");
        rt.block_on(run_media(track_tx, &mut media_rx));
    });
}

async fn run_media(
    track_tx: mpsc::UnboundedSender<TrackEvent>,
    media_rx: &mut mpsc::UnboundedReceiver<MediaCommand>,
) {
    let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
        Ok(op) => match op.get() {
            Ok(m) => Some(m),
            Err(e) => {
                log::error!("[Media] Failed to get SMTC manager: {:?}", e);
                None
            }
        },
        Err(e) => {
            log::error!("[Media] Failed to request SMTC manager: {:?}", e);
            None
        }
    };

    let manager = match manager {
        Some(m) => Arc::new(m),
        None => {
            let _ = track_tx.send(TrackEvent::Update(None));
            while media_rx.recv().await.is_some() {}
            return;
        }
    };

    // Subscribe to session changes
    let has_session = Arc::new(AtomicBool::new(false));
    let session_changed = has_session.clone();

    let handler = TypedEventHandler::<SessionManager, CurrentSessionChangedEventArgs>::new(
        move |_sender: &Option<SessionManager>, _args: &Option<CurrentSessionChangedEventArgs>| {
            session_changed.store(true, Ordering::Release);
            Ok(())
        },
    );
    if let Err(e) = manager.CurrentSessionChanged(&handler) {
        log::error!("[Media] Failed to subscribe to session changes: {:?}", e);
    }

    // Shared last-known track to detect changes
    let last_track: Arc<parking_lot::Mutex<Option<TrackInfo>>> =
        Arc::new(parking_lot::Mutex::new(None));

    let manager_clone = manager.clone();
    let last_track_clone = last_track.clone();
    let track_tx_clone = track_tx.clone();

    // The first interval tick fires immediately (t=0), so no separate pre-loop
    // call is needed — this avoids fetching twice at startup.
    let poll_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            fetch_and_notify(&manager_clone, &last_track_clone, &track_tx_clone);
        }
    });

    loop {
        tokio::select! {
            cmd = media_rx.recv() => {
                match cmd {
                    Some(MediaCommand::TogglePlayPause) => {
                        if let Ok(session) = manager.GetCurrentSession() {
                            if let Ok(op) = session.TryTogglePlayPauseAsync() {
                                let _ = op.get();
                            }
                        }
                    }
                    Some(MediaCommand::SkipNext) => {
                        if let Ok(session) = manager.GetCurrentSession() {
                            if let Ok(op) = session.TrySkipNextAsync() {
                                let _ = op.get();
                            }
                        }
                    }
                    Some(MediaCommand::SkipPrevious) => {
                        if let Ok(session) = manager.GetCurrentSession() {
                            if let Ok(op) = session.TrySkipPreviousAsync() {
                                let _ = op.get();
                            }
                        }
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if has_session.swap(false, Ordering::Acquire) {
                    fetch_and_notify(&manager, &last_track, &track_tx);
                }
            }
        }
    }

    poll_task.abort();
}

fn fetch_and_notify(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    last_track: &parking_lot::Mutex<Option<TrackInfo>>,
    track_tx: &mpsc::UnboundedSender<TrackEvent>,
) {
    let info = match manager.GetCurrentSession() {
        Ok(session) => match session.TryGetMediaPropertiesAsync() {
            Ok(op) => match op.get() {
                Ok(props) => {
                    let title = props.Title().ok().map(|t| t.to_string());
                    let artist = props.Artist().ok().map(|a| a.to_string());
                    Some(TrackInfo { title, artist })
                }
                Err(e) => {
                    log::debug!("[Media] No media properties: {:?}", e);
                    None
                }
            },
            Err(e) => {
                log::debug!("[Media] TryGetMediaPropertiesAsync failed: {:?}", e);
                None
            }
        },
        Err(_) => None,
    };

    let mut last = last_track.lock();
    if *last != info {
        *last = info.clone();
        drop(last);
        let _ = track_tx.send(TrackEvent::Update(info));
    }
}
