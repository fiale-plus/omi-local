import io
import logging
import os
import struct
from typing import Optional, Tuple

import numpy as np

logger = logging.getLogger(__name__)

# faster-whisper is lazily imported to avoid hard dependency when not in LOCAL_MODE
_local_model = None
_local_model_size = None
_local_model_type = None


def _get_local_model(
    model_size: str = "small",
    model_type: str = "default",
    compute_type: str = "default",
):
    """Lazily load and cache the faster-whisper model."""
    global _local_model, _local_model_size, _local_model_type

    if _local_model is None or _local_model_size != model_size or _local_model_type != model_type:
        from faster_whisper import WhisperModel

        logger.info(f"Loading faster-whisper model: {model_size} ({model_type}, compute={compute_type})")
        _local_model = WhisperModel(
            model_size,
            device="cpu",
            compute_type=compute_type if compute_type != "default" else "int8",
        )
        _local_model_size = model_size
        _local_model_type = model_type
        logger.info("faster-whisper model loaded successfully")

    return _local_model


def is_local_stt_enabled() -> bool:
    """Check if LOCAL_MODE is enabled and local STT is configured."""
    return os.getenv("LOCAL_MODE", "").lower() in ("1", "true", "yes")


def transcribe_pcm_with_local_stt(
    audio_bytes: bytes,
    language: str = "en",
    sample_rate: int = 16000,
    channels: int = 1,
    model_size: str = "small",
    model_type: str = "default",
    compute_type: str = "default",
) -> Tuple[Optional[str], Optional[str]]:
    """
    Transcribe PCM audio bytes using local faster-whisper.

    Returns:
        Tuple of (transcript_text, detected_language)
    """
    try:
        # Convert PCM bytes to numpy array
        pcm_data = np.frombuffer(audio_bytes, dtype=np.int16)

        # Handle stereo by mixing down to mono
        if channels > 1:
            pcm_data = pcm_data.reshape(-1, channels)
            pcm_data = np.mean(pcm_data, axis=1, dtype=np.int16)

        # Ensure proper sample rate (simple resampling by decimation/duplication)
        if sample_rate != 16000:
            ratio = sample_rate // 16000
            if ratio > 1:
                pcm_data = pcm_data[::ratio]
            elif ratio < 1:
                pcm_data = np.repeat(pcm_data, -ratio)

        # Normalize to float32 for faster-whisper
        audio_float32 = pcm_data.astype(np.float32) / 32768.0

        # Load model and transcribe
        model = _get_local_model(model_size, model_type, compute_type)

        # Map language codes - faster-whisper uses simpler codes
        lang_map = {
            "en-US": "en",
            "en-GB": "en",
            "en-AU": "en",
            "es-419": "es",
            "pt-BR": "pt",
            "pt-PT": "pt",
        }
        whisper_lang = lang_map.get(language, language.split("-")[0] if "-" in language else language)

        # Run transcription
        segments, info = model.transcribe(
            audio_float32,
            language=whisper_lang,
            task="transcribe",
            beam_size=5,
            vad_filter=False,
        )

        # Collect transcript
        transcript_parts = []
        for segment in segments:
            if segment.text:
                transcript_parts.append(segment.text.strip())

        if not transcript_parts:
            return None, info.language if info else whisper_lang

        transcript = " ".join(transcript_parts)
        detected = info.language if info else whisper_lang

        return transcript, detected

    except Exception as e:
        logger.error(f"Local STT transcription failed: {e}")
        raise RuntimeError(f"Local STT transcription failed: {e}")


def transcribe_wav_bytes_with_local_stt(
    wav_bytes: bytes,
    language: str = "en",
    model_size: str = "small",
    model_type: str = "default",
    compute_type: str = "default",
) -> Tuple[Optional[str], Optional[str]]:
    """
    Transcribe WAV audio bytes using local faster-whisper.

    Returns:
        Tuple of (transcript_text, detected_language)
    """
    try:
        # Parse WAV bytes to get sample rate and channels
        with wave.open(io.BytesIO(wav_bytes), "rb") as wav_file:
            sample_rate = wav_file.getframerate()
            channels = wav_file.getnchannels()
            frames = wav_file.readframes(wav_file.getnframes())

        # Convert to numpy array
        pcm_data = np.frombuffer(frames, dtype=np.int16)

        # Handle stereo by mixing down to mono
        if channels > 1:
            pcm_data = pcm_data.reshape(-1, channels)
            pcm_data = np.mean(pcm_data, axis=1, dtype=np.int16)

        # Resample to 16kHz if needed
        if sample_rate != 16000:
            ratio = sample_rate // 16000
            if ratio > 1:
                pcm_data = pcm_data[::ratio]
            elif ratio < 1:
                pcm_data = np.repeat(pcm_data, -ratio)

        # Normalize to float32
        audio_float32 = pcm_data.astype(np.float32) / 32768.0

        # Load model and transcribe
        model = _get_local_model(model_size, model_type, compute_type)

        # Map language codes
        lang_map = {
            "en-US": "en",
            "en-GB": "en",
            "en-AU": "en",
            "es-419": "es",
            "pt-BR": "pt",
            "pt-PT": "pt",
        }
        whisper_lang = lang_map.get(language, language.split("-")[0] if "-" in language else language)

        # Run transcription
        segments, info = model.transcribe(
            audio_float32,
            language=whisper_lang,
            task="transcribe",
            beam_size=5,
            vad_filter=False,
        )

        # Collect transcript
        transcript_parts = []
        for segment in segments:
            if segment.text:
                transcript_parts.append(segment.text.strip())

        if not transcript_parts:
            return None, info.language if info else whisper_lang

        transcript = " ".join(transcript_parts)
        detected = info.language if info else whisper_lang

        return transcript, detected

    except Exception as e:
        logger.error(f"Local STT WAV transcription failed: {e}")
        raise RuntimeError(f"Local STT WAV transcription failed: {e}")