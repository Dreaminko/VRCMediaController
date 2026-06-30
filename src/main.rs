// Suppress console window on Windows release builds
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Embed the icon so it always works regardless of working directory
const EMBEDDED_ICON: &[u8] = include_bytes!("../fav.ico");

mod config;
mod display;
mod i18n;
mod media;
mod osc;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use config::ConfigManager;
use display::compose_chatbox;
use i18n::I18n;
use media::{start_media_monitoring, TrackEvent, TrackInfo};
use osc::{start_osc, MediaCommand, OscCommand, OscHandle};

enum TrayCommand {
    Show,
    Quit,
}

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
    last_chatbox_text: Option<String>,
    last_chatbox_send: Option<std::time::Instant>,
    chatbox_dirty: bool,

    // System tray
    has_tray: bool,
    tray_show_id: tray_icon::menu::MenuId,
    tray_quit_id: tray_icon::menu::MenuId,
    _tray_icon: Option<tray_icon::TrayIcon>,
    tray_rx: Option<mpsc::UnboundedReceiver<TrayCommand>>,
    quitting: bool,
    quitting_flag: Arc<AtomicBool>,

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
        tray: Option<TrayComponents>,
        track_rx: mpsc::UnboundedReceiver<TrackEvent>,
        tray_rx: mpsc::UnboundedReceiver<TrayCommand>,
        tray_tx: mpsc::UnboundedSender<TrayCommand>,
    ) -> Self {
        setup_cjk_fonts(&cc.egui_ctx);

        let chatbox_enabled = config.get_chatbox_enabled();
        let format_buffer = config.get_chatbox_format();
        let display_mode = config.get_display_mode();
        let display_duration = config.get_display_duration();
        let lang_code = config.get_language();
        let no_media_text = i18n.get(&lang_code, "no_media");
        let quitting_flag = Arc::new(AtomicBool::new(false));

        let (tray_icon, has_tray, tray_show_id, tray_quit_id) = match tray {
            Some(t) => {
                let show_id = t.show_id.clone();
                let quit_id = t.quit_id.clone();
                let egui_ctx = cc.egui_ctx.clone();
                let menu_tx = tray_tx.clone();
                let config_for_menu = config.clone();
                let quitting_flag_for_menu = quitting_flag.clone();
                tray_icon::menu::MenuEvent::set_event_handler(Some(
                    move |event: tray_icon::menu::MenuEvent| {
                        if event.id == show_id {
                            let _ = menu_tx.send(TrayCommand::Show);
                            egui_ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                            egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        } else if event.id == quit_id {
                            config_for_menu.write_to_disk();
                            quitting_flag_for_menu.store(true, Ordering::Relaxed);
                            let _ = menu_tx.send(TrayCommand::Quit);
                            egui_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        egui_ctx.request_repaint();
                    },
                ));

                let egui_ctx = cc.egui_ctx.clone();
                tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
                    if let tray_icon::TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = tray_tx.send(TrayCommand::Show);
                        egui_ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        egui_ctx.request_repaint();
                    }
                }));

                (Some(t.icon), true, t.show_id, t.quit_id)
            }
            None => (
                None,
                false,
                tray_icon::menu::MenuId::from(0u32),
                tray_icon::menu::MenuId::from(0u32),
            ),
        };

        Self {
            i18n,
            config,
            osc,
            lang_code,
            current_track: no_media_text,
            current_raw_track: None,
            osc_ok: false,
            chatbox_enabled,
            format_buffer,
            display_mode,
            display_duration,
            last_chatbox_text: None,
            last_chatbox_send: None,
            chatbox_dirty: true,
            has_tray,
            tray_show_id,
            tray_quit_id,
            _tray_icon: tray_icon,
            tray_rx: if has_tray { Some(tray_rx) } else { None },
            quitting: false,
            quitting_flag,
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

    fn desired_chatbox_text(&self) -> Option<String> {
        let track = self.current_raw_track.as_ref()?;
        let name = track
            .title
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or("");
        let artist = track
            .artist
            .as_deref()
            .filter(|a| !a.is_empty())
            .unwrap_or("");
        compose_chatbox(&self.format_buffer, name, artist)
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

        let should_bypass_rate_limit = desired.is_none() && self.last_chatbox_text.is_some();
        let rate_limit_elapsed = self
            .last_chatbox_send
            .map(|sent| sent.elapsed() >= std::time::Duration::from_secs(3))
            .unwrap_or(true);
        if !force && !rate_limit_elapsed && !should_bypass_rate_limit {
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
        self.quitting |= self.quitting_flag.load(Ordering::Relaxed);

        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if self.has_tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            return;
        }

        if let Some(ref mut tray_rx) = self.tray_rx {
            while let Ok(command) = tray_rx.try_recv() {
                match command {
                    TrayCommand::Show => {
                        self.pending_show = true;
                    }
                    TrayCommand::Quit => {
                        self.config.write_to_disk();
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                }
            }
        } else if self.has_tray {
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
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } = event
                {
                    self.pending_show = true;
                }
            }
        }

        if self.pending_show {
            self.pending_show = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        while let Ok(TrackEvent::Update(info)) = self.track_rx.try_recv() {
            self.handle_track_update(info);
        }

        if self.chatbox_dirty {
            self.update_chatbox(false);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.build_ui(ui);
        });

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

        ui.vertical_centered(|ui| {
            ui.heading(&t_title);
        });
        ui.add_space(8.0);

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

        let track_display = self.current_track.clone();
        ui.add(egui::Label::new(egui::RichText::new(track_display).size(16.0)).wrap());
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

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

        ui.label(egui::RichText::new(&t_format_label).size(12.0));
        let fmt_resp = ui.add_sized(
            [ui.available_width(), 22.0],
            egui::TextEdit::singleline(&mut self.format_buffer),
        );
        if fmt_resp.changed() {
            self.config.set_chatbox_format(&self.format_buffer);
            if let Some(ref track) = self.current_raw_track {
                self.current_track = self.format_track(track);
            }
            self.chatbox_dirty = true;
        }

        ui.add_space(10.0);

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

fn load_icon_image() -> Option<image::RgbaImage> {
    if let Ok(img) = image::load_from_memory(EMBEDDED_ICON) {
        return Some(img.to_rgba8());
    }
    let path = get_resource_path("fav.ico");
    if let Ok(img) = image::open(&path) {
        return Some(img.to_rgba8());
    }
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

    let osc = start_osc(config.clone(), media_tx.clone());
    start_media_monitoring(track_tx, media_rx);

    let tray = setup_tray_icon(&i18n, &config.get_language());
    let window_icon = load_window_icon();
    let (tray_tx, tray_rx) = mpsc::unbounded_channel::<TrayCommand>();

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
                tray,
                track_rx,
                tray_rx,
                tray_tx,
            )))
        }),
    );
}
