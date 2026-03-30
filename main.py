import ctypes
import os
import sys
import threading

import customtkinter as ctk
import pystray
from PIL import Image

from core import i18n, media_control, osc_runner
from core.config import config_manager

ctk.set_appearance_mode("Dark")
ctk.set_default_color_theme("blue")


def get_resource_path(relative_path):
    try:
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.abspath(".")
    return os.path.join(base_path, relative_path)


class App(ctk.CTk):
    def __init__(self):
        super().__init__()
        self.lang_code = config_manager.get("language") or "en"

        self.title(i18n.get_text(self.lang_code, "title"))
        self.geometry("450x440")
        self.resizable(False, False)

        # Set AppUserModelID so Windows taskbar shows the correct icon
        try:
            myappid = "vrc.media.controller.1.0"
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)
        except Exception:
            pass

        # Set window icon
        icon_path = get_resource_path("fav.ico")
        if os.path.exists(icon_path):
            try:
                self.iconbitmap(icon_path)
                self.after(200, lambda: self.iconbitmap(icon_path))
            except Exception as e:
                print(f"Could not load icon: {e}")

        # --- Cached i18n strings ---
        self._no_media_text = i18n.get_text(self.lang_code, "no_media")

        # State Variables
        self.current_track = self._no_media_text
        self.last_track = ""
        self._current_raw_track = None
        self._osc_ok = False
        self.tray_icon = None
        self._quitting = False

        # --- UI SETUP ---

        # Status Frame
        self.status_frame = ctk.CTkFrame(self)
        self.status_frame.pack(pady=10, padx=20, fill="x")

        self.osc_status_label = ctk.CTkLabel(
            self.status_frame,
            text=i18n.get_text(self.lang_code, "osc_starting"),
            font=("Arial", 14, "bold"),
        )
        self.osc_status_label.pack(pady=5)

        # Track Label
        self.track_label = ctk.CTkLabel(
            self,
            text=self.current_track,
            font=("Arial", 16),
            wraplength=410,
            justify="center",
        )
        self.track_label.pack(pady=10)

        # Settings Frame
        self.settings_frame = ctk.CTkFrame(self)
        self.settings_frame.pack(pady=10, padx=20, fill="both", expand=True)

        # Chatbox Toggle
        self.chatbox_var = ctk.BooleanVar(value=config_manager.get("chatbox_enabled"))
        self.chatbox_switch = ctk.CTkSwitch(
            self.settings_frame,
            text=i18n.get_text(self.lang_code, "enable_chatbox"),
            variable=self.chatbox_var,
            command=self.on_toggle_chatbox,
        )
        self.chatbox_switch.pack(pady=10, padx=10, anchor="w")

        # Format String Input
        self.format_label = ctk.CTkLabel(
            self.settings_frame,
            text=i18n.get_text(self.lang_code, "format_label"),
        )
        self.format_label.pack(anchor="w", padx=10)

        self.format_entry = ctk.CTkEntry(self.settings_frame, width=350)
        self.format_entry.insert(0, config_manager.get("chatbox_format"))
        self.format_entry.pack(pady=5, padx=10, fill="x")
        self.format_entry.bind("<KeyRelease>", self.on_format_changed)

        # --- Display Mode ---
        self.display_mode_label = ctk.CTkLabel(
            self.settings_frame,
            text=i18n.get_text(self.lang_code, "display_mode_label"),
            anchor="w",
        )
        self.display_mode_label.pack(anchor="w", padx=10, pady=(8, 2))

        # Build name <-> key map for current language
        self._mode_options_map = {
            "persistent": i18n.get_text(self.lang_code, "display_mode_persistent"),
            "timed": i18n.get_text(self.lang_code, "display_mode_timed"),
        }
        current_mode = config_manager.get("chatbox_display_mode") or "persistent"

        self.display_mode_seg = ctk.CTkSegmentedButton(
            self.settings_frame,
            values=list(self._mode_options_map.values()),
            command=self.on_display_mode_changed,
        )
        self.display_mode_seg.set(self._mode_options_map[current_mode])
        self.display_mode_seg.pack(anchor="w", padx=10, pady=(0, 4))

        # Duration row (label + slider), always in the layout
        self.duration_frame = ctk.CTkFrame(self.settings_frame, fg_color="transparent")
        self.duration_frame.pack(anchor="w", padx=10, fill="x", pady=(0, 6))

        self.duration_title_label = ctk.CTkLabel(self.duration_frame, text="")
        self.duration_title_label.pack(side="left")

        current_duration = config_manager.get("chatbox_display_duration") or 10
        self.duration_slider = ctk.CTkSlider(
            self.duration_frame,
            from_=5,
            to=60,
            number_of_steps=11,  # 5, 10, 15 … 60
            command=self.on_duration_changed,
            width=190,
        )
        self.duration_slider.set(current_duration)
        self.duration_slider.pack(side="left", padx=(8, 0))

        # Initialise label text; hide duration row if not in timed mode
        self._update_duration_label()
        if current_mode != "timed":
            self.duration_frame.pack_forget()

        # Language Selection
        self.lang_combo = ctk.CTkComboBox(
            self.settings_frame,
            values=["English", "中文", "日本語"],
            command=self.on_lang_changed,
        )
        code_to_name = {"en": "English", "zh": "中文", "ja": "日本語"}
        self.lang_combo.set(code_to_name.get(self.lang_code, "English"))
        self.lang_combo.pack(pady=10, padx=10, anchor="w")

        # --- INITIALIZATION ---

        # Start OSC Components
        if osc_runner.start_osc():
            self._osc_ok = True
            self.osc_status_label.configure(
                text=i18n.get_text(self.lang_code, "osc_online"), text_color="green"
            )
        else:
            self._osc_ok = False
            self.osc_status_label.configure(
                text=i18n.get_text(self.lang_code, "osc_error"), text_color="red"
            )

        # Start Media Monitoring Component
        media_control.start_media_polling(self.on_media_update)

        # Start UI updater
        self.after(500, self.update_ui)

        # Setup system tray icon
        self._setup_tray()

        # Override close button to hide to tray
        self.protocol("WM_DELETE_WINDOW", self.hide_to_tray)

    # ------------------------------------------------------------------
    # System tray
    # ------------------------------------------------------------------

    def _build_tray_menu(self):
        """Build the pystray menu using current language strings."""

        def on_show(icon, item):
            self.after(0, self._show_window)

        def on_quit(icon, item):
            self.after(0, self._quit_app)

        return pystray.Menu(
            pystray.MenuItem(
                i18n.get_text(self.lang_code, "tray_show"),
                on_show,
                default=True,
            ),
            pystray.MenuItem(
                i18n.get_text(self.lang_code, "tray_quit"),
                on_quit,
            ),
        )

    def _setup_tray(self):
        """Create and start the system tray icon in a background daemon thread."""
        icon_path = get_resource_path("fav.ico")
        try:
            image = Image.open(icon_path)
        except Exception:
            # Fallback: plain blue square
            image = Image.new("RGB", (64, 64), color=(100, 149, 237))

        tooltip = i18n.get_text(self.lang_code, "tray_tooltip")
        self.tray_icon = pystray.Icon(
            "VRCMediaController",
            image,
            tooltip,
            self._build_tray_menu(),
        )

        tray_thread = threading.Thread(target=self.tray_icon.run, daemon=True)
        tray_thread.start()

    def hide_to_tray(self):
        """Hide the main window to the system tray (called by X button)."""
        self.withdraw()

    def _show_window(self):
        """Restore the main window from the system tray."""
        self.deiconify()
        self.lift()
        self.focus_force()

    def _quit_app(self):
        """Fully exit the application (called from tray menu)."""
        self._quitting = True
        config_manager._save_internal()
        osc_runner.stop_osc()
        media_control.stop_media_polling()
        if self.tray_icon is not None:
            self.tray_icon.stop()
        self.destroy()
        os._exit(0)

    # ------------------------------------------------------------------
    # Language helpers
    # ------------------------------------------------------------------

    def on_lang_changed(self, choice):
        name_to_code = {"English": "en", "中文": "zh", "日本語": "ja"}
        self.lang_code = name_to_code.get(choice, "en")
        config_manager.set("language", self.lang_code)
        self.apply_language()

    def apply_language(self):
        # Refresh cached no_media string first
        self._no_media_text = i18n.get_text(self.lang_code, "no_media")

        self.title(i18n.get_text(self.lang_code, "title"))
        if self._osc_ok:
            self.osc_status_label.configure(
                text=i18n.get_text(self.lang_code, "osc_online")
            )
        else:
            self.osc_status_label.configure(
                text=i18n.get_text(self.lang_code, "osc_error")
            )
        self.chatbox_switch.configure(
            text=i18n.get_text(self.lang_code, "enable_chatbox")
        )
        self.format_label.configure(text=i18n.get_text(self.lang_code, "format_label"))

        # Refresh display-mode section
        self.display_mode_label.configure(
            text=i18n.get_text(self.lang_code, "display_mode_label")
        )
        self._mode_options_map = {
            "persistent": i18n.get_text(self.lang_code, "display_mode_persistent"),
            "timed": i18n.get_text(self.lang_code, "display_mode_timed"),
        }
        self.display_mode_seg.configure(values=list(self._mode_options_map.values()))
        current_mode = config_manager.get("chatbox_display_mode") or "persistent"
        self.display_mode_seg.set(self._mode_options_map[current_mode])
        self._update_duration_label()

        # Rebuild tray menu with updated language
        if self.tray_icon is not None:
            self.tray_icon.menu = self._build_tray_menu()
            self.tray_icon.title = i18n.get_text(self.lang_code, "tray_tooltip")

        if self._current_raw_track:
            self.on_media_update(self._current_raw_track)
        else:
            self.current_track = self._no_media_text

    # ------------------------------------------------------------------
    # Display-mode helpers
    # ------------------------------------------------------------------

    def _show_duration_frame(self):
        """Insert the duration row between the segmented button and lang combo."""
        self.lang_combo.pack_forget()
        self.duration_frame.pack(anchor="w", padx=10, fill="x", pady=(0, 6))
        self.lang_combo.pack(pady=10, padx=10, anchor="w")

    def _hide_duration_frame(self):
        """Remove the duration row from the layout."""
        self.duration_frame.pack_forget()

    def _update_duration_label(self):
        """Refresh the duration label to show the current slider value."""
        val = int(round(self.duration_slider.get()))
        tmpl = i18n.get_text(self.lang_code, "display_duration_label")
        self.duration_title_label.configure(text=tmpl.replace("{n}", str(val)))

    def on_display_mode_changed(self, choice):
        """Called when the user clicks a segment on the display-mode button."""
        # Map localised label back to internal key
        mode = "persistent"
        for key, label in self._mode_options_map.items():
            if label == choice:
                mode = key
                break
        config_manager.set("chatbox_display_mode", mode)
        if mode == "timed":
            self._show_duration_frame()
        else:
            self._hide_duration_frame()

        # Re-apply the new mode to whatever is currently showing in the chatbox
        if self.current_track != self._no_media_text and config_manager.get(
            "chatbox_enabled"
        ):
            osc_runner.send_chatbox(self.current_track)

    def on_duration_changed(self, value):
        """Called as the user drags the duration slider."""
        val = int(round(value))
        config_manager.set("chatbox_display_duration", val)
        self._update_duration_label()

    # ------------------------------------------------------------------
    # Event handlers
    # ------------------------------------------------------------------

    def on_toggle_chatbox(self):
        enabled = self.chatbox_var.get()
        config_manager.set("chatbox_enabled", enabled)
        if not enabled:
            osc_runner.clear_chatbox()
        elif self.current_track != self._no_media_text:
            osc_runner.send_chatbox(self.current_track)

    def on_format_changed(self, event):
        val = self.format_entry.get()
        config_manager.set("chatbox_format", val)
        if self._current_raw_track:
            self.on_media_update(self._current_raw_track)

    def on_media_update(self, track_info):
        """Called by the background media-monitoring thread when the track changes."""
        self._current_raw_track = track_info
        if track_info is None:
            self.current_track = self._no_media_text
        else:
            title, artist = track_info
            if not title:
                title = i18n.get_text(self.lang_code, "unknown")
            if not artist:
                artist = i18n.get_text(self.lang_code, "unknown_artist")

            fmt = config_manager.get("chatbox_format")
            try:
                self.current_track = fmt.replace("{name}", title).replace(
                    "{artist}", artist
                )
            except Exception as e:
                self.current_track = f"{title} - {artist}"
                print(f"Error formatting string: {e}")

    # ------------------------------------------------------------------
    # UI update loop
    # ------------------------------------------------------------------

    def update_ui(self):
        """Polled by the main thread to sync UI state and send OSC updates."""
        if self._quitting:
            return
        if self.current_track != self.last_track:
            self.last_track = self.current_track
            self.track_label.configure(text=self.current_track)

            if self.current_track != self._no_media_text:
                osc_runner.send_chatbox(self.current_track)
            else:
                # Media stopped — cancel any running display timer and clear chatbox.
                osc_runner.clear_chatbox()

        self.after(500, self.update_ui)


if __name__ == "__main__":
    app = App()
    app.mainloop()
