"""
Filesystem-based storage adapter.

Replaces Google Cloud Storage (GCS) with local filesystem storage for:
- Audio chunks (private cloud sync)
- Speech profiles
- Conversation recordings
- Chat files
- App logos/thumbnails
- Desktop updates

All GCS bucket operations are replaced with equivalent filesystem operations.
GCS URLs (https://storage.googleapis.com/...) are replaced with file:// URLs.
"""

import datetime
import io
import json
import os
import struct
import wave
from pathlib import Path
from typing import List
from concurrent.futures import as_completed

from utils.executors import storage_executor

import opuslib

from database.redis_db import cache_signed_url, get_cached_signed_url
from utils import encryption
from database import users as users_db
import logging

logger = logging.getLogger(__name__)

# Opus encoding constants
OPUS_SAMPLE_RATE = 16000
OPUS_CHANNELS = 1
OPUS_FRAME_DURATION_MS = 20  # 20ms frames (standard for voice)
OPUS_FRAME_SIZE = OPUS_SAMPLE_RATE * OPUS_FRAME_DURATION_MS // 1000  # 320 samples per frame

# Valid private cloud sync extensions (longest first for correct matching)
PRIVATE_CLOUD_EXTENSIONS = ['.batch.enc', '.batch.bin', '.opus.enc', '.opus', '.enc', '.bin']

# Base path for local filesystem storage
LOCAL_STORAGE_ROOT = os.environ.get('LOCAL_STORAGE_PATH', '/tmp/omi-storage')

# Per-bucket subdirectories - maps bucket key to filesystem subdirectory
# For GCS compatibility, paths within bucket are constructed as: bucket_subdir/path
# The bucket key is the logical bucket name used in code
STORAGE_SUBDIRS = {
    'speech_profiles': 'speech-profiles',
    'postprocessing_audio': 'postprocessing',
    'memories_recordings': 'memories-recordings',
    'private_cloud_sync': 'private-cloud-sync',
    'temporal_sync': 'temporal-sync',
    'plugins_logos': 'plugins-logos',
    'app_thumbnails': 'app-thumbnails',
    'chat_files': 'chat-files',
    'desktop_updates': 'desktop-updates',
}


def _get_bucket_path(bucket_key: str, *path_parts) -> Path:
    """Get filesystem path for a bucket and path components.
    
    Args:
        bucket_key: The logical bucket name (key in STORAGE_SUBDIRS)
        *path_parts: Path components to join under the bucket
        
    Returns:
        Full filesystem path
    """
    bucket_subdir = STORAGE_SUBDIRS.get(bucket_key, bucket_key)
    # Filter out empty parts from path components
    filtered_parts = [p for p in path_parts if p]
    # Use Path for proper multi-segment joins — avoids double-slash issues
    # When LOCAL_STORAGE_ROOT already ends with the bucket subdir (e.g. tests patching
    # to /tmp/xyz/private-cloud-sync while bucket_subdir = 'private-cloud-sync'), we avoid
    # doubling by checking if bucket_subdir is already part of LOCAL_STORAGE_ROOT.
    root_str = str(Path(LOCAL_STORAGE_ROOT))
    if root_str.rstrip('/').endswith(bucket_subdir):
        # Bucket subdir already present in root — use root directly with sub-path
        return Path(LOCAL_STORAGE_ROOT) / Path(*filtered_parts)
    return Path(LOCAL_STORAGE_ROOT) / bucket_subdir / Path(*filtered_parts)


def _ensure_dir(path: Path) -> None:
    """Ensure directory exists for a path."""
    path.parent.mkdir(parents=True, exist_ok=True)


def _file_exists(path: Path) -> bool:
    """Check if file exists."""
    return path.exists()


def _write_file(path: Path, data: bytes, content_type: str = None) -> None:
    """Write data to file, ensuring parent directory exists."""
    _ensure_dir(path)
    path.write_bytes(data)


def _read_file(path: Path) -> bytes:
    """Read file contents."""
    return path.read_bytes()


def _delete_file(path: Path) -> bool:
    """Delete a file. Returns True if deleted, False if not found."""
    try:
        path.unlink()
        return True
    except FileNotFoundError:
        return False


def _list_files(prefix: Path) -> List[Path]:
    """List all files under a prefix path (recursive)."""
    if not prefix.exists():
        return []
    if prefix.is_file():
        return [prefix]
    return list(prefix.rglob('*'))


def _list_blobs(prefix: Path):
    """
    List "blobs" (files) under a prefix.
    Returns objects with .name, .exists(), .size, .time_created attributes
    to match GCS blob interface used in this module.
    """

    class BlobInfo:
        def __init__(self, path: Path, prefix_path: Path):
            self.path = path
            self.prefix_path = prefix_path
            self.name = str(path.relative_to(prefix_path)) if path.is_relative_to(prefix_path) else str(path)
            self._stat = None

        def exists(self) -> bool:
            return self.path.exists()

        @property
        def size(self) -> int:
            if self._stat is None:
                self._stat = self.path.stat()
            return self._stat.st_size

        @property
        def time_created(self):
            if self._stat is None:
                self._stat = self.path.stat()
            return datetime.datetime.fromtimestamp(self._stat.st_ctime, tz=datetime.timezone.utc)

        def reload(self):
            self._stat = self.path.stat()

        def delete(self) -> None:
            _delete_file(self.path)

        def download_to_filename(self, dest: Path) -> None:
            dest.write_bytes(_read_file(self.path))

        def download_as_bytes(self) -> bytes:
            return _read_file(self.path)

        def upload_from_filename(self, src: Path) -> None:
            _ensure_dir(self.path)
            self.path.write_bytes(src.read_bytes())

        def upload_from_string(self, data: bytes, content_type: str = None) -> None:
            _write_file(self.path, data, content_type)

        def open(self, mode='rb', content_type=None):
            """Open file for reading/writing (streaming)."""
            _ensure_dir(self.path)
            return open(self.path, mode)

        def generate_signed_url(self, version=None, expiration=None, method=None):
            """Return local file path as a 'URL'."""
            return f'file://{self.path}'

    files = _list_files(prefix)
    return [BlobInfo(f, prefix) for f in files if f.is_file()]


# Bucket handles (simulated GCS bucket interface)
speech_profiles_bucket = 'speech_profiles'
postprocessing_audio_bucket = 'postprocessing_audio'
memories_recordings_bucket = 'memories_recordings'
private_cloud_sync_bucket = 'private_cloud_sync'
syncing_local_bucket = 'temporal_sync'
omi_apps_bucket = 'plugins_logos'
app_thumbnails_bucket = 'app_thumbnails'
chat_files_bucket = 'chat_files'
desktop_updates_bucket = 'desktop_updates'


# *******************************************
# ************* SPEECH PROFILE **************
# *******************************************
def upload_profile_audio(file_path: str, uid: str):
    path = _get_bucket_path(speech_profiles_bucket, uid, 'speech_profile.wav')
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def get_user_has_speech_profile(uid: str, max_age_days: int = None) -> bool:
    path = _get_bucket_path(speech_profiles_bucket, uid, 'speech_profile.wav')
    if not path.exists():
        return False

    if max_age_days is not None:
        age = datetime.datetime.now(datetime.timezone.utc) - datetime.datetime.fromtimestamp(
            path.stat().st_ctime, tz=datetime.timezone.utc
        )
        if age.days > max_age_days:
            return False

    return True


def get_profile_audio_if_exists(uid: str, download: bool = True) -> str:
    path = _get_bucket_path(speech_profiles_bucket, uid, 'speech_profile.wav')
    if path.exists():
        if download:
            file_path = Path(f'_temp/{uid}_speech_profile.wav')
            _ensure_dir(file_path)
            file_path.write_bytes(_read_file(path))
            return str(file_path)
        return f'file://{path}'

    return None


def delete_additional_profile_audio(uid: str, file_name: str) -> None:
    path = _get_bucket_path(speech_profiles_bucket, uid, 'additional_profile_recordings', file_name)
    if path.exists():
        logger.info(f'delete_additional_profile_audio deleting {file_name}')
        path.unlink()


def get_additional_profile_recordings(uid: str, download: bool = False) -> List[str]:
    prefix = _get_bucket_path(speech_profiles_bucket, uid, 'additional_profile_recordings')
    blobs = _list_blobs(prefix)
    if download:
        paths = []
        for blob in blobs:
            file_path = Path(f'_temp/{uid}_{blob.name.split("/")[-1]}')
            _ensure_dir(file_path)
            file_path.write_bytes(_read_file(blob.path))
            paths.append(str(file_path))
        return paths

    return [f'file://{blob.path}' for blob in blobs]


# ********************************************
# ************* PEOPLE PROFILES **************
# ********************************************


def delete_user_person_speech_sample(uid: str, person_id: str, file_name: str) -> None:
    path = _get_bucket_path(speech_profiles_bucket, uid, 'people_profiles', person_id, file_name)
    if path.exists():
        path.unlink()


def delete_user_person_speech_samples(uid: str, person_id: str) -> None:
    prefix = _get_bucket_path(speech_profiles_bucket, uid, 'people_profiles', person_id)
    for blob in _list_blobs(prefix):
        blob.delete()


def upload_person_speech_sample_from_bytes(
    audio_bytes: bytes,
    uid: str,
    person_id: str,
    sample_rate: int = 16000,
) -> str:
    """Upload PCM audio bytes as WAV speech sample. Returns filesystem path."""
    import uuid as uuid_module

    wav_buffer = io.BytesIO()
    with wave.open(wav_buffer, 'wb') as wav_file:
        wav_file.setnchannels(1)
        wav_file.setsampwidth(2)  # 16-bit audio
        wav_file.setframerate(sample_rate)
        wav_file.writeframes(audio_bytes)

    filename = f"{uuid_module.uuid4()}.wav"
    path = _get_bucket_path(speech_profiles_bucket, uid, 'people_profiles', person_id, filename)
    _write_file(path, wav_buffer.getvalue(), 'audio/wav')

    return str(path)


def get_user_people_ids(uid: str) -> List[str]:
    prefix = _get_bucket_path(speech_profiles_bucket, uid, 'people_profiles')
    blobs = _list_blobs(prefix)
    return [blob.name.split("/")[-2] for blob in blobs if '/' in blob.name]


def get_user_person_speech_samples(uid: str, person_id: str, download: bool = False) -> List[str]:
    prefix = _get_bucket_path(speech_profiles_bucket, uid, 'people_profiles', person_id)
    blobs = _list_blobs(prefix)
    if download:
        paths = []
        for blob in blobs:
            file_path = Path(f'_temp/{uid}_person_{blob.name.split("/")[-1]}')
            _ensure_dir(file_path)
            file_path.write_bytes(_read_file(blob.path))
            paths.append(str(file_path))
        return paths

    return [f'file://{blob.path}' for blob in blobs]


def get_speech_sample_signed_urls(paths: List[str]) -> List[str]:
    """
    Generate file:// URLs for speech samples given their filesystem paths.

    Args:
        paths: List of filesystem paths

    Returns:
        List of file:// URLs
    """
    if not paths:
        return []
    return [f'file://{p}' if not p.startswith('file://') else p for p in paths]


# ********************************************
# ************* POST PROCESSING **************
# ********************************************
def upload_postprocessing_audio(file_path: str):
    filename = Path(file_path).name
    path = _get_bucket_path(postprocessing_audio_bucket, filename)
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def delete_postprocessing_audio(file_path: str):
    path = _get_bucket_path(postprocessing_audio_bucket, file_path)
    _delete_file(path)


# ***********************************
# ************* SDCARD **************
# ***********************************


def upload_sdcard_audio(file_path: str):
    filename = Path(file_path).name
    path = _get_bucket_path(postprocessing_audio_bucket, 'sdcard', filename)
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def download_postprocessing_audio(file_path: str, destination_file_path: str):
    path = _get_bucket_path(postprocessing_audio_bucket, file_path)
    Path(destination_file_path).write_bytes(_read_file(path))


# ************************************************
# *********** CONVERSATIONS RECORDINGS ***********
# ************************************************


def upload_conversation_recording(file_path: str, uid: str, conversation_id: str):
    path = _get_bucket_path(memories_recordings_bucket, uid, f'{conversation_id}.wav')
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def get_conversation_recording_if_exists(uid: str, memory_id: str) -> str:
    logger.info(f'get_conversation_recording_if_exists {uid} {memory_id}')
    path = _get_bucket_path(memories_recordings_bucket, uid, f'{memory_id}.wav')
    if path.exists():
        file_path = Path(f'_temp/{memory_id}.wav')
        _ensure_dir(file_path)
        file_path.write_bytes(_read_file(path))
        return str(file_path)
    return None


def delete_all_conversation_recordings(uid: str):
    if not uid:
        return
    prefix = _get_bucket_path(memories_recordings_bucket, uid)
    for blob in _list_blobs(prefix):
        blob.delete()


# ********************************************
# ************* SYNCING FILES **************
# ********************************************
def get_syncing_file_temporal_url(file_path: str):
    path = _get_bucket_path(syncing_local_bucket, file_path)
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def get_syncing_file_temporal_signed_url(file_path: str):
    path = _get_bucket_path(syncing_local_bucket, file_path)
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def delete_syncing_temporal_file(file_path: str):
    path = _get_bucket_path(syncing_local_bucket, file_path)
    _delete_file(path)


# ************************************************
# *********** PRIVATE CLOUD SYNC *****************
# ************************************************


def encode_pcm_to_opus(pcm_data: bytes, sample_rate: int = OPUS_SAMPLE_RATE, channels: int = OPUS_CHANNELS) -> bytes:
    """
    Encode PCM16 audio to Opus.

    Format: 4-byte little-endian packet count, then for each packet:
    2-byte little-endian length prefix followed by the Opus packet bytes.
    This allows exact reconstruction on decode.

    Args:
        pcm_data: Raw PCM16 audio bytes
        sample_rate: Sample rate in Hz (default 16000)
        channels: Number of audio channels (default 1)

    Returns:
        Length-prefixed Opus packets as bytes
    """
    encoder = opuslib.Encoder(sample_rate, channels, opuslib.APPLICATION_VOIP)
    frame_size = sample_rate * OPUS_FRAME_DURATION_MS // 1000
    bytes_per_frame = frame_size * channels * 2  # 16-bit = 2 bytes per sample

    packets = []
    offset = 0
    while offset + bytes_per_frame <= len(pcm_data):
        frame = pcm_data[offset : offset + bytes_per_frame]
        encoded = encoder.encode(frame, frame_size)
        packets.append(encoded)
        offset += bytes_per_frame

    # Encode remaining samples (pad with silence)
    if offset < len(pcm_data):
        remaining = pcm_data[offset:]
        padded = remaining + b'\x00' * (bytes_per_frame - len(remaining))
        encoded = encoder.encode(padded, frame_size)
        packets.append(encoded)

    # Pack: [packet_count (4 bytes)] + [original_pcm_len (4 bytes)] + [len (2 bytes) + data] per packet
    output = struct.pack('<I', len(packets))
    output += struct.pack('<I', len(pcm_data))
    for pkt in packets:
        output += struct.pack('<H', len(pkt)) + pkt

    return output


def decode_opus_to_pcm(opus_data: bytes, sample_rate: int = OPUS_SAMPLE_RATE, channels: int = OPUS_CHANNELS) -> bytes:
    """
    Decode length-prefixed Opus packets back to PCM16.

    Args:
        opus_data: Length-prefixed Opus packets (from encode_pcm_to_opus)
        sample_rate: Sample rate in Hz (default 16000)
        channels: Number of audio channels (default 1)

    Returns:
        Raw PCM16 audio bytes

    Raises:
        ValueError: If opus_data is too short or has invalid header/packet structure
    """
    if len(opus_data) < 8:
        raise ValueError(f"Opus data too short: {len(opus_data)} bytes (need at least 8 for header)")

    decoder = opuslib.Decoder(sample_rate, channels)
    frame_size = sample_rate * OPUS_FRAME_DURATION_MS // 1000

    offset = 0
    packet_count = struct.unpack_from('<I', opus_data, offset)[0]
    offset += 4
    original_pcm_len = struct.unpack_from('<I', opus_data, offset)[0]
    offset += 4

    pcm_parts = []
    for i in range(packet_count):
        if offset + 2 > len(opus_data):
            raise ValueError(f"Truncated Opus data: expected packet {i}/{packet_count} length at offset {offset}")
        pkt_len = struct.unpack_from('<H', opus_data, offset)[0]
        offset += 2
        if offset + pkt_len > len(opus_data):
            raise ValueError(
                f"Truncated Opus data: packet {i} needs {pkt_len} bytes at offset {offset}, only {len(opus_data) - offset} available"
            )
        pkt_data = opus_data[offset : offset + pkt_len]
        offset += pkt_len
        decoded = decoder.decode(pkt_data, frame_size)
        pcm_parts.append(decoded)

    result = b''.join(pcm_parts)
    # Trim to original PCM length to remove padding from partial final frame
    if original_pcm_len > 0 and original_pcm_len < len(result):
        result = result[:original_pcm_len]
    return result


def _get_extension_for_path(path: str) -> str:
    """Extract the private cloud sync extension from a path."""
    if path.endswith('.batch.enc'):
        return 'batch.enc'
    elif path.endswith('.batch.bin'):
        return 'batch.bin'
    elif path.endswith('.opus.enc'):
        return 'opus.enc'
    elif path.endswith('.opus'):
        return 'opus'
    elif path.endswith('.enc'):
        return 'enc'
    elif path.endswith('.bin'):
        return 'bin'
    return 'bin'


def _strip_extension(filename: str) -> str:
    """Strip private cloud sync extension to get the timestamp string.

    Handles both single-chunk filenames (e.g. '1000.000.opus') and
    batch filenames (e.g. '1000.000-1010.000.batch.bin').
    """
    for ext in ('.batch.enc', '.batch.bin', '.opus.enc', '.opus', '.enc', '.bin'):
        if filename.endswith(ext):
            return filename[: -len(ext)]
    return filename.rsplit('.', 1)[0]


def upload_audio_chunk(
    chunk_data: bytes, uid: str, conversation_id: str, timestamp: float, data_protection_level: str = None
) -> str:
    """
    Upload an audio chunk to local filesystem storage with optional encryption.

    Args:
        chunk_data: Raw audio bytes (PCM16)
        uid: User ID
        conversation_id: Conversation ID
        timestamp: Unix timestamp when chunk was recorded
        data_protection_level: Optional cached protection level. When provided,
            skips the per-chunk Firestore read. Falls back to DB read when None.

    Returns:
        Filesystem path of the uploaded chunk
    """
    protection_level = (
        data_protection_level if data_protection_level is not None else users_db.get_data_protection_level(uid)
    )

    # Format timestamp to 3 decimal places for cleaner filenames
    formatted_timestamp = f'{timestamp:.3f}'

    upload_data = encode_pcm_to_opus(chunk_data)

    if protection_level == 'enhanced':
        encrypted_chunk = encryption.encrypt_audio_chunk(upload_data, uid)
        path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{formatted_timestamp}.opus.enc')
        _write_file(path, encrypted_chunk, 'application/octet-stream')
    else:
        path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{formatted_timestamp}.opus')
        _write_file(path, upload_data, 'application/octet-stream')

    del upload_data
    return str(path)


def upload_audio_chunks_batch(
    chunks: List[dict],
    uid: str,
    conversation_id: str,
    data_protection_level: str = None,
) -> List[str]:
    """
    Upload multiple audio chunks to local filesystem in a single write.

    Concatenates all chunk data into one file (1 write op instead of N).

    Args:
        chunks: List of dicts with 'data' (bytes) and 'timestamp' (float).
        uid: User ID.
        conversation_id: Conversation ID.
        data_protection_level: Optional cached protection level. When provided,
            skips the Firestore read. Falls back to DB read when None.

    Returns:
        List of filesystem paths for the uploaded batch.
    """
    if not chunks:
        return []

    # Sort by timestamp for consistent ordering
    sorted_chunks = sorted(chunks, key=lambda c: c['timestamp'])

    # Resolve protection level once for the entire batch
    protection_level = (
        data_protection_level if data_protection_level is not None else users_db.get_data_protection_level(uid)
    )

    # Build batch filename from first and last timestamps
    first_ts = f'{sorted_chunks[0]["timestamp"]:.3f}'
    last_ts = f'{sorted_chunks[-1]["timestamp"]:.3f}'
    batch_name = f'{first_ts}-{last_ts}' if len(sorted_chunks) > 1 else first_ts

    if protection_level == 'enhanced':
        # Encrypt each chunk individually (length-prefixed), write to file
        path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{batch_name}.batch.enc')
        _ensure_dir(path)
        with open(path, 'wb') as f:
            for chunk in sorted_chunks:
                encrypted_chunk = encryption.encrypt_audio_chunk(chunk['data'], uid)
                f.write(encrypted_chunk)
                del encrypted_chunk
    else:
        # Standard — stream raw PCM data to file
        path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{batch_name}.batch.bin')
        _ensure_dir(path)
        with open(path, 'wb') as f:
            for chunk in sorted_chunks:
                f.write(chunk['data'])

    return [str(path)]


def delete_audio_chunks(uid: str, conversation_id: str, timestamps: List[float]) -> None:
    """Delete audio chunks after they've been merged.

    Handles both single-chunk files (per-timestamp lookup) and batch files
    (listed and matched by start timestamp).
    """
    deleted_batch_paths = set()

    for timestamp in timestamps:
        # Format timestamp to match upload format (3 decimal places)
        formatted_timestamp = f'{timestamp:.3f}'

        # Try single-chunk extensions first
        for extension in PRIVATE_CLOUD_EXTENSIONS:
            if extension in ('.batch.enc', '.batch.bin'):
                continue  # batch blobs handled separately below
            chunk_path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{formatted_timestamp}{extension}')
            if chunk_path.exists():
                chunk_path.unlink()

        # Try batch blobs: exact single-timestamp batch (e.g. "1000.000.batch.bin")
        for batch_ext in ('.batch.enc', '.batch.bin'):
            batch_path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{formatted_timestamp}{batch_ext}')
            if str(batch_path) not in deleted_batch_paths and batch_path.exists():
                batch_path.unlink()
                deleted_batch_paths.add(str(batch_path))

    # Scan for range-named batch blobs whose start timestamp matches any requested timestamp
    ts_set = {f'{ts:.3f}' for ts in timestamps}
    prefix = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id)
    for blob in _list_blobs(prefix):
        if blob.name in deleted_batch_paths:
            continue
        filename = blob.name.split('/')[-1]
        if '.batch.' not in filename:
            continue
        timestamp_str = _strip_extension(filename)
        if '-' in timestamp_str:
            start_ts = timestamp_str.split('-', 1)[0]
            if start_ts in ts_set:
                blob.delete()
                deleted_batch_paths.add(blob.name)


def list_audio_chunks(uid: str, conversation_id: str) -> List[dict]:
    """
    List all audio chunks for a conversation.

    Returns:
        List of dicts with chunk info: {'timestamp': float, 'path': str, 'size': int}
    """
    prefix = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id)
    blobs = _list_blobs(prefix)

    chunks = []
    for blob in blobs:
        # Extract timestamp from filename
        # Supports single-chunk: '1234567890.123.opus', '1234567890.123.opus.enc', etc.
        # Supports batch: '1234567890.123-1234567900.123.batch.bin', '1234567890.123.batch.enc'
        filename = blob.name.split('/')[-1]
        has_valid_ext = any(filename.endswith(ext) for ext in PRIVATE_CLOUD_EXTENSIONS)
        if has_valid_ext:
            try:
                timestamp_str = _strip_extension(filename)
                is_batch = '.batch.' in filename

                if is_batch and '-' in timestamp_str:
                    # Batch blob with timestamp range: "first_ts-last_ts"
                    first_ts_str, last_ts_str = timestamp_str.split('-', 1)
                    timestamp = float(first_ts_str)
                else:
                    timestamp = float(timestamp_str)

                chunks.append(
                    {
                        'timestamp': timestamp,
                        'path': blob.name,
                        'size': blob.size,
                        'is_batch': is_batch,
                    }
                )
            except ValueError:
                continue

    return sorted(chunks, key=lambda x: x['timestamp'])


def delete_conversation_audio_files(uid: str, conversation_id: str) -> None:
    """Delete all audio files (chunks and merged) for a conversation."""
    # Delete chunks
    chunks_prefix = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id)
    for blob in _list_blobs(chunks_prefix):
        blob.delete()

    # Delete merged files
    audio_prefix = _get_bucket_path(private_cloud_sync_bucket, 'audio', uid, conversation_id)
    for blob in _list_blobs(audio_prefix):
        blob.delete()


def download_audio_chunks_and_merge(
    uid: str,
    conversation_id: str,
    timestamps: List[float],
    fill_gaps: bool = True,
    sample_rate: int = 16000,
) -> bytes:
    """
    Download and merge audio chunks on-demand, handling mixed encryption states.
    Downloads chunks in parallel.
    Normalizes all chunks to unencrypted PCM format for consistent merging.
    Supports both single-chunk blobs and batch blobs (from upload_audio_chunks_batch).

    Args:
        uid: User ID
        conversation_id: Conversation ID
        timestamps: List of chunk timestamps to merge
        fill_gaps: If True, insert silence (zero bytes) between chunks to maintain
                   continuous time-aligned audio. Default True.
        sample_rate: Audio sample rate in Hz (default 16000)

    Returns:
        Merged audio bytes (PCM16)
    """

    # Resolve actual filesystem paths — needed to find batch blobs whose filenames
    # contain timestamp ranges instead of single timestamps
    actual_chunks = list_audio_chunks(uid, conversation_id)
    ts_set = {round(ts, 3) for ts in timestamps}

    # Build batch blob map: for batch blobs, track which timestamps they cover
    batch_paths = {}  # path -> chunk_info (deduplicate downloads)
    ts_to_batch_path = {}  # timestamp -> batch_path (for timestamps inside batch range)
    single_chunk_timestamps = []  # timestamps that have individual blobs

    for chunk in actual_chunks:
        if chunk.get('is_batch'):
            path = chunk['path']
            batch_paths[path] = chunk

            # Parse batch range to determine covered timestamps
            filename = path.split('/')[-1]
            ts_str = _strip_extension(filename)
            if '-' in ts_str:
                start_str, end_str = ts_str.split('-', 1)
                batch_start = float(start_str)
                batch_end = float(end_str)
            else:
                batch_start = batch_end = float(ts_str)

            # Map requested timestamps that fall within this batch's range
            for ts in timestamps:
                if batch_start <= round(ts, 3) <= batch_end:
                    ts_to_batch_path[round(ts, 3)] = path
        elif round(chunk['timestamp'], 3) in ts_set:
            single_chunk_timestamps.append(chunk['timestamp'])

    def _download_and_decode_blob(filename: str) -> bytes | None:
        """Download a blob and decode/decrypt based on extension."""
        full_path = _get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, filename)

        ext = _get_extension_for_path(filename)
        encrypted = ext in ('opus.enc', 'enc', 'batch.enc')
        is_opus = ext in ('opus.enc', 'opus')

        try:
            chunk_data = _read_file(full_path)
        except FileNotFoundError:
            return None

        try:
            if encrypted:
                raw_data = encryption.decrypt_audio_file(chunk_data, uid)
            else:
                raw_data = chunk_data

            if is_opus:
                pcm_data = decode_opus_to_pcm(raw_data, sample_rate=sample_rate)
                del raw_data
            else:
                pcm_data = raw_data

            return pcm_data
        except Exception as e:
            logger.warning(f"Failed to decode/decrypt {filename}: {e}")
            return None

    def download_single_chunk(timestamp: float) -> tuple[float, bytes | None]:
        """Download a single-chunk blob by trying extensions in priority order."""
        formatted_timestamp = f'{timestamp:.3f}'

        extensions_to_try = [
            ('opus.enc', True, True),  # (ext, encrypted, opus)
            ('enc', True, False),
            ('opus', False, True),
            ('bin', False, False),
        ]

        for ext, encrypted, opus in extensions_to_try:
            try:
                chunk_data = _read_file(_get_bucket_path(private_cloud_sync_bucket, 'chunks', uid, conversation_id, f'{formatted_timestamp}.{ext}'))
            except FileNotFoundError:
                continue

            try:
                if encrypted:
                    raw_data = encryption.decrypt_audio_file(chunk_data, uid)
                else:
                    raw_data = chunk_data

                if opus:
                    pcm_data = decode_opus_to_pcm(raw_data, sample_rate=sample_rate)
                    del raw_data
                else:
                    pcm_data = raw_data

                return (timestamp, pcm_data)
            except Exception as e:
                logger.warning(
                    f"Failed to decode/decrypt {ext} chunk at {formatted_timestamp}: {e}, trying next format"
                )
                continue

        logger.warning(f"Warning: Chunk not found for timestamp {formatted_timestamp}")
        return (timestamp, None)

    # Download all data in parallel
    chunk_results = {}

    # Determine which timestamps need individual downloads vs batch downloads
    individual_timestamps = [ts for ts in timestamps if round(ts, 3) not in ts_to_batch_path]
    unique_batch_paths = set(ts_to_batch_path.values())

    # Submit individual chunk downloads via shared storage executor
    individual_futures = {storage_executor.submit(download_single_chunk, ts): ts for ts in individual_timestamps}

    # Submit batch blob downloads (once per unique path)
    batch_futures = {storage_executor.submit(_download_and_decode_blob, path.split('/')[-1]): path for path in unique_batch_paths}

    # Collect individual results
    for future in as_completed(individual_futures):
        timestamp, pcm_data = future.result()
        if pcm_data is not None:
            chunk_results[timestamp] = pcm_data

    # Collect batch results — assign full batch data at the batch's start timestamp
    for future in as_completed(batch_futures):
        path = batch_futures[future]
        pcm_data = future.result()
        if pcm_data is not None:
            batch_info = batch_paths[path]
            chunk_results[batch_info['timestamp']] = pcm_data

    # Merge chunks
    merged_data = bytearray()

    if fill_gaps and timestamps and chunk_results:
        # Sort timestamps to ensure proper ordering
        sorted_timestamps = sorted(timestamps)
        first_timestamp = sorted_timestamps[0]
        current_time = first_timestamp  # Track current audio end time in seconds

        for timestamp in sorted_timestamps:
            if timestamp not in chunk_results:
                continue

            pcm_data = chunk_results[timestamp]

            # Calculate gap from current position to this chunk's start
            gap_seconds = timestamp - current_time
            if gap_seconds > 0:
                # Insert silence: 16-bit mono = 2 bytes per sample
                gap_samples = int(gap_seconds * sample_rate)
                silence_bytes = bytes(gap_samples * 2)  # Zero bytes for silence
                merged_data.extend(silence_bytes)
                logger.info(f"Filled {gap_seconds:.3f}s gap ({len(silence_bytes)} bytes) before chunk at {timestamp}")

            merged_data.extend(pcm_data)

            # Update current time based on chunk duration
            # PCM16 mono: 2 bytes per sample
            chunk_duration = len(pcm_data) / (sample_rate * 2)
            current_time = timestamp + chunk_duration
    else:
        # Original behavior - just concatenate without gap filling
        for timestamp in timestamps:
            if timestamp in chunk_results:
                merged_data.extend(chunk_results[timestamp])

    # Free memory from chunk results immediately after merging
    chunk_results.clear()

    if not merged_data:
        raise FileNotFoundError(f"No chunks found for conversation {conversation_id}")

    return bytes(merged_data)


def get_cached_merged_audio_path(uid: str, conversation_id: str, audio_file_id: str) -> str:
    """Get the filesystem path for a cached merged audio file."""
    return f'audio/{uid}/{conversation_id}/{audio_file_id}.wav'


def get_or_create_merged_audio(
    uid: str,
    conversation_id: str,
    audio_file_id: str,
    timestamps: List[float],
    pcm_to_wav_func,
    fill_gaps: bool = True,
    sample_rate: int = 16000,
) -> tuple[bytes, bool]:
    """
    Get merged audio from cache or create it.
    Cached files are stored in local filesystem with no TTL (filesystem persistence).

    Args:
        uid: User ID
        conversation_id: Conversation ID
        audio_file_id: Audio file ID
        timestamps: List of chunk timestamps
        pcm_to_wav_func: Function to convert PCM to WAV
        fill_gaps: If True, insert silence between chunks to maintain time alignment. Default True.
        sample_rate: Audio sample rate in Hz (default 16000)

    Returns:
        Tuple of (audio_data_bytes, was_cached)
    """
    cache_rel_path = get_cached_merged_audio_path(uid, conversation_id, audio_file_id)
    cache_path = _get_bucket_path(private_cloud_sync_bucket, cache_rel_path)

    # Check if cached version exists
    if cache_path.exists():
        logger.info(f"Serving merged audio from cache: {cache_path}")
        return _read_file(cache_path), True

    # Cache miss - create new merged file
    logger.info(f"Cache miss, merging audio for: {cache_path}")

    # Download and merge chunks
    pcm_data = download_audio_chunks_and_merge(
        uid, conversation_id, timestamps, fill_gaps=fill_gaps, sample_rate=sample_rate
    )

    # Convert to WAV
    wav_data = pcm_to_wav_func(pcm_data)
    del pcm_data  # Free PCM data immediately after WAV conversion

    # Upload to cache in background thread
    def _upload_to_cache():
        try:
            _ensure_dir(cache_path)
            _write_file(cache_path, wav_data, 'audio/wav')
            logger.info(f"Cached merged audio at: {cache_path}")
        except Exception as e:
            logger.error(f"Error uploading audio cache: {e}")

    storage_executor.submit(_upload_to_cache)

    return wav_data, False


def get_merged_audio_signed_url(uid: str, conversation_id: str, audio_file_id: str) -> str | None:
    """
    Get a file:// URL for cached merged audio if it exists.

    Returns:
        file:// URL, or None if cache doesn't exist
    """
    cache_rel_path = get_cached_merged_audio_path(uid, conversation_id, audio_file_id)
    cache_path = _get_bucket_path(private_cloud_sync_bucket, cache_rel_path)

    if not cache_path.exists():
        return None

    return f'file://{cache_path}'


def delete_cached_merged_audio(uid: str, conversation_id: str) -> None:
    """Delete all cached merged audio for a conversation."""
    prefix = _get_bucket_path(private_cloud_sync_bucket, 'audio', uid, conversation_id)
    for blob in _list_blobs(prefix):
        blob.delete()


def _pcm_to_wav(pcm_data: bytes, sample_rate: int = 16000, channels: int = 1) -> bytes:
    """Convert PCM16 data to WAV format."""
    wav_buffer = io.BytesIO()
    with wave.open(wav_buffer, 'wb') as wav_file:
        wav_file.setnchannels(channels)
        wav_file.setsampwidth(2)  # 16-bit audio
        wav_file.setframerate(sample_rate)
        wav_file.writeframes(pcm_data)
    return wav_buffer.getvalue()


def precache_conversation_audio(
    uid: str, conversation_id: str, audio_files: list, fill_gaps: bool = True, sample_rate: int = 16000
) -> None:
    """
    Pre-cache all audio files for a conversation in a background thread.

    Args:
        uid: User ID
        conversation_id: Conversation ID
        audio_files: List of audio file dicts with 'id' and 'chunk_timestamps'
        fill_gaps: If True, insert silence between chunks to maintain time alignment. Default True.
        sample_rate: Audio sample rate in Hz (default 16000)
    """
    if not audio_files:
        return

    def _precache_all():

        def _cache_single(af):
            try:
                audio_file_id = af.get('id')
                timestamps = af.get('chunk_timestamps')
                if not audio_file_id or not timestamps:
                    return
                get_or_create_merged_audio(
                    uid=uid,
                    conversation_id=conversation_id,
                    audio_file_id=audio_file_id,
                    timestamps=timestamps,
                    pcm_to_wav_func=_pcm_to_wav,
                    fill_gaps=fill_gaps,
                    sample_rate=sample_rate,
                )
            except Exception as e:
                logger.error(f"[PRECACHE] Error caching audio file {af.get('id')}: {e}")

        futures = [storage_executor.submit(_cache_single, af) for af in audio_files]
        for future in as_completed(futures):
            try:
                future.result()
            except Exception:
                pass

    storage_executor.submit(_precache_all)


# **********************************
# ************* UTILS **************
# **********************************


def download_blob_bytes(bucket_name: str, path: str) -> bytes:
    """
    Download blob content as bytes from local filesystem.

    Args:
        bucket_name: Name of the storage bucket (maps to subdirectory)
        path: Path within the bucket

    Returns:
        File content as bytes

    Raises:
        FileNotFoundError: If the file doesn't exist
    """
    file_path = _get_bucket_path(bucket_name, path)
    return _read_file(file_path)


def delete_blob(bucket_name: str, path: str) -> bool:
    """
    Delete a blob from local filesystem.

    Args:
        bucket_name: Name of the storage bucket (maps to subdirectory)
        path: Path within the bucket

    Returns:
        True if deleted, False if not found
    """
    file_path = _get_bucket_path(bucket_name, path)
    return _delete_file(file_path)


def download_speech_profile_bytes(path: str) -> bytes:
    """
    Download speech profile/sample audio from local filesystem.

    Args:
        path: Filesystem path to the sample (e.g., '{uid}/people_profiles/{person_id}/{filename}.wav')

    Returns:
        Audio bytes (WAV format)

    Raises:
        FileNotFoundError: If the sample doesn't exist
    """
    # path is already a relative path, parse it
    parts = path.split('/')
    return download_blob_bytes(speech_profiles_bucket, *parts)


def delete_speech_profile_blob(path: str) -> bool:
    """
    Delete speech profile/sample from local filesystem.

    Args:
        path: Filesystem path to the sample

    Returns:
        True if deleted, False if not found
    """
    parts = path.split('/')
    return delete_blob(speech_profiles_bucket, *parts)


def _get_signed_url(path: Path, minutes: int) -> str:
    """Return a file:// URL for a local path."""
    # For filesystem storage, signed URL is just the file path
    return f'file://{path}'


def upload_app_logo(file_path: str, app_id: str):
    path = _get_bucket_path(omi_apps_bucket, f'{app_id}.png')
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def delete_app_logo(img_url: str):
    path = img_url.split('file://')[1] if 'file://' in img_url else img_url
    path = Path(path)
    logger.info(f'delete_app_logo {path}')
    _delete_file(path)


def upload_app_thumbnail(file_path: str, thumbnail_id: str) -> str:
    path = _get_bucket_path(app_thumbnails_bucket, f'{thumbnail_id}.jpg')
    _ensure_dir(path)
    Path(file_path).copy_to(path)
    return f'file://{path}'


def get_app_thumbnail_url(thumbnail_id: str) -> str:
    path = _get_bucket_path(app_thumbnails_bucket, f'{thumbnail_id}.jpg')
    return f'file://{path}'


# **********************************
# ************* CHAT FILES **************
# **********************************
def upload_multi_chat_files(files_name: List[str], uid: str) -> dict:
    """
    Upload multiple files to local filesystem storage in the chat files bucket.

    Args:
        files_name: List of file paths to upload
        uid: User ID to use as part of the storage path

    Returns:
        dict: A dictionary mapping original filenames to their local filesystem URLs
    """
    dictFiles = {}
    for name in files_name:
        try:
            path = _get_bucket_path(chat_files_bucket, uid, name)
            _ensure_dir(path)
            Path(f'./{name}').copy_to(path)
            dictFiles[name] = f'file://{path}'
        except Exception as e:
            logger.error("Failed to upload {} due to exception: {}".format(name, e))
    return dictFiles


# **************************************************
# ************* DESKTOP UPDATES ********************
# **************************************************


def get_desktop_update_signed_url(blob_path: str, expiration_hours: int = 1) -> str:
    """
    Generate a URL for a desktop update file (ZIP).

    Args:
        blob_path: Path to the file (e.g., "1.0.78+474-macos/1.0.78+474-macos.zip")
        expiration_hours: Hours until the URL expires (unused for local storage, default: 1 hour)

    Returns:
        file:// URL valid for the specified duration
    """
    path = _get_bucket_path(desktop_updates_bucket, blob_path)
    return f'file://{path}'
