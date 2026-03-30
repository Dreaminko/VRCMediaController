import threading

from pythonosc.dispatcher import Dispatcher
from pythonosc.osc_server import BlockingOSCUDPServer
from pythonosc.udp_client import SimpleUDPClient

from . import media_control
from .config import config_manager

# OSC Server endpoints
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 9001
CLIENT_HOST = "127.0.0.1"
CLIENT_PORT = 9000

# Client / server instances initialized later
_client = None
_server = None
_server_thread = None

# Track states to trigger on False -> True transition
_state_playpause = False
_state_next = False
_state_prev = False

# ---------------------------------------------------------------------------
# Display-mode state
# ---------------------------------------------------------------------------

# How often (seconds) to re-send in persistent mode.
# VRChat chatbox messages auto-expire in ~5 s, so 3 s keeps it visible.
PERSISTENT_INTERVAL = 3.0

_current_chatbox_text = None  # Text currently shown (or None if cleared)
_persistent_timer = None  # Recurring re-send timer (persistent mode)
_timed_clear_timer = None  # One-shot clear timer (timed mode)


# ---------------------------------------------------------------------------
# Timer helpers
# ---------------------------------------------------------------------------


def _cancel_timers():
    """Cancel any active persistent-resend or timed-clear timer."""
    global _persistent_timer, _timed_clear_timer
    if _persistent_timer is not None:
        _persistent_timer.cancel()
        _persistent_timer = None
    if _timed_clear_timer is not None:
        _timed_clear_timer.cancel()
        _timed_clear_timer = None


def _start_persistent_timer():
    """Schedule a re-send after PERSISTENT_INTERVAL seconds."""
    global _persistent_timer
    t = threading.Timer(PERSISTENT_INTERVAL, _persistent_resend)
    t.daemon = True
    t.start()
    _persistent_timer = t


def _persistent_resend():
    """Re-send current chatbox text, then reschedule if still in persistent mode."""
    global _persistent_timer
    _persistent_timer = None

    if not _current_chatbox_text:
        return
    if not config_manager.get("chatbox_enabled"):
        return
    if config_manager.get("chatbox_display_mode") != "persistent":
        # Mode was changed while the timer was in flight — do not reschedule.
        return

    if _client:
        try:
            _client.send_message("/chatbox/input", [_current_chatbox_text, True])
        except Exception as e:
            print(f"[OSC] Persistent resend error: {e}")

    _start_persistent_timer()


def _start_timed_clear_timer(duration):
    """Schedule a chatbox clear after *duration* seconds."""
    global _timed_clear_timer
    t = threading.Timer(float(duration), _do_timed_clear)
    t.daemon = True
    t.start()
    _timed_clear_timer = t


def _do_timed_clear():
    """Clear the chatbox when the timed duration expires."""
    global _timed_clear_timer, _current_chatbox_text
    _timed_clear_timer = None
    _current_chatbox_text = None
    if _client:
        try:
            _client.send_message("/chatbox/input", ["", True])
            print("[OSC] Timed display expired — chatbox cleared.")
        except Exception as e:
            print(f"[OSC] Timed clear error: {e}")


# ---------------------------------------------------------------------------
# OSC message handlers
# ---------------------------------------------------------------------------


def handle_playpause(address, *args):
    global _state_playpause
    if args:
        is_true = bool(args[0])
        print(f"[OSC] Recv play_pause: {is_true}")
        if is_true and not _state_playpause:
            media_control.toggle_play_pause()
        _state_playpause = is_true


def handle_next(address, *args):
    global _state_next
    if args:
        is_true = bool(args[0])
        print(f"[OSC] Recv skip_next: {is_true}")
        if is_true and not _state_next:
            media_control.skip_next()
        _state_next = is_true


def handle_prev(address, *args):
    global _state_prev
    if args:
        is_true = bool(args[0])
        print(f"[OSC] Recv skip_prev: {is_true}")
        if is_true and not _state_prev:
            media_control.skip_previous()
        _state_prev = is_true


# ---------------------------------------------------------------------------
# Lifecycle
# ---------------------------------------------------------------------------


def start_osc():
    """Initialise the OSC client and server on a background thread."""
    global _client, _server, _server_thread

    # 1. Start OSC client
    _client = SimpleUDPClient(CLIENT_HOST, CLIENT_PORT)
    print(f"[OSC Client] Ready to send to {CLIENT_HOST}:{CLIENT_PORT}")

    # 2. Build dispatcher
    dispatcher = Dispatcher()
    dispatcher.map("/avatar/parameters/Media_PlayPause", handle_playpause)
    dispatcher.map("/avatar/parameters/Media_Next", handle_next)
    dispatcher.map("/avatar/parameters/Media_Prev", handle_prev)

    # 3. Start OSC server
    try:
        _server = BlockingOSCUDPServer((SERVER_HOST, SERVER_PORT), dispatcher)
        print(f"[OSC Server] Listening on {SERVER_HOST}:{SERVER_PORT}")
    except OSError as e:
        print(
            f"[OSC Error] Failed to bind to {SERVER_HOST}:{SERVER_PORT}. "
            "Is another instance running?"
        )
        print(e)
        return False

    _server_thread = threading.Thread(
        target=_server.serve_forever, daemon=True, name="OSCServer"
    )
    _server_thread.start()
    return True


def stop_osc():
    """Shut down the OSC server and cancel any active display timers."""
    _cancel_timers()
    global _server
    if _server is not None:
        try:
            _server.shutdown()
            print("[OSC Server] Shut down.")
        except Exception as e:
            print(f"[OSC Server] Error during shutdown: {e}")


# ---------------------------------------------------------------------------
# Chatbox helpers
# ---------------------------------------------------------------------------


def send_chatbox(text):
    """Send *text* to VRChat chatbox and start the appropriate display timer.

    Persistent mode — the message is re-sent every PERSISTENT_INTERVAL seconds
    so it stays visible until the track changes or the chatbox is cleared.

    Timed mode — the message is sent once, then cleared after the configured
    duration (chatbox_display_duration seconds).
    """
    global _current_chatbox_text

    if not config_manager.get("chatbox_enabled"):
        return

    # Cancel any previous timer before starting a fresh cycle.
    _cancel_timers()
    _current_chatbox_text = text

    if _client:
        try:
            _client.send_message("/chatbox/input", [text, True])
            print(f"[OSC] Sent to chatbox: {text}")
        except Exception as e:
            print(f"[OSC] Error sending chatbox message: {e}")

    mode = config_manager.get("chatbox_display_mode") or "persistent"
    if mode == "persistent":
        _start_persistent_timer()
    else:
        duration = config_manager.get("chatbox_display_duration") or 10
        _start_timed_clear_timer(duration)


def clear_chatbox():
    """Clear the VRChat chatbox and cancel any active display timers."""
    global _current_chatbox_text
    _cancel_timers()
    _current_chatbox_text = None
    if _client:
        try:
            _client.send_message("/chatbox/input", ["", True])
            print("[OSC] Cleared chatbox.")
        except Exception as e:
            print(f"[OSC] Error clearing chatbox: {e}")
