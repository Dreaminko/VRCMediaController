// Suppress console window on Windows release builds
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Embed the icon so it always works regardless of working directory
const EMBEDDED_ICON: &[u8] = include_bytes!("../fav.ico");

mod config;
mod display;
mod heart_rate;
mod i18n;
mod media;
mod osc;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc;

use config::ConfigManager;
use display::compose_chatbox;
use heart_rate::{
    start_heart_rate_monitoring, HeartRateCommand, HeartRateDevice, HeartRateEvent,
    HeartRateHandle, HeartRateStatus,
};
use i18n::I18n;
use media::{start_media_monitoring, TrackEvent, TrackInfo};
use osc::{start_osc, MediaCommand, OscCommand, OscHandle};

struct VrcMediaController {
    i18n: I18n,
    config: ConfigManager,
    osc: OscHandle,

    lang_code: String,
    current_track: String,
    current_raw_track: Option<TrackInfo>,
    osc_ok: bool,

    chatbox_enabled: bool,
    format_buffer: String,
    display_mode: String,
    display_duration: u32,

    heart_rate: HeartRateHandle,
    heart_rate_rx: mpsc::UnboundedReceiver<HeartRateEvent>,
    heart_rate_enabled: bool,
    heart_rate_devices: Vec<HeartRateDevice>,
    heart_rate_status: HeartRateStatus,
    heart_rate_device_id: Option<String>,
    heart_rate_device_name: Option<String>,
    current_heart_rate: Option<(u16, std::time::Instant)>,
    last_chatbox_text: Option<String>,
    last_chatbox_send: Option<std::time::Instant>,
    chatbox_dirty: bool,
    next_heart_rate_reconnect: Option<std::time::Instant>,
    heart_rate_reconnect_delay: std::time::Duration,

    // System tray
    has_tray: bool,
    tray_show_id: tray_icon::menu::MenuId,
    tray_quit_id: tray_icon::menu::MenuId,
    _tray_icon: Option<tray_icon::TrayIcon>,
    quitting: bool,

    // Track updates from media thread
    track_rx: mpsc::UnboundedReceiver<TrackEvent>,

    // Pending window show from tray
    pending_show: bool,
}

impl VrcMediaController {
    #[allow(clippy::too_many_arguments)]
    fn new(
        cc: &eframe::CreationContext<'_>,
        config: ConfigManager,
        i18n: I18n,
        osc: OscHandle,
        heart_rate: HeartRateHandle,
        heart_rate_rx: mpsc::UnboundedReceiver<HeartRateEvent>,
        tray: Option<TrayComponents>,
        track_rx: mpsc::UnboundedReceiver<TrackEvent>,
    ) -> Self {
        // Load CJK-capable system font before anything else
        setup_cjk_fonts(&cc.egui_ctx);

        let chatbox_enabled = config.get_chatbox_enabled();
        let format_buffer = config.get_chatbox_format();
        let display_mode = config.get_display_mode();
        let display_duration = config.get_display_duration();
        let lang_code = config.get_language();
        let heart_rate_enabled = config.get_heart_rate_enabled();
        let heart_rate_device_id = config.get_heart_rate_device_id();
        let heart_rate_device_name = config.get_heart_rate_device_name();
        let no_media_text = i18n.get(&lang_code, "no_media");

        let (tray_icon, has_tray, tray_show_id, tray_quit_id) = match tray {
            Some(t) => {
                // Tray menu events are polled via MenuEvent::receiver() in
                // update(), avoiding the cross-thread set_event_handler approach
                // that can silently break on some Windows configurations.
                (Some(t.icon), true, t.show_id, t.quit_id)
            }
            None => {
                // Dummy IDs for the unused path; has_tray is false so the
                // polling code in update() is never reached.
                (
                    None,
                    false,
                    tray_icon::menu::MenuId::from(0u32),
                    tray_icon::menu::MenuId::from(0u32),
                )
            }
        };

        if heart_rate_enabled {
            if let Some(ref id) = heart_rate_device_id {
                let _ = heart_rate
                    .cmd_tx
                    .send(HeartRateCommand::Connect(id.clone()));
            } else {
                let _ = heart_rate.cmd_tx.send(HeartRateCommand::Scan);
            }
        }

        Self {
            config,
            i18n,
            osc,
            lang_code,
            current_track: no_media_text,
            current_raw_track: None,
            osc_ok: false,
            chatbox_enabled,
            format_buffer,
            display_mode,
            display_duration,
            heart_rate,
            heart_rate_rx,
            heart_rate_enabled,
            heart_rate_devices: Vec::new(),
            heart_rate_status: HeartRateStatus::Disabled,
            heart_rate_device_id,
            heart_rate_device_name,
            current_heart_rate: None,
            last_chatbox_text: None,
            last_chatbox_send: None,
            chatbox_dirty: true,
            next_heart_rate_reconnect: None,
            heart_rate_reconnect_delay: std::time::Duration::from_secs(2),
            _tray_icon: tray_icon,
            tray_show_id,
            tray_quit_id,
            has_tray,
            quitting: false,
            track_rx,
            pending_show: false,
        }
    }

    fn no_media_text(&self) -> String {
        self.i18n.get(&self.lang_code, "no_media")
    }

    fn format_track(&self, info: &TrackInfo) -> String {
        let unknown_str = self.i18n.get(&self.lang_code, "unknown");
        let unknown_artist_str = self.i18n.get(&self.lang_code, "unknown_artist");

        let title = info
            .title
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(&unknown_str);
        let artist = info
            .artist
            .as_deref()
            .filter(|a| !a.is_empty())
            .unwrap_or(&unknown_artist_str);

        self.format_buffer
            .replace("{name}", title)
            .replace("{artist}", artist)
            .replace("{heartrate}", "")
    }

    fn handle_track_update(&mut self, info: Option<TrackInfo>) {
        self.current_raw_track = info.clone();
        self.current_track = match info {
            Some(ref track) => self.format_track(track),
            None => self.no_media_text(),
        };
        self.chatbox_dirty = true;
    }

    fn apply_language(&mut self) {
        self.current_track = match self.current_raw_track {
            Some(ref track) => self.format_track(track),
            None => self.no_media_text(),
        };
        self.chatbox_dirty = true;
    }

    fn active_heart_rate(&self) -> Option<u16> {
        self.current_heart_rate.and_then(|(bpm, received)| {
            (received.elapsed() <= std::time::Duration::from_secs(10)).then_some(bpm)
        })
    }

    fn desired_chatbox_text(&self) -> Option<String> {
        let name = self
            .current_raw_track
            .as_ref()
            .and_then(|t| t.title.as_deref().filter(|n| !n.is_empty()))
            .unwrap_or("");
        let artist = self
            .current_raw_track
            .as_ref()
            .and_then(|t| t.artist.as_deref().filter(|a| !a.is_empty()))
            .unwrap_or("");
        let heart_rate = self
            .heart_rate_enabled
            .then(|| self.active_heart_rate())
            .flatten();

        // When neither source is active, return nothing.
        if self.current_raw_track.is_none() && heart_rate.is_none() {
            return None;
        }

        compose_chatbox(&self.format_buffer, name, artist, heart_rate)
    }

    fn update_chatbox(&mut self, force: bool) {
        if !self.chatbox_enabled {
            return;
        }
        let desired = self.desired_chatbox_text();
        if desired == self.last_chatbox_text && !force {
            self.chatbox_dirty = false;
            return;
        }
        let rate_limit_elapsed = self
            .last_chatbox_send
            .map(|sent| sent.elapsed() >= std::time::Duration::from_secs(3))
            .unwrap_or(true);
        if !force && !rate_limit_elapsed {
            self.chatbox_dirty = true;
            return;
        }

        match desired.clone() {
            Some(text) => {
                let _ = self.osc.cmd_tx.send(OscCommand::SendChatbox(text));
            }
            None => {
                let _ = self.osc.cmd_tx.send(OscCommand::ClearChatbox);
            }
        }
        self.last_chatbox_text = desired;
        self.last_chatbox_send = Some(std::time::Instant::now());
        self.chatbox_dirty = false;
    }
}

impl eframe::App for VrcMediaController {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.osc_ok = self.osc.online.load(Ordering::Relaxed);

        // Close -> hide to tray (only if tray icon was created)
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if self.has_tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                // Keep the event loop alive so tray commands are processed
                // even when the window is hidden.
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            // If no tray, let close happen normally (app exits)
            return;
        }

        // Poll tray menu / click events directly from the tray-icon event
        // channel.  This avoids the global set_event_handler callback which
        // depends on cross-thread wake-ups that can silently break on some
        // Windows configurations.
        if self.has_tray {
            use tray_icon::menu::MenuEvent;
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == self.tray_show_id {
                    self.pending_show = true;
                } else if event.id == self.tray_quit_id {
                    self.config.write_to_disk();
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }

            use tray_icon::TrayIconEvent;
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                } = event
                {
                    self.pending_show = true;
                }
            }
        }

        if self.pending_show {
            self.pending_show = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Track updates from media thread
        while let Ok(TrackEvent::Update(info)) = self.track_rx.try_recv() {
            self.handle_track_update(info);
        }

        while let Ok(event) = self.heart_rate_rx.try_recv() {
            match event {
                HeartRateEvent::Devices(devices) => {
                    // If we are in disconnected/error state and our target
                    // device shows up in the scan result, attempt to
                    // reconnect immediately.
                    if self.heart_rate_enabled {
                        if let Some(ref target_id) = self.heart_rate_device_id {
                            if matches!(
                                &self.heart_rate_status,
                                HeartRateStatus::Disconnected | HeartRateStatus::Error(_)
                            ) && devices.iter().any(|d| d.id == *target_id)
                            {
                                log::info!("[HeartRate] Target device reappeared – reconnecting");
                                self.next_heart_rate_reconnect = None;
                                let _ = self
                                    .heart_rate
                                    .cmd_tx
                                    .send(HeartRateCommand::Connect(target_id.clone()));
                            }
                        }
                    }
                    self.heart_rate_devices = devices;
                }
                HeartRateEvent::Status(status) => {
                    if !matches!(status, HeartRateStatus::Connected) {
                        self.current_heart_rate = None;
                        self.chatbox_dirty = true;
                    }
                    if matches!(status, HeartRateStatus::Connected) {
                        self.next_heart_rate_reconnect = None;
                        self.heart_rate_reconnect_delay = std::time::Duration::from_secs(2);
                    } else if self.heart_rate_enabled
                        && self.heart_rate_device_id.is_some()
                        && matches!(
                            status,
                            HeartRateStatus::Disconnected | HeartRateStatus::Error(_)
                        )
                    {
                        // Immediately scan for the device so we can
                        // reconnect as soon as it reappears.  Fall-back
                        // periodic scans use exponential backoff.
                        self.next_heart_rate_reconnect =
                            Some(std::time::Instant::now() + self.heart_rate_reconnect_delay);
                        self.heart_rate_reconnect_delay = (self.heart_rate_reconnect_delay * 2)
                            .min(std::time::Duration::from_secs(30));
                        let _ = self.heart_rate.cmd_tx.send(HeartRateCommand::Scan);
                    }
                    self.heart_rate_status = status;
                }
                HeartRateEvent::Measurement(bpm) => {
                    self.current_heart_rate = Some((bpm, std::time::Instant::now()));
                    self.chatbox_dirty = true;
                }
            }
        }

        if self
            .current_heart_rate
            .is_some_and(|(_, received)| received.elapsed() > std::time::Duration::from_secs(10))
        {
            self.current_heart_rate = None;
            self.chatbox_dirty = true;
        }

        // Periodic re-scan when waiting to reconnect.
        // Only fire when we are genuinely disconnected — the deadline
        // is cleared as soon as Connecting / Connected is observed.
        if self
            .next_heart_rate_reconnect
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
            && matches!(
                &self.heart_rate_status,
                HeartRateStatus::Disconnected | HeartRateStatus::Error(_)
            )
        {
            self.next_heart_rate_reconnect =
                Some(std::time::Instant::now() + self.heart_rate_reconnect_delay);
            self.heart_rate_reconnect_delay =
                (self.heart_rate_reconnect_delay * 2).min(std::time::Duration::from_secs(30));
            let _ = self.heart_rate.cmd_tx.send(HeartRateCommand::Scan);
        }

        if self.chatbox_dirty {
            self.update_chatbox(false);
        }

        // Build UI
        egui::CentralPanel::default().show(ctx, |ui| {
            self.build_ui(ui);
        });

        // Dynamically adjust window height to fit content
        if !self.quitting {
            let used = ctx.used_size();
            let desired_h = used.y.max(280.0).min(900.0);
            let desired = egui::vec2(480.0, desired_h);
            let current =
                ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()).unwrap_or(desired));
            if (desired.y - current.y).abs() > 4.0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired));
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}

impl VrcMediaController {
    fn build_ui(&mut self, ui: &mut egui::Ui) {
        // Pre-fetch all i18n strings to avoid borrow-extends-into-closure issues
        let t_title = self.i18n.get(&self.lang_code, "title");
        let t_osc_online = self.i18n.get(&self.lang_code, "osc_online");
        let t_osc_error = self.i18n.get(&self.lang_code, "osc_error");
        let t_enable_chatbox = self.i18n.get(&self.lang_code, "enable_chatbox");
        let t_format_label = self.i18n.get(&self.lang_code, "format_label");
        let t_display_mode_label = self.i18n.get(&self.lang_code, "display_mode_label");
        let t_mode_persistent = self.i18n.get(&self.lang_code, "display_mode_persistent");
        let t_mode_timed = self.i18n.get(&self.lang_code, "display_mode_timed");
        let t_display_duration_label = self.i18n.get(&self.lang_code, "display_duration_label");
        let t_language = self.i18n.get(&self.lang_code, "language");
        let t_heart_rate = self.i18n.get(&self.lang_code, "heart_rate");
        let t_enable_heart_rate = self.i18n.get(&self.lang_code, "enable_heart_rate");
        let t_scan_devices = self.i18n.get(&self.lang_code, "scan_devices");
        let t_heart_rate_device = self.i18n.get(&self.lang_code, "heart_rate_device");

        ui.vertical_centered(|ui| {
            ui.heading(&t_title);
        });
        ui.add_space(8.0);

        let heart_status = match &self.heart_rate_status {
            HeartRateStatus::Disabled => self.i18n.get(&self.lang_code, "heart_rate_disabled"),
            HeartRateStatus::Scanning => self.i18n.get(&self.lang_code, "heart_rate_scanning"),
            HeartRateStatus::Disconnected => {
                self.i18n.get(&self.lang_code, "heart_rate_disconnected")
            }
            HeartRateStatus::Connecting => self.i18n.get(&self.lang_code, "heart_rate_connecting"),
            HeartRateStatus::Connected => self.i18n.get(&self.lang_code, "heart_rate_connected"),
            HeartRateStatus::Error(error) => format!(
                "{}: {}",
                self.i18n.get(&self.lang_code, "heart_rate_error"),
                error
            ),
        };
        ui.label(heart_status);
        if let Some(bpm) = self.active_heart_rate() {
            ui.label(
                egui::RichText::new(format!("❤️ {} bpm", bpm))
                    .size(18.0)
                    .strong(),
            );
        }
        ui.add_space(8.0);

        // OSC Status
        let (osc_text, osc_color) = if self.osc_ok {
            (&t_osc_online, egui::Color32::GREEN)
        } else {
            (&t_osc_error, egui::Color32::RED)
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(osc_text.as_str())
                    .color(osc_color)
                    .strong(),
            );
        });
        ui.add_space(8.0);

        // Current track
        let track_display = self.current_track.clone();
        ui.add(egui::Label::new(egui::RichText::new(track_display).size(16.0)).wrap());
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Settings area
        // --- Chatbox toggle ---
        let mut enabled = self.chatbox_enabled;
        if ui.checkbox(&mut enabled, &t_enable_chatbox).changed() {
            self.chatbox_enabled = enabled;
            self.config.set_chatbox_enabled(enabled);
            if !enabled {
                let _ = self.osc.cmd_tx.send(OscCommand::ClearChatbox);
                self.last_chatbox_text = None;
            } else {
                self.chatbox_dirty = true;
                self.update_chatbox(true);
            }
        }

        ui.add_space(6.0);

        // --- Format string ---
        ui.label(egui::RichText::new(&t_format_label).size(12.0));
        let fmt_resp = ui.add_sized(
            [ui.available_width(), 22.0],
            egui::TextEdit::singleline(&mut self.format_buffer),
        );
        if fmt_resp.changed() {
            self.config.set_chatbox_format(&self.format_buffer);
            if let Some(ref track) = self.current_raw_track {
                self.current_track = self.format_track(track);
                self.chatbox_dirty = true;
            }
        }

        ui.add_space(10.0);

        // --- Display mode ---
        ui.label(&t_display_mode_label);

        let mut mode_changed = false;
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.display_mode == "persistent", &t_mode_persistent)
                .clicked()
            {
                self.display_mode = "persistent".to_string();
                self.config.set_display_mode("persistent");
                mode_changed = true;
            }
            if ui
                .selectable_label(self.display_mode == "timed", &t_mode_timed)
                .clicked()
            {
                self.display_mode = "timed".to_string();
                self.config.set_display_mode("timed");
                mode_changed = true;
            }
        });

        if self.display_mode == "timed" {
            ui.add_space(4.0);
            let mut dur = self.display_duration;
            let dur_label = t_display_duration_label.replace("{n}", &dur.to_string());
            ui.horizontal(|ui| {
                ui.label(&dur_label);
                if ui
                    .add(egui::Slider::new(&mut dur, 5..=60).step_by(5.0).suffix("s"))
                    .changed()
                {
                    self.display_duration = dur;
                    self.config.set_display_duration(dur);
                }
            });
        }

        if mode_changed {
            let _ = self.osc.cmd_tx.send(OscCommand::RefreshDisplay);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label(egui::RichText::new(&t_heart_rate).strong());
        let mut hr_enabled = self.heart_rate_enabled;
        if ui.checkbox(&mut hr_enabled, &t_enable_heart_rate).changed() {
            self.heart_rate_enabled = hr_enabled;
            self.config.set_heart_rate_enabled(hr_enabled);
            self.current_heart_rate = None;
            self.chatbox_dirty = true;
            if hr_enabled {
                if let Some(ref id) = self.heart_rate_device_id {
                    let _ = self
                        .heart_rate
                        .cmd_tx
                        .send(HeartRateCommand::Connect(id.clone()));
                } else {
                    let _ = self.heart_rate.cmd_tx.send(HeartRateCommand::Scan);
                }
            } else {
                self.next_heart_rate_reconnect = None;
                let _ = self.heart_rate.cmd_tx.send(HeartRateCommand::Disconnect);
            }
        }

        ui.horizontal(|ui| {
            ui.label(&t_heart_rate_device);
            let selected_name = self
                .heart_rate_device_name
                .clone()
                .unwrap_or_else(|| "-".to_string());
            egui::ComboBox::from_id_salt("heart_rate_device")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for device in self.heart_rate_devices.clone() {
                        let selected =
                            self.heart_rate_device_id.as_deref() == Some(device.id.as_str());
                        if ui.selectable_label(selected, &device.name).clicked() {
                            self.heart_rate_device_id = Some(device.id.clone());
                            self.heart_rate_device_name = Some(device.name.clone());
                            self.config
                                .set_heart_rate_device(Some(device.id.clone()), Some(device.name));
                            if self.heart_rate_enabled {
                                let _ = self
                                    .heart_rate
                                    .cmd_tx
                                    .send(HeartRateCommand::Connect(device.id));
                            }
                        }
                    }
                });
            if ui.button(&t_scan_devices).clicked() {
                let _ = self.heart_rate.cmd_tx.send(HeartRateCommand::Scan);
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // --- Language ---
        ui.horizontal(|ui| {
            ui.label(&t_language);
            let languages = ["English", "中文", "日本語"];
            let lang_codes = ["en", "zh", "ja"];
            let current_idx = lang_codes
                .iter()
                .position(|&c| c == self.lang_code)
                .unwrap_or(0);

            let mut selected_idx = current_idx;
            egui::ComboBox::from_id_salt("lang")
                .selected_text(languages[current_idx])
                .show_ui(ui, |ui| {
                    for (i, name) in languages.iter().enumerate() {
                        if ui.selectable_label(i == selected_idx, *name).clicked() {
                            selected_idx = i;
                        }
                    }
                });

            if selected_idx != current_idx {
                self.lang_code = lang_codes[selected_idx].to_string();
                self.config.set_language(lang_codes[selected_idx]);
                self.apply_language();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Font loading
// ---------------------------------------------------------------------------

fn load_cjk_font_data() -> Option<Vec<u8>> {
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\yugoth.ttf",
        "C:\\Windows\\Fonts\\yugothb.ttf",
        "C:\\Windows\\Fonts\\yugothic.ttf",
        "C:\\Windows\\Fonts\\msgothic.ttf",
        "C:\\Windows\\Fonts\\msjh.ttc",
        "C:\\Windows\\Fonts\\malgun.ttf",
    ];

    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            log::info!("Loaded CJK font: {}", path);
            return Some(data);
        }
    }
    log::warn!("No system CJK font found; CJK characters may show as tofu");
    None
}

fn setup_cjk_fonts(ctx: &egui::Context) {
    let font_data = match load_cjk_font_data() {
        Some(d) => d,
        None => return,
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".to_string(), egui::FontData::from_owned(font_data));
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("cjk".to_string());

    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------------------
// System tray
// ---------------------------------------------------------------------------

struct TrayComponents {
    icon: tray_icon::TrayIcon,
    show_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

#[cfg(windows)]
fn setup_tray_icon(i18n: &I18n, lang: &str) -> Option<TrayComponents> {
    use tray_icon::menu::{Menu, MenuItem};
    use tray_icon::TrayIconBuilder;

    let show_item = MenuItem::new(i18n.get(lang, "tray_show"), true, None);
    let quit_item = MenuItem::new(i18n.get(lang, "tray_quit"), true, None);

    // Extract IDs BEFORE the items are consumed by the menu builder
    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let menu = Menu::with_items(&[&show_item, &quit_item]).ok()?;

    let icon = load_tray_icon()?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(i18n.get(lang, "tray_tooltip"))
        .with_icon(icon)
        .build()
        .ok()?;

    Some(TrayComponents {
        icon: tray,
        show_id,
        quit_id,
    })
}

fn load_tray_icon() -> Option<tray_icon::Icon> {
    let img = load_icon_image()?;
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()
}

#[cfg(not(windows))]
fn setup_tray_icon(_i18n: &I18n, _lang: &str) -> Option<TrayComponents> {
    None
}

/// Load icon image from embedded data, with file-based fallback
fn load_icon_image() -> Option<image::RgbaImage> {
    // Try embedded icon first
    if let Ok(img) = image::load_from_memory(EMBEDDED_ICON) {
        return Some(img.to_rgba8());
    }
    // Fallback to file-based loading
    let path = get_resource_path("fav.ico");
    if let Ok(img) = image::open(&path) {
        return Some(img.to_rgba8());
    }
    // Last resort: blue square fallback (matches Python version)
    Some(image::RgbaImage::from_pixel(
        64,
        64,
        image::Rgba([100, 149, 237, 255]),
    ))
}

fn load_window_icon() -> Option<Arc<egui::IconData>> {
    let img = load_icon_image()?;
    let (w, h) = img.dimensions();
    Some(Arc::new(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }))
}

fn get_resource_path(filename: &str) -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join(filename);
            if path.exists() {
                return path.to_string_lossy().to_string();
            }
        }
    }
    filename.to_string()
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        let id: Vec<u16> = "vrc.media.controller.1.0\0".encode_utf16().collect();
        unsafe {
            let _ = SetCurrentProcessExplicitAppUserModelID(PCWSTR::from_raw(id.as_ptr()));
        }
    }

    let config = ConfigManager::new();
    let i18n = I18n::new();

    let (media_tx, media_rx) = tokio::sync::mpsc::unbounded_channel::<MediaCommand>();
    let (track_tx, track_rx) = tokio::sync::mpsc::unbounded_channel::<TrackEvent>();
    let (heart_rate_tx, heart_rate_rx) = tokio::sync::mpsc::unbounded_channel::<HeartRateEvent>();

    let osc = start_osc(config.clone(), media_tx.clone());
    start_media_monitoring(track_tx, media_rx);
    let heart_rate = start_heart_rate_monitoring(heart_rate_tx);

    let tray = setup_tray_icon(&i18n, &config.get_language());
    let window_icon = load_window_icon();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_min_inner_size([480.0, 280.0])
            .with_resizable(false)
            .with_icon(window_icon.unwrap_or_default()),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "VRCMediaController",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(VrcMediaController::new(
                cc,
                config.clone(),
                i18n,
                osc,
                heart_rate,
                heart_rate_rx,
                tray,
                track_rx,
            )))
        }),
    );
}
