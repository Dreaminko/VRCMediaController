import threading

from pythonosc.dispatcher import Dispatcher
from pythonosc.osc_server import BlockingOSCUDPServer
from pythonosc.udp_client import SimpleUDPClient

import media_control
from config import config_manager

# OSC Server endpoints
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 9001
CLIENT_HOST = "127.0.0.1"
CLIENT_PORT = 9000

# Client instance initialized later
_client = None
_server = None
_server_thread = None

# Track states to trigger on False -> True transition
_state_playpause = False
_state_next = False
_state_prev = False


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


def start_osc():
    """Initializes the OSC Client and Server on a background thread."""
    global _client, _server, _server_thread

    # 1. Start OSC Client
    _client = SimpleUDPClient(CLIENT_HOST, CLIENT_PORT)
    print(f"[OSC Client] Ready to send to {CLIENT_HOST}:{CLIENT_PORT}")

    # 2. Start OSC Server
    dispatcher = Dispatcher()
    dispatcher.map("/avatar/parameters/Media_PlayPause", handle_playpause)
    dispatcher.map("/avatar/parameters/Media_Next", handle_next)
    dispatcher.map("/avatar/parameters/Media_Prev", handle_prev)

    try:
        # BlockingOSCUDPServer processes messages sequentially on a single thread.
        # OSC control messages (button presses) are infrequent, so there is no need
        # for the per-request thread overhead of ThreadingOSCUDPServer.
        _server = BlockingOSCUDPServer((SERVER_HOST, SERVER_PORT), dispatcher)
        print(f"[OSC Server] Listening on {SERVER_HOST}:{SERVER_PORT}")
    except OSError as e:
        print(
            f"[OSC Error] Failed to bind to {SERVER_HOST}:{SERVER_PORT}. Is another instance running?"
        )
        print(e)
        return False

    def _serve():
        _server.serve_forever()

    _server_thread = threading.Thread(target=_serve, daemon=True, name="OSCServer")
    _server_thread.start()
    return True


def stop_osc():
    """Shuts down the OSC server cleanly."""
    global _server
    if _server is not None:
        try:
            _server.shutdown()
            print("[OSC Server] Shut down.")
        except Exception as e:
            print(f"[OSC Server] Error during shutdown: {e}")


def send_chatbox(text):
    """Sends a formatted string to VRChat chatbox via OSC."""
    if not config_manager.get("chatbox_enabled"):
        return

    if _client:
        try:
            _client.send_message("/chatbox/input", [text, True])
            print(f"[OSC] Sent to chatbox: {text}")
        except Exception as e:
            print(f"[OSC] Error sending Chatbox message: {e}")


def clear_chatbox():
    """Clears the VRChat chatbox via OSC."""
    if _client:
        try:
            _client.send_message("/chatbox/input", ["", True])
            print("[OSC] Cleared chatbox")
        except Exception as e:
            print(f"[OSC] Error clearing Chatbox message: {e}")
