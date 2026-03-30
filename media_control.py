import asyncio
import threading
import traceback
import winsdk.windows.media.control as wmc

# Keep a reference to the active loop for threading interactions
_event_loop = None

async def _get_current_session():
    """Helper to get the current media session."""
    manager = await wmc.GlobalSystemMediaTransportControlsSessionManager.request_async()
    if manager:
        return manager.get_current_session()
    return None

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

async def _polling_task(callback):
    """
    Periodically polls Windows SMTC for track changes.
    Calls 'callback(title, artist)' when a change is detected.
    """
    last_title = None
    last_artist = None
    
    while True:
        try:
            session = await _get_current_session()
            if session:
                properties = await session.try_get_media_properties_async()
                if properties:
                    title = properties.title
                    artist = properties.artist
                    
                    if title != last_title or artist != last_artist:
                        last_title = title
                        last_artist = artist
                        callback((title, artist))
            else:
                if last_title is not None:
                    last_title = None
                    last_artist = None
                    callback(None)
                    
        except Exception as e:
            print(f"[MediaControl] Error in polling loop: {e}")
            traceback.print_exc()

        await asyncio.sleep(1.0) # Poll every 1 second
        
def start_media_polling(callback):
    """
    Starts media polling in a background asyncio event loop.
    Returns the thread object for lifecycle management.
    """
    def _run_loop():
        global _event_loop
        _event_loop = asyncio.new_event_loop()
        asyncio.set_event_loop(_event_loop)
        _event_loop.run_until_complete(_polling_task(callback))

    thread = threading.Thread(target=_run_loop, daemon=True, name="MediaPoller")
    thread.start()
    return thread
