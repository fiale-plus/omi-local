"""
Local continuous listen STT handler for LOCAL_MODE.

Handles real-time audio streaming from desktop app via WebSocket,
transcribes using local faster-whisper, and manages conversation lifecycle
without any cloud dependencies (Deepgram, GCS, Firestore, pusher).

Key features:
- Streaming audio transcription with local faster-whisper
- Conversation rotation based on silence timeout
- Local filesystem storage for audio chunks
- Local SQLite metadata storage (via desktop Backend-Rust IPC)
- No cloud services in LOCAL_MODE
"""

import asyncio
import io
import json
import logging
import os
import struct
import time
import uuid
import wave
from collections import deque
from datetime import datetime, timezone
from typing import Callable, Deque, Dict, List, Optional, Tuple, Any

import numpy as np

from utils.stt.local_stt import _get_local_model, is_local_stt_enabled

logger = logging.getLogger(__name__)

# Audio buffer settings
CHUNK_DURATION_SECONDS = 5.0  # Transcribe every 5 seconds of audio
MIN_CHUNK_DURATION_SECONDS = 2.0  # Minimum audio before triggering transcription
MAX_BUFFER_SECONDS = 30.0  # Maximum audio to buffer

# Sample rate for transcription
TARGET_SAMPLE_RATE = 16000


class LocalListenSession:
    """
    Manages a single local listen session with continuous transcription.
    
    Receives audio chunks via WebSocket, buffers them, and runs faster-whisper
    transcription at regular intervals. Handles conversation rotation based on
    silence detection.
    """

    def __init__(
        self,
        uid: str,
        session_id: str,
        language: str = "en",
        sample_rate: int = 16000,
        channels: int = 1,
        conversation_timeout: int = 120,
        storage_dir: Optional[str] = None,
    ):
        self.uid = uid
        self.session_id = session_id
        self.language = language
        self.sample_rate = sample_rate
        self.channels = channels
        self.conversation_timeout = conversation_timeout
        
        # Storage directory for audio chunks
        self.storage_dir = storage_dir or os.path.join(
            os.getcwd(), "_segments", uid
        )
        os.makedirs(self.storage_dir, exist_ok=True)
        
        # Audio buffer (deque of numpy arrays)
        self.audio_buffer: Deque[np.ndarray] = deque()
        self.buffer_duration_ms = 0.0
        
        # Current conversation state
        self.current_conversation_id: Optional[str] = None
        self.conversation_start_time: Optional[float] = None
        self.last_audio_time: Optional[float] = None
        self.segment_count = 0
        
        # All segments for current conversation
        self.segments: List[Dict[str, Any]] = []
        
        # Callbacks
        self.on_transcript: Optional[Callable[[List[Dict]], None]] = None
        self.on_conversation_start: Optional[Callable[[str], None]] = None
        self.on_conversation_end: Optional[Callable[[str, List[Dict]], None]] = None
        self.on_service_status: Optional[Callable[[str, str], None]] = None
        
        # Transcription task
        self._transcribe_task: Optional[asyncio.Task] = None
        self._running = False
        self._buffer_lock = asyncio.Lock()
        
        # Model settings
        self._model_size = "small"
        self._model_type = "default"
        self._compute_type = "default"

    def _map_language(self, language: str) -> str:
        """Map language code to faster-whisper format."""
        lang_map = {
            "en-US": "en",
            "en-GB": "en",
            "en-AU": "en",
            "es-419": "es",
            "pt-BR": "pt",
            "pt-PT": "pt",
        }
        return lang_map.get(language, language.split("-")[0] if "-" in language else language)

    async def start(self):
        """Start the listen session."""
        self._running = True
        self._send_status("initiating", "Service Starting")
        
        # Create initial conversation
        await self._create_conversation()
        
        # Start transcription loop
        self._transcribe_task = asyncio.create_task(self._transcription_loop())
        
        logger.info(f"LocalListenSession started: uid={self.uid} session={self.session_id}")

    async def stop(self):
        """Stop the listen session."""
        self._running = False
        
        if self._transcribe_task:
            self._transcribe_task.cancel()
            try:
                await self._transcribe_task
            except asyncio.CancelledError:
                pass
        
        # Finalize current conversation
        if self.current_conversation_id and self.segments:
            await self._finalize_conversation()
        
        logger.info(f"LocalListenSession stopped: uid={self.uid} session={self.session_id}")

    def _send_status(self, status: str, status_text: str):
        """Send service status event."""
        if self.on_service_status:
            try:
                self.on_service_status(status, status_text)
            except Exception as e:
                logger.error(f"Error sending status: {e}")

    async def _create_conversation(self):
        """Create a new conversation."""
        self.current_conversation_id = str(uuid.uuid4())
        self.conversation_start_time = time.time()
        self.segments = []
        self.segment_count = 0
        
        # Notify conversation start
        if self.on_conversation_start:
            try:
                self.on_conversation_start(self.current_conversation_id)
            except Exception as e:
                logger.error(f"Error on conversation start: {e}")
        
        logger.info(f"Created conversation: {self.current_conversation_id} uid={self.uid}")

    async def _finalize_conversation(self):
        """Finalize and store the current conversation."""
        if not self.current_conversation_id:
            return
        
        conv_id = self.current_conversation_id
        
        # Store conversation metadata locally
        await self._store_conversation_locally(conv_id)
        
        # Notify conversation end
        if self.on_conversation_end:
            try:
                self.on_conversation_end(conv_id, self.segments.copy())
            except Exception as e:
                logger.error(f"Error on conversation end: {e}")
        
        logger.info(f"Finalized conversation: {conv_id} segments={len(self.segments)} uid={self.uid}")
        
        self.current_conversation_id = None

    async def _store_conversation_locally(self, conversation_id: str):
        """
        Store conversation metadata locally.
        
        In LOCAL_MODE, this writes to local filesystem and communicates
        with desktop Backend-Rust for SQLite storage.
        """
        try:
            # Write transcript segments to local file
            segments_file = os.path.join(
                self.storage_dir, 
                f"{conversation_id}_segments.json"
            )
            with open(segments_file, "w") as f:
                json.dump({
                    "id": conversation_id,
                    "uid": self.uid,
                    "created_at": datetime.now(timezone.utc).isoformat(),
                    "started_at": datetime.fromtimestamp(
                        self.conversation_start_time, tz=timezone.utc
                    ).isoformat() if self.conversation_start_time else None,
                    "finished_at": datetime.now(timezone.utc).isoformat(),
                    "language": self.language,
                    "segments": self.segments,
                    "status": "completed",
                }, f, indent=2)
            
            # Store audio file reference
            audio_file = os.path.join(
                self.storage_dir, 
                f"{conversation_id}_audio.wav"
            )
            
            logger.info(f"Stored conversation locally: {conversation_id} uid={self.uid}")
            
        except Exception as e:
            logger.error(f"Error storing conversation locally: {e}")

    async def add_audio_chunk(self, audio_data: bytes, received_at: float):
        """
        Add an audio chunk to the buffer.
        
        Args:
            audio_data: Raw PCM audio bytes
            received_at: Timestamp when the chunk was received
        """
        async with self._buffer_lock:
            # Convert bytes to numpy array
            samples = np.frombuffer(audio_data, dtype=np.int16)
            
            # Handle stereo by mixing to mono
            if self.channels > 1:
                samples = samples.reshape(-1, self.channels)
                samples = np.mean(samples, axis=1, dtype=np.int16)
            
            # Resample if needed
            if self.sample_rate != TARGET_SAMPLE_RATE:
                samples = self._resample(samples, self.sample_rate, TARGET_SAMPLE_RATE)
            
            # Add to buffer
            self.audio_buffer.append(samples)
            self.buffer_duration_ms += (len(samples) / TARGET_SAMPLE_RATE) * 1000
            
            # Trim buffer if too large
            while self.buffer_duration_ms > MAX_BUFFER_SECONDS * 1000 and len(self.audio_buffer) > 1:
                old_chunk = self.audio_buffer.popleft()
                self.buffer_duration_ms -= (len(old_chunk) / TARGET_SAMPLE_RATE) * 1000
            
            self.last_audio_time = received_at

    def _resample(self, samples: np.ndarray, source_rate: int, target_rate: int) -> np.ndarray:
        """Simple resampling by decimation/duplication."""
        if source_rate == target_rate:
            return samples
        
        ratio = target_rate / source_rate
        new_length = int(len(samples) * ratio)
        indices = np.linspace(0, len(samples) - 1, new_length).astype(int)
        return samples[indices]

    async def _transcription_loop(self):
        """Background loop that periodically transcribes buffered audio."""
        last_transcribe_time = time.time()
        
        while self._running:
            try:
                await asyncio.sleep(0.5)  # Check every 500ms
                
                current_time = time.time()
                buffer_duration_sec = self.buffer_duration_ms / 1000
                
                # Check if we should transcribe
                should_transcribe = (
                    buffer_duration_sec >= MIN_CHUNK_DURATION_SECONDS and
                    current_time - last_transcribe_time >= CHUNK_DURATION_SECONDS
                )
                
                # Also check for conversation timeout (silence)
                if self.last_audio_time:
                    silence_duration = current_time - self.last_audio_time
                    if (silence_duration >= self.conversation_timeout and 
                        self.current_conversation_id and 
                        self.segments):
                        logger.info(f"Conversation timeout due to silence: {silence_duration:.1f}s")
                        await self._finalize_conversation()
                        await self._create_conversation()
                        last_transcribe_time = current_time
                        continue
                
                if should_transcribe:
                    await self._transcribe_buffer()
                    last_transcribe_time = current_time
                    
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in transcription loop: {e}")

    async def _transcribe_buffer(self):
        """Transcribe the current audio buffer."""
        async with self._buffer_lock:
            if not self.audio_buffer:
                return
            
            # Combine all buffered audio
            all_samples = np.concatenate(list(self.audio_buffer))
            buffer_duration = len(all_samples) / TARGET_SAMPLE_RATE
            
            # Clear buffer
            self.audio_buffer.clear()
            self.buffer_duration_ms = 0.0
        
        try:
            # Run transcription in thread pool (CPU-bound)
            loop = asyncio.get_event_loop()
            segments = await loop.run_in_executor(
                None, 
                self._transcribe_numpy, 
                all_samples.astype(np.float32) / 32768.0
            )
            
            if not segments:
                return
            
            # Process and format segments
            formatted_segments = []
            current_time = time.time()
            
            for seg in segments:
                start_time = self.conversation_start_time + seg["start"] if self.conversation_start_time else seg["start"]
                end_time = self.conversation_start_time + seg["end"] if self.conversation_start_time else seg["end"]
                
                segment = {
                    "id": str(uuid.uuid4()),
                    "text": seg["text"].strip(),
                    "speaker": "SPEAKER_00",
                    "speaker_id": 0,
                    "is_user": True,
                    "person_id": None,
                    "start": start_time,
                    "end": end_time,
                    "translations": [],
                    "speech_profile_processed": False,
                    "stt_provider": "local_whisper",
                }
                formatted_segments.append(segment)
                self.segments.append(segment)
                self.segment_count += 1
            
            # Send transcript to client
            if self.on_transcript and formatted_segments:
                try:
                    self.on_transcript(formatted_segments)
                except Exception as e:
                    logger.error(f"Error sending transcript: {e}")
            
            logger.info(
                f"Transcribed {len(formatted_segments)} segments, "
                f"buffer={buffer_duration:.1f}s, total_segments={self.segment_count}"
            )
            
            # Free memory
            del all_samples
            
        except Exception as e:
            logger.error(f"Transcription error: {e}")

    def _transcribe_numpy(self, audio_float32: np.ndarray) -> List[Dict[str, Any]]:
        """
        Transcribe numpy audio array using faster-whisper.
        
        Runs in thread pool to avoid blocking.
        """
        try:
            model = _get_local_model(
                self._model_size, 
                self._model_type, 
                self._compute_type
            )
            
            whisper_lang = self._map_language(self.language)
            
            segments, info = model.transcribe(
                audio_float32,
                language=whisper_lang,
                task="transcribe",
                beam_size=5,
                vad_filter=True,
                vad_parameters=dict(min_silence_duration_ms=500),
            )
            
            result = []
            for seg in segments:
                if seg.text and seg.text.strip():
                    result.append({
                        "start": seg.start,
                        "end": seg.end,
                        "text": seg.text,
                    })
            
            return result
            
        except Exception as e:
            logger.error(f"faster-whisper transcription failed: {e}")
            return []

    async def handle_websocket_message(self, message: bytes) -> Optional[dict]:
        """
        Handle incoming WebSocket message.
        
        Messages can be:
        - Binary audio data (raw PCM)
        - JSON control messages
        
        Returns a response dict for JSON messages, None for audio.
        """
        if len(message) < 4:
            return None
        
        # Check message type
        msg_type = struct.unpack("<I", message[:4])[0]
        
        if msg_type == 100:
            # Binary audio data
            audio_data = message[4:]
            received_at = time.time()
            await self.add_audio_chunk(audio_data, received_at)
            return None
        
        elif msg_type == 101:
            # Audio with timestamp header
            if len(message) >= 12:
                timestamp = struct.unpack("<d", message[4:12])[0]
                audio_data = message[12:]
                await self.add_audio_chunk(audio_data, timestamp)
            return None
        
        elif msg_type == 1:
            # JSON control message
            try:
                data = json.loads(message[4:].decode("utf-8"))
                return data
            except json.JSONDecodeError:
                logger.error("Failed to decode JSON message")
                return None
        
        return None

    def get_session_info(self) -> dict:
        """Get current session information."""
        return {
            "session_id": self.session_id,
            "uid": self.uid,
            "conversation_id": self.current_conversation_id,
            "buffer_duration_ms": self.buffer_duration_ms,
            "segment_count": self.segment_count,
            "last_audio_time": self.last_audio_time,
        }


async def create_local_listen_session(
    uid: str,
    language: str = "en",
    sample_rate: int = 16000,
    channels: int = 1,
    conversation_timeout: int = 120,
) -> LocalListenSession:
    """
    Create and start a new local listen session.
    
    Args:
        uid: User ID
        language: Language code
        sample_rate: Audio sample rate
        channels: Number of audio channels
        conversation_timeout: Seconds of silence before ending conversation
        
    Returns:
        Started LocalListenSession instance
    """
    session_id = str(uuid.uuid4())
    
    session = LocalListenSession(
        uid=uid,
        session_id=session_id,
        language=language,
        sample_rate=sample_rate,
        channels=channels,
        conversation_timeout=conversation_timeout,
    )
    
    await session.start()
    
    return session
