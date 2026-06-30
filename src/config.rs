use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;

const CONFIG_FILE: &str = "config.json";

/// Returns the path to the config file.
/// In release builds, uses the directory of the executable for reliable
/// operation when launched via shortcuts, the system tray, or Start Menu.
/// In debug builds, falls back to the current working directory.
fn config_path() -> std::path::PathBuf {
    #[cfg(not(debug_assertions))]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join(CONFIG_FILE);
            }
        }
    }
    std::path::PathBuf::from(CONFIG_FILE)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_chatbox_format")]
    pub chatbox_format: String,
    #[serde(default = "default_chatbox_enabled")]
    pub chatbox_enabled: bool,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_display_mode")]
    pub chatbox_display_mode: String,
    #[serde(default = "default_display_duration")]
    pub chatbox_display_duration: u32,
    #[serde(default)]
    pub heart_rate_enabled: bool,
    #[serde(default)]
    pub heart_rate_device_id: Option<String>,
    #[serde(default)]
    pub heart_rate_device_name: Option<String>,
    #[serde(default = "default_heart_rate_format")]
    pub heart_rate_format: String,
}

fn default_chatbox_format() -> String {
    "🎵 {name} - {artist}".to_string()
}
fn default_chatbox_enabled() -> bool {
    true
}
fn default_language() -> String {
    "en".to_string()
}
fn default_display_mode() -> String {
    "persistent".to_string()
}
fn default_display_duration() -> u32 {
    10
}
fn default_heart_rate_format() -> String {
    "❤️ {heartrate} bpm".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            chatbox_format: default_chatbox_format(),
            chatbox_enabled: default_chatbox_enabled(),
            language: default_language(),
            chatbox_display_mode: default_display_mode(),
            chatbox_display_duration: default_display_duration(),
            heart_rate_enabled: false,
            heart_rate_device_id: None,
            heart_rate_device_name: None,
            heart_rate_format: default_heart_rate_format(),
        }
    }
}

#[derive(Clone)]
pub struct ConfigManager {
    inner: Arc<RwLock<Config>>,
}

impl ConfigManager {
    pub fn new() -> Self {
        let config = Self::load_from_disk();
        let mgr = Self {
            inner: Arc::new(RwLock::new(config)),
        };
        // Write defaults on first run
        if !config_path().exists() {
            mgr.write_to_disk();
        }
        mgr
    }

    fn load_from_disk() -> Config {
        let path = config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(data) => serde_json::from_str(&data).unwrap_or_else(|e| {
                    log::error!("Error parsing config.json: {}", e);
                    Config::default()
                }),
                Err(e) => {
                    log::error!("Error reading config.json: {}", e);
                    Config::default()
                }
            }
        } else {
            Config::default()
        }
    }

    pub fn get_language(&self) -> String {
        self.inner.read().language.clone()
    }

    pub fn get_chatbox_enabled(&self) -> bool {
        self.inner.read().chatbox_enabled
    }

    pub fn get_chatbox_format(&self) -> String {
        self.inner.read().chatbox_format.clone()
    }

    pub fn get_display_mode(&self) -> String {
        self.inner.read().chatbox_display_mode.clone()
    }

    pub fn get_display_duration(&self) -> u32 {
        self.inner.read().chatbox_display_duration
    }

    pub fn get_heart_rate_enabled(&self) -> bool {
        self.inner.read().heart_rate_enabled
    }

    pub fn get_heart_rate_device_id(&self) -> Option<String> {
        self.inner.read().heart_rate_device_id.clone()
    }

    pub fn get_heart_rate_device_name(&self) -> Option<String> {
        self.inner.read().heart_rate_device_name.clone()
    }

    pub fn get_heart_rate_format(&self) -> String {
        self.inner.read().heart_rate_format.clone()
    }

    pub fn set_language(&self, lang: &str) {
        let mut cfg = self.inner.write();
        if cfg.language != lang {
            cfg.language = lang.to_string();
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_chatbox_enabled(&self, enabled: bool) {
        let mut cfg = self.inner.write();
        if cfg.chatbox_enabled != enabled {
            cfg.chatbox_enabled = enabled;
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_chatbox_format(&self, format: &str) {
        let mut cfg = self.inner.write();
        if cfg.chatbox_format != format {
            cfg.chatbox_format = format.to_string();
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_display_mode(&self, mode: &str) {
        let mut cfg = self.inner.write();
        if cfg.chatbox_display_mode != mode {
            cfg.chatbox_display_mode = mode.to_string();
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_display_duration(&self, duration: u32) {
        let mut cfg = self.inner.write();
        if cfg.chatbox_display_duration != duration {
            cfg.chatbox_display_duration = duration;
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_heart_rate_enabled(&self, enabled: bool) {
        let mut cfg = self.inner.write();
        if cfg.heart_rate_enabled != enabled {
            cfg.heart_rate_enabled = enabled;
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_heart_rate_device(&self, id: Option<String>, name: Option<String>) {
        let mut cfg = self.inner.write();
        if cfg.heart_rate_device_id != id || cfg.heart_rate_device_name != name {
            cfg.heart_rate_device_id = id;
            cfg.heart_rate_device_name = name;
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn set_heart_rate_format(&self, format: &str) {
        let mut cfg = self.inner.write();
        if cfg.heart_rate_format != format {
            cfg.heart_rate_format = format.to_string();
            drop(cfg);
            self.write_to_disk();
        }
    }

    pub fn write_to_disk(&self) {
        let cfg = self.inner.read();
        match serde_json::to_string_pretty(&*cfg) {
            Ok(json) => {
                if let Err(e) = fs::write(config_path(), json) {
                    log::error!("Error writing config.json: {}", e);
                }
            }
            Err(e) => log::error!("Error serializing config: {}", e),
        }
    }
}
