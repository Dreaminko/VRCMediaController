import json
import os
import threading

CONFIG_FILE = "config.json"

DEFAULT_CONFIG = {
    "chatbox_format": "🎵 {name} - {artist}",
    "chatbox_enabled": True,
    "language": "en"
}

class ConfigManager:
    def __init__(self):
        self.config = DEFAULT_CONFIG.copy()
        self.lock = threading.Lock()
        self.load()

    def load(self):
        """Load configuration from file."""
        with self.lock:
            if os.path.exists(CONFIG_FILE):
                try:
                    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                        data = json.load(f)
                        # Merge loaded data with defaults in case of missing keys
                        for k, v in data.items():
                            if k in self.config:
                                self.config[k] = v
                except Exception as e:
                    print(f"Error loading config.json: {e}")
            else:
                self._save_internal()

    def get(self, key):
        """Safely get a configuration value."""
        with self.lock:
            return self.config.get(key)

    def set(self, key, value):
        """Safely set a configuration value and save."""
        with self.lock:
            if key in self.config:
                self.config[key] = value
                self._save_internal()

    def _save_internal(self):
        """Save configuration to file (requires lock)."""
        try:
            with open(CONFIG_FILE, "w", encoding="utf-8") as f:
                json.dump(self.config, f, indent=4)
        except Exception as e:
            print(f"Error saving config.json: {e}")

# Global instance for easy access
config_manager = ConfigManager()
