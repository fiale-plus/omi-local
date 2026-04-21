"""
Local continuous listen WebSocket transport for LOCAL_MODE.

Provides a self-contained /v4/listen WebSocket endpoint that:
- Accepts audio from desktop app via WebSocket
- Transcribes via local faster-whisper (no Deepgram)
- Handles conversation rotation locally
- Writes audio to local filesystem storage
- Writes conversation metadata to local SQLite via desktop Backend-Rust IPC

No cloud services (Deepgram, GCS, Firestore, pusher) are used in LOCAL_MODE.
"""

import asyncio
import json
import logging
import os
import struct
import time
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional, Set

from fastapi import APIRouter, Depends, WebSocket, WebSocketDisconnect
from starlette.websockets import WebSocketState

from utils.stt.local_listen import LocalListenSession, create_local_listen_session
from utils.stt.local_stt import is_local_stt_enabled

# LOCAL_MODE LLM check - imported lazily to avoid environment dependency issues
def _is_local_llm_enabled() -> bool:
    try:
        from utils.llm.clients import is_local_llm_enabled as _check
        return _check()
    except Exception:
        return False

logger = logging.getLogger(__name__)

router = APIRouter()

# Storage directory for local conversations
LOCAL_STORAGE_DIR = os.environ.get("LOCAL_STORAGE_DIR", "_local_data")

# Track active sessions
_active_sessions: Dict[str, LocalListenSession] = {}
_session_tasks: Dict[str, asyncio.Task] = {}


def _get_local_storage_dir(uid: str) -> str:
    """Get storage directory for a user's local data."""
    storage_dir = os.path.join(LOCAL_STORAGE_DIR, uid, "conversations")
    os.makedirs(storage_dir, exist_ok=True)
    return storage_dir


async def _send_json(websocket: WebSocket, data: dict):
    """Send JSON message to WebSocket."""
    try:
        await websocket.send_json(data)
    except Exception as e:
        logger.error(f"Error sending JSON: {e}")


async def _send_text(websocket: WebSocket, text: str):
    """Send text message to WebSocket."""
    try:
        await websocket.send_text(text)
    except Exception as e:
        logger.error(f"Error sending text: {e}")


def _build_message_event(event_type: str, **kwargs) -> dict:
    """Build a message event dict."""
    return {"type": event_type, **kwargs}


class LocalListenTransport:
    """
    WebSocket transport for local listen streaming.
    
    Wraps LocalListenSession and handles WebSocket protocol:
    - Receives audio binary messages
    - Sends transcript JSON events
    - Handles control messages (start/stop/config)
    """

    def __init__(
        self,
        websocket: WebSocket,
        uid: str,
        session: LocalListenSession,
    ):
        self.websocket = websocket
        self.uid = uid
        self.session = session
        self._running = False
        self._receive_task: Optional[asyncio.Task] = None
        
        # Set up session callbacks
        session.on_transcript = self._handle_transcript
        session.on_conversation_start = self._handle_conversation_start
        session.on_conversation_end = self._handle_conversation_end
        session.on_service_status = self._handle_service_status

    async def start(self):
        """Start the transport."""
        self._running = True
        self._receive_task = asyncio.create_task(self._receive_loop())

    async def stop(self):
        """Stop the transport."""
        self._running = False
        
        if self._receive_task:
            self._receive_task.cancel()
            try:
                await self._receive_task
            except asyncio.CancelledError:
                pass
        
        await self.session.stop()

    async def _receive_loop(self):
        """Receive messages from WebSocket."""
        try:
            while self._running and self.websocket.client_state == WebSocketState.CONNECTED:
                try:
                    data = await asyncio.wait_for(
                        self.websocket.receive(),
                        timeout=1.0
                    )
                except asyncio.TimeoutError:
                    continue
                
                if "text" in data:
                    await self._handle_text_message(data["text"])
                elif "bytes" in data:
                    await self._handle_binary_message(data["bytes"])
                elif "disconnect" in data:
                    break
                    
        except WebSocketDisconnect:
            logger.info(f"WebSocket disconnected: uid={self.uid}")
        except Exception as e:
            logger.error(f"Receive loop error: {e}")
        finally:
            self._running = False

    async def _handle_text_message(self, text: str):
        """Handle text message (JSON control message)."""
        try:
            msg = json.loads(text)
            msg_type = msg.get("type")
            
            if msg_type == "config":
                # Configuration update
                logger.info(f"Config update: {msg}")
                
            elif msg_type == "ping":
                await _send_text(self.websocket, "pong")
                
            elif msg_type == "stop":
                await _send_json(self.websocket, _build_message_event("stopped"))
                self._running = False
                
        except json.JSONDecodeError:
            logger.error(f"Invalid JSON: {text}")

    async def _handle_binary_message(self, data: bytes):
        """Handle binary message (audio data)."""
        await self.session.handle_websocket_message(data)

    def _handle_transcript(self, segments: List[dict]):
        """Handle transcript segments from session."""
        if self._running and self.websocket.client_state == WebSocketState.CONNECTED:
            asyncio.create_task(_send_json(
                self.websocket,
                _build_message_event("transcript", segments=segments)
            ))

    def _handle_conversation_start(self, conversation_id: str):
        """Handle conversation start event."""
        if self._running and self.websocket.client_state == WebSocketState.CONNECTED:
            asyncio.create_task(_send_json(
                self.websocket,
                _build_message_event(
                    "conversation_start",
                    conversation_id=conversation_id
                )
            ))

    def _handle_conversation_end(self, conversation_id: str, segments: List[dict]):
        """Handle conversation end event."""
        if self._running and self.websocket.client_state == WebSocketState.CONNECTED:
            asyncio.create_task(_send_json(
                self.websocket,
                _build_message_event(
                    "conversation_end",
                    conversation_id=conversation_id,
                    segment_count=len(segments)
                )
            ))

    def _handle_service_status(self, status: str, status_text: str):
        """Handle service status update."""
        if self._running and self.websocket.client_state == WebSocketState.CONNECTED:
            asyncio.create_task(_send_json(
                self.websocket,
                _build_message_event(
                    "service_status",
                    status=status,
                    status_text=status_text
                )
            ))


@router.websocket("/v4/local/listen")
async def local_listen_handler(
    websocket: WebSocket,
    uid: str,
    language: str = "en",
    sample_rate: int = 16000,
    channels: int = 1,
    conversation_timeout: int = 120,
):
    """
    WebSocket endpoint for local continuous listen in LOCAL_MODE.
    
    This endpoint is only active when LOCAL_MODE is enabled. It provides
    a self-contained transcription service using local faster-whisper
    without any cloud dependencies.
    
    Message Protocol:
    - Binary audio data (4-byte header + PCM16 LE audio)
    - JSON control messages
    
    Response Protocol:
    - JSON events for transcripts, conversation lifecycle, and status
    
    Args:
        uid: User ID (from auth)
        language: Language code (default: en)
        sample_rate: Audio sample rate (default: 16000)
        channels: Number of audio channels (default: 1)
        conversation_timeout: Seconds of silence before ending conversation (default: 120)
    """
    # Verify LOCAL_MODE is enabled
    if not is_local_stt_enabled():
        logger.warning(f"Local listen requested but LOCAL_MODE not enabled")
        await websocket.close(code=1011, reason="Local mode not enabled")
        return
    
    session_id = str(uuid.uuid4())
    logger.info(
        f"local_listen_handler: uid={uid} session={session_id} "
        f"language={language} sample_rate={sample_rate} channels={channels}"
    )
    
    try:
        await websocket.accept()
    except RuntimeError as e:
        logger.error(f"Failed to accept WebSocket: {e}")
        return
    
    # Create local storage directory
    storage_dir = _get_local_storage_dir(uid)
    
    # Create session
    try:
        session = await create_local_listen_session(
            uid=uid,
            language=language,
            sample_rate=sample_rate,
            channels=channels,
            conversation_timeout=conversation_timeout,
        )
    except Exception as e:
        logger.error(f"Failed to create local listen session: {e}")
        await websocket.close(code=1011, reason="Session creation failed")
        return
    
    # Store session
    _active_sessions[session_id] = session
    
    # Create and start transport
    transport = LocalListenTransport(websocket, uid, session)
    
    try:
        await transport.start()
        
        # Send ready event
        await _send_json(
            websocket,
            _build_message_event(
                "ready",
                session_id=session_id,
                status="listening"
            )
        )
        
        # Wait for transport to finish
        while transport._running and websocket.client_state == WebSocketState.CONNECTED:
            await asyncio.sleep(0.5)
            
    except WebSocketDisconnect:
        logger.info(f"WebSocket disconnected: uid={uid} session={session_id}")
    except Exception as e:
        logger.error(f"Transport error: {e}")
    finally:
        await transport.stop()
        _active_sessions.pop(session_id, None)
        logger.info(f"Session cleaned up: uid={uid} session={session_id}")


@router.get("/v4/local/status")
async def local_listen_status(uid: str):
    """
    Get status of local listen service.
    
    Returns information about active sessions and local storage.
    """
    if not is_local_stt_enabled():
        return {
            "enabled": False,
            "reason": "LOCAL_MODE not enabled",
            "active_sessions": 0,
        }
    
    active_sessions = [
        {
            "session_id": sid,
            "info": session.get_session_info(),
        }
        for sid, session in _active_sessions.items()
        if session.uid == uid
    ]
    
    storage_dir = _get_local_storage_dir(uid)
    
    return {
        "enabled": True,
        "active_sessions": len(active_sessions),
        "sessions": active_sessions,
        "storage_dir": storage_dir,
        "local_llm_enabled": _is_local_llm_enabled(),
    }


@router.post("/v4/local/conversations/{conversation_id}/process")
async def local_process_conversation(
    conversation_id: str,
    uid: str,
    language: str = "en",
):
    """
    Trigger local processing for a conversation.
    
    In LOCAL_MODE, this triggers local LLM processing instead of
    routing to pusher service. Uses the local SQLite database for
    storage and local LLM for extraction.
    """
    if not is_local_stt_enabled():
        return {"error": "Local mode not enabled"}
    
    # Check if local processing is available
    try:
        from utils.conversations.local_process_conversation import is_local_processing_available
        if not is_local_processing_available():
            return {
                "error": "Local LLM not configured",
                "detail": "Set LOCAL_LLM_BASE_URL and LOCAL_LLM_MODEL to enable local processing"
            }
    except ImportError:
        return {"error": "Local processing module not available"}
    
    # Find the conversation file
    storage_dir = _get_local_storage_dir(uid)
    segments_file = os.path.join(storage_dir, f"{conversation_id}_segments.json")
    
    if not os.path.exists(segments_file):
        return {"error": "Conversation not found"}
    
    # Load conversation
    with open(segments_file, "r") as f:
        conversation = json.load(f)
    
    # Save to local SQLite first
    try:
        from database.local_db import upsert_conversation
        upsert_conversation(uid, conversation)
    except Exception as e:
        logger.warning(f"Failed to save conversation to local DB: {e}")
    
    # Mark as processing
    conversation["status"] = "processing"
    conversation["processing_started_at"] = datetime.now(timezone.utc).isoformat()
    
    with open(segments_file, "w") as f:
        json.dump(conversation, f, indent=2)
    
    # Trigger local LLM processing in background
    try:
        from utils.conversations.local_process_conversation import process_conversation_locally
        import asyncio
        asyncio.create_task(
            process_conversation_locally(uid, conversation_id, language)
        )
    except Exception as e:
        logger.error(f"Failed to start local processing: {e}")
        return {"error": f"Failed to start processing: {e}"}
    
    return {
        "conversation_id": conversation_id,
        "status": "processing",
    }
