import json
import os
import threading

CONFIG_FILE = "config.json"

DEFAULT_CONFIG = {
    "chatbox_format": "🎵 {name} - {artist}",
    "chatbox_enabled": True,
    "language": "en",
    "chatbox_display_mode": "persistent",  # "persistent" or "timed"
    "chatbox_display_duration": 10,  # seconds, used when mode is "timed"
}


class ConfigManager:
    def __init__(self):
        self.config = DEFAULT_CONFIG.copy()
        self.lock = threading.Lock()
        self._save_timer = None
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
                # No file yet — write defaults immediately (first run)
                self._write_to_disk(self.config.copy())

    def get(self, key):
        """Safely get a configuration value."""
        with self.lock:
            return self.config.get(key)

    def set(self, key, value):
        """Safely set a configuration value.

        Skips writing if the value has not changed.
        Actual disk write is debounced by 500 ms to coalesce rapid updates
        (e.g. every keystroke in the format-string entry).
        """
        with self.lock:
            if key not in self.config or self.config[key] == value:
                return  # Nothing to do
            self.config[key] = value
            self._schedule_save()

    # ------------------------------------------------------------------
    # Internal save helpers
    # ------------------------------------------------------------------

    def _schedule_save(self):
        """(Re)start a 500 ms debounce timer; must be called while holding lock."""
        if self._save_timer is not None:
            self._save_timer.cancel()
        # Take a snapshot of current config to avoid holding the lock during I/O
        snapshot = self.config.copy()
        timer = threading.Timer(0.5, self._write_to_disk, args=(snapshot,))
        timer.daemon = True
        timer.start()
        self._save_timer = timer

    def _save_internal(self):
        """Flush any pending debounced save immediately.

        Call this before the application exits so no in-flight changes are lost.
        Must NOT be called while holding self.lock.
        """
        with self.lock:
            if self._save_timer is not None:
                self._save_timer.cancel()
                self._save_timer = None
            snapshot = self.config.copy()
        self._write_to_disk(snapshot)

    @staticmethod
    def _write_to_disk(snapshot: dict):
        """Write a config snapshot to disk.

        Deliberately called *outside* any lock so file I/O never blocks readers.
        """
        try:
            with open(CONFIG_FILE, "w", encoding="utf-8") as f:
                json.dump(snapshot, f, indent=4)
        except Exception as e:
            print(f"Error saving config.json: {e}")


# Global instance for easy access
config_manager = ConfigManager()
