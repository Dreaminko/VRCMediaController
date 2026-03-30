import asyncio
import threading
import traceback

import winrt.windows.media.control as wmc

# Keep a reference to the active loop for threading interactions
_event_loop = None

# Cached WinRT manager — initialized once, reused for all operations
_manager = None

# Cached current session and its event token
_session = None
_props_token = None
_session_token = None

# Last known track state (used to suppress duplicate callbacks)
_last_title = None
_last_artist = None

# User-supplied callback
_callback = None


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _schedule(coro):
    """Thread-safe helper: schedule a coroutine onto the asyncio event loop."""
    if _event_loop and _event_loop.is_running():
        asyncio.run_coroutine_threadsafe(coro, _event_loop)


async def _fetch_and_notify():
    """Fetch current media properties and fire callback only when changed."""
    global _last_title, _last_artist

    if _session is None:
        if _last_title is not None or _last_artist is not None:
            _last_title = None
            _last_artist = None
            if _callback:
                _callback(None)
        return

    try:
        props = await _session.try_get_media_properties_async()
        if props:
            title = props.title
            artist = props.artist
            if title != _last_title or artist != _last_artist:
                _last_title = title
                _last_artist = artist
                if _callback:
                    _callback((title, artist))
    except Exception as e:
        print(f"[MediaControl] Error fetching properties: {e}")


async def _subscribe_to_session(new_session):
    """Swap to a new session: unsubscribe from the old one, subscribe to the new one."""
    global _session, _props_token

    # Unsubscribe from the previous session's property-changed event
    if _session is not None and _props_token is not None:
        try:
            _session.remove_media_properties_changed(_props_token)
        except Exception:
            pass
        _props_token = None

    _session = new_session

    if _session is not None:
        # Subscribe to property changes on the new session for immediate updates
        _props_token = _session.add_media_properties_changed(_on_props_changed)

    # Immediately fetch track info for the new session
    await _fetch_and_notify()


async def _update_session():
    """Re-query the current session from the cached manager and (re)subscribe."""
    if _manager is None:
        return
    session = _manager.get_current_session()
    await _subscribe_to_session(session)


# ---------------------------------------------------------------------------
# WinRT event handlers — called from COM/WinRT threads, must schedule work
# ---------------------------------------------------------------------------


def _on_props_changed(sender, args):
    """Fired by WinRT when the current session's media properties change."""
    _schedule(_fetch_and_notify())


def _on_session_changed(sender, args):
    """Fired by WinRT when the active media session changes."""
    _schedule(_update_session())


# ---------------------------------------------------------------------------
# Control functions (toggle/skip) — reuse the cached manager
# ---------------------------------------------------------------------------


async def _get_current_session():
    """Return the current session, using the cached manager when available."""
    global _manager
    if _manager is None:
        _manager = (
            await wmc.GlobalSystemMediaTransportControlsSessionManager.request_async()
        )
    return _manager.get_current_session() if _manager else None


async def _toggle_play_pause_async():
    session = await _get_current_session()
    if session:
        await session.try_toggle_play_pause_async()


async def _skip_next_async():
    session = await _get_current_session()
    if session:
        await session.try_skip_next_async()


async def _skip_previous_async():
    session = await _get_current_session()
    if session:
        await session.try_skip_previous_async()


def toggle_play_pause():
    """Sync wrapper to toggle play/pause via SMTC."""
    if _event_loop and _event_loop.is_running():
        asyncio.run_coroutine_threadsafe(_toggle_play_pause_async(), _event_loop)
    else:
        asyncio.run(_toggle_play_pause_async())


def skip_next():
    """Sync wrapper to skip next via SMTC."""
    if _event_loop and _event_loop.is_running():
        asyncio.run_coroutine_threadsafe(_skip_next_async(), _event_loop)
    else:
        asyncio.run(_skip_next_async())


def skip_previous():
    """Sync wrapper to skip previous via SMTC."""
    if _event_loop and _event_loop.is_running():
        asyncio.run_coroutine_threadsafe(_skip_previous_async(), _event_loop)
    else:
        asyncio.run(_skip_previous_async())


# ---------------------------------------------------------------------------
# Main polling / monitoring loop
# ---------------------------------------------------------------------------


async def _monitoring_loop(cb):
    """
    Initialize event-based monitoring, then run a lightweight fallback poll.

    Events handle real-time updates; the 5-second poll is a safety net for
    any missed transitions (e.g. app restart, rapid session switches).
    """
    global _manager, _session_token, _callback

    _callback = cb

    # Initialize the manager exactly once
    _manager = (
        await wmc.GlobalSystemMediaTransportControlsSessionManager.request_async()
    )

    if _manager:
        # Subscribe to session-switch events
        _session_token = _manager.add_current_session_changed(_on_session_changed)
        # Subscribe to the current session and fetch initial state
        await _update_session()

    # Lightweight fallback poll — events cover most cases, this is a safety net
    while True:
        try:
            await _fetch_and_notify()
        except Exception as e:
            print(f"[MediaControl] Fallback poll error: {e}")
            traceback.print_exc()

        await asyncio.sleep(5.0)  # Reduced from 1 s; events handle real-time updates


def start_media_polling(callback):
    """
    Starts event-based media monitoring in a background asyncio event loop.
    Returns the thread object for lifecycle management.
    """

    def _run_loop():
        global _event_loop
        _event_loop = asyncio.new_event_loop()
        asyncio.set_event_loop(_event_loop)
        try:
            _event_loop.run_until_complete(_monitoring_loop(callback))
        except Exception:
            pass
        finally:
            try:
                _event_loop.close()
            except Exception:
                pass

    thread = threading.Thread(target=_run_loop, daemon=True, name="MediaPoller")
    thread.start()
    return thread


def stop_media_polling():
    """Stops the asyncio event loop used for media monitoring."""
    global _event_loop
    if _event_loop is not None and _event_loop.is_running():
        try:
            _event_loop.call_soon_threadsafe(_event_loop.stop)
            print("[MediaControl] Event loop stop requested.")
        except Exception as e:
            print(f"[MediaControl] Error stopping event loop: {e}")
