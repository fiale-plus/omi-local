"""Unit tests for upload_audio_chunk data_protection_level caching.

Verifies that when data_protection_level is passed to upload_audio_chunk(),
the per-chunk Firestore read (users_db.get_data_protection_level) is skipped.
When not provided, falls back to the DB read for backward compatibility.
"""

import os
import sys
from unittest.mock import MagicMock, patch, call

import pytest

os.environ.setdefault("ENCRYPTION_SECRET", "omi_ZwB2ZNqB2HHpMK6wStk7sTpavJiPTFg7gXUHnc4tFABPU6pZ2c2DKgehtfgi4RZv")

# Mock heavy dependencies at sys.modules level before importing storage
sys.modules.setdefault("database._client", MagicMock())
sys.modules.setdefault("database.redis_db", MagicMock())
sys.modules.setdefault("database.users", MagicMock())
sys.modules.setdefault("redis", MagicMock())
sys.modules.setdefault("google.cloud", MagicMock())
sys.modules.setdefault("google.cloud.firestore", MagicMock())

# Use filesystem-based storage (no GCS mocks needed)
from utils.other import storage as storage_mod


class TestUploadAudioChunkDataProtectionCache:
    """Tests for the data_protection_level caching in upload_audio_chunk."""

    @patch.object(storage_mod, 'users_db')
    def test_skips_db_read_when_level_provided(self, mock_users_db, tmp_path):
        """When data_protection_level is passed, should NOT call Firestore."""
        with patch.object(storage_mod, 'LOCAL_STORAGE_ROOT', str(tmp_path)):
            storage_mod.upload_audio_chunk(
                chunk_data=b'\x00' * 100,
                uid='test-uid',
                conversation_id='conv-1',
                timestamp=1234567890.123,
                data_protection_level='standard',
            )

        mock_users_db.get_data_protection_level.assert_not_called()

    @patch.object(storage_mod, 'users_db')
    def test_falls_back_to_db_when_level_not_provided(self, mock_users_db, tmp_path):
        """When data_protection_level is None (default), should read from Firestore."""
        mock_users_db.get_data_protection_level.return_value = 'standard'

        with patch.object(storage_mod, 'LOCAL_STORAGE_ROOT', str(tmp_path)):
            storage_mod.upload_audio_chunk(
                chunk_data=b'\x00' * 100,
                uid='test-uid',
                conversation_id='conv-1',
                timestamp=1234567890.123,
            )

        mock_users_db.get_data_protection_level.assert_called_once_with('test-uid')

    def test_standard_level_uploads_unencrypted(self, tmp_path):
        """Standard protection level should upload .opus (Opus encoded, no encryption)."""
        with patch.object(storage_mod, 'LOCAL_STORAGE_ROOT', str(tmp_path)):
            path = storage_mod.upload_audio_chunk(
                chunk_data=b'\x00' * 100,
                uid='test-uid',
                conversation_id='conv-1',
                timestamp=1234567890.123,
                data_protection_level='standard',
            )

        assert path.endswith('.opus')
        assert '.enc' not in path

    @patch.object(storage_mod, 'encryption')
    @patch.object(storage_mod, 'users_db')
    def test_enhanced_level_uploads_encrypted(self, mock_users_db, mock_encryption, tmp_path):
        """Enhanced protection level should encrypt and upload .opus.enc."""
        mock_encryption.encrypt_audio_chunk.return_value = b'\x01' * 120

        with patch.object(storage_mod, 'LOCAL_STORAGE_ROOT', str(tmp_path)):
            path = storage_mod.upload_audio_chunk(
                chunk_data=b'\x00' * 100,
                uid='test-uid',
                conversation_id='conv-1',
                timestamp=1234567890.123,
                data_protection_level='enhanced',
            )

        assert path.endswith('.opus.enc')

    @patch.object(storage_mod, 'users_db')
    def test_explicit_none_falls_back_to_db(self, mock_users_db, tmp_path):
        """Explicitly passing None should still fall back to DB read."""
        mock_users_db.get_data_protection_level.return_value = 'standard'

        with patch.object(storage_mod, 'LOCAL_STORAGE_ROOT', str(tmp_path)):
            storage_mod.upload_audio_chunk(
                chunk_data=b'\x00' * 100,
                uid='test-uid',
                conversation_id='conv-1',
                timestamp=1234567890.123,
                data_protection_level=None,
            )

        mock_users_db.get_data_protection_level.assert_called_once_with('test-uid')
