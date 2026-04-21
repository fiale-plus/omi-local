"""
Local SQLite database for LOCAL_MODE.

Provides local storage for conversations, memories, action items, and other
data that would normally be stored in Firestore. Uses the same SQLite database
as local_fts.py at ~/.omi/local/omi_local.db.

No cloud services (Firebase, Firestore) are used in LOCAL_MODE.
"""

import json
import logging
import os
import sqlite3
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

logger = logging.getLogger(__name__)

# Path to the local SQLite database (shared with local_fts.rs via desktop Backend-Rust)
_LOCAL_DB_DIR = os.path.expanduser("~/.omi/local")
_LOCAL_DB_PATH = os.path.join(_LOCAL_DB_DIR, "omi_local.db")


def _get_conn() -> Optional[sqlite3.Connection]:
    """Get a connection to the local database, creating if needed."""
    try:
        Path(_LOCAL_DB_DIR).mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(_LOCAL_DB_PATH, timeout=10.0)
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA foreign_keys=ON")
        return conn
    except Exception as e:
        logger.warning(f"Could not open local database: {e}")
        return None


def _ensure_schema(conn: sqlite3.Connection):
    """Create tables if they don't exist."""
    # Conversations table
    conn.execute("""
        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            title TEXT,
            overview TEXT,
            category TEXT,
            emoji TEXT,
            discarded INTEGER DEFAULT 0,
            status TEXT DEFAULT 'completed',
            language TEXT,
            created_at TEXT,
            started_at TEXT,
            finished_at TEXT,
            folder_id TEXT,
            structured_json TEXT,
            transcript_segments_json TEXT,
            photos_json TEXT,
            action_items_json TEXT,
            apps_results_json TEXT,
            data_protection_level TEXT DEFAULT 'standard',
            source TEXT DEFAULT 'local_listen',
            is_locked INTEGER DEFAULT 0,
            external_data_json TEXT,
            suggested_summarization_apps_json TEXT,
            audio_files_json TEXT
        )
    """)
    
    # Memories table
    conn.execute("""
        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            conversation_id TEXT,
            content TEXT NOT NULL,
            category TEXT,
            is_locked INTEGER DEFAULT 0,
            kg_extracted INTEGER DEFAULT 0,
            created_at TEXT,
            updated_at TEXT
        )
    """)
    
    # Action items table
    conn.execute("""
        CREATE TABLE IF NOT EXISTS action_items (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            conversation_id TEXT,
            description TEXT NOT NULL,
            completed INTEGER DEFAULT 0,
            due_at TEXT,
            created_at TEXT,
            updated_at TEXT,
            completed_at TEXT,
            is_locked INTEGER DEFAULT 0
        )
    """)
    
    # Indexes for common queries
    conn.execute("CREATE INDEX IF NOT EXISTS idx_conversations_uid ON conversations(uid)")
    conn.execute("CREATE INDEX IF NOT EXISTS idx_conversations_created_at ON conversations(created_at)")
    conn.execute("CREATE INDEX IF NOT EXISTS idx_memories_uid ON memories(uid)")
    conn.execute("CREATE INDEX IF NOT EXISTS idx_memories_conversation_id ON memories(conversation_id)")
    conn.execute("CREATE INDEX IF NOT EXISTS idx_action_items_uid ON action_items(uid)")
    conn.execute("CREATE INDEX IF NOT EXISTS idx_action_items_conversation_id ON action_items(conversation_id)")
    
    conn.commit()


# ── Conversations ──────────────────────────────────────────────────────────────

def upsert_conversation(uid: str, conversation_data: Dict[str, Any]) -> bool:
    """Save a conversation to local SQLite."""
    conn = _get_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        
        # Serialize complex fields
        structured_json = conversation_data.get("structured")
        if structured_json:
            structured_json = json.dumps(structured_json) if isinstance(structured_json, dict) else structured_json
        
        transcript_segments_json = conversation_data.get("transcript_segments")
        if transcript_segments_json:
            transcript_segments_json = json.dumps(transcript_segments_json) if isinstance(transcript_segments_json, list) else transcript_segments_json
        
        photos_json = conversation_data.get("photos")
        if photos_json:
            photos_json = json.dumps(photos_json) if isinstance(photos_json, list) else photos_json
        
        action_items_json = conversation_data.get("action_items")
        if action_items_json:
            action_items_json = json.dumps(action_items_json) if isinstance(action_items_json, list) else action_items_json
        
        apps_results_json = conversation_data.get("apps_results")
        if apps_results_json:
            apps_results_json = json.dumps(apps_results_json) if isinstance(apps_results_json, list) else apps_results_json
        
        suggested_apps_json = conversation_data.get("suggested_summarization_apps")
        if suggested_apps_json:
            suggested_apps_json = json.dumps(suggested_apps_json) if isinstance(suggested_apps_json, list) else suggested_apps_json
        
        audio_files_json = conversation_data.get("audio_files")
        if audio_files_json:
            audio_files_json = json.dumps(audio_files_json) if isinstance(audio_files_json, list) else audio_files_json
        
        external_data_json = conversation_data.get("external_data")
        if external_data_json:
            external_data_json = json.dumps(external_data_json) if isinstance(external_data_json, dict) else external_data_json
        
        # Handle datetime serialization
        def to_iso(val):
            if isinstance(val, datetime):
                return val.isoformat()
            return val
        
        conn.execute("""
            INSERT INTO conversations (
                id, uid, title, overview, category, emoji, discarded, status,
                language, created_at, started_at, finished_at, folder_id,
                structured_json, transcript_segments_json, photos_json,
                action_items_json, apps_results_json, data_protection_level,
                source, is_locked, external_data_json, suggested_summarization_apps_json,
                audio_files_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title=excluded.title,
                overview=excluded.overview,
                category=excluded.category,
                emoji=excluded.emoji,
                discarded=excluded.discarded,
                status=excluded.status,
                language=excluded.language,
                finished_at=excluded.finished_at,
                folder_id=excluded.folder_id,
                structured_json=excluded.structured_json,
                transcript_segments_json=excluded.transcript_segments_json,
                photos_json=excluded.photos_json,
                action_items_json=excluded.action_items_json,
                apps_results_json=excluded.apps_results_json,
                suggested_summarization_apps_json=excluded.suggested_summarization_apps_json,
                audio_files_json=excluded.audio_files_json
        """, (
            conversation_data.get("id"),
            uid,
            conversation_data.get("title"),
            conversation_data.get("overview"),
            conversation_data.get("category"),
            conversation_data.get("emoji"),
            int(conversation_data.get("discarded", False)),
            conversation_data.get("status", "completed"),
            conversation_data.get("language"),
            to_iso(conversation_data.get("created_at")),
            to_iso(conversation_data.get("started_at")),
            to_iso(conversation_data.get("finished_at")),
            conversation_data.get("folder_id"),
            structured_json,
            transcript_segments_json,
            photos_json,
            action_items_json,
            apps_results_json,
            conversation_data.get("data_protection_level", "standard"),
            conversation_data.get("source", "local_listen"),
            int(conversation_data.get("is_locked", False)),
            external_data_json,
            suggested_apps_json,
            audio_files_json,
        ))
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to upsert conversation: {e}")
        return False


def get_conversation(uid: str, conversation_id: str) -> Optional[Dict[str, Any]]:
    """Get a conversation by ID from local SQLite."""
    conn = _get_conn()
    if conn is None:
        return None
    try:
        _ensure_schema(conn)
        cursor = conn.execute(
            "SELECT * FROM conversations WHERE id = ? AND uid = ?",
            (conversation_id, uid)
        )
        row = cursor.fetchone()
        if row is None:
            return None
        
        columns = [desc[0] for desc in conn.execute("SELECT * FROM conversations LIMIT 1").description]
        data = dict(zip(columns, row))
        
        # Deserialize complex fields
        for field in ["structured_json", "transcript_segments_json", "photos_json", 
                      "action_items_json", "apps_results_json", "suggested_summarization_apps_json",
                      "audio_files_json", "external_data_json"]:
            if data.get(field):
                try:
                    data[field[:-5]] = json.loads(data[field])
                except json.JSONDecodeError:
                    pass
        
        if "is_locked" in data:
            data["is_locked"] = bool(data["is_locked"])
        if "discarded" in data:
            data["discarded"] = bool(data["discarded"])
        
        return data
    except Exception as e:
        logger.warning(f"Failed to get conversation: {e}")
        return None


def get_conversations(
    uid: str,
    limit: int = 100,
    offset: int = 0,
    include_discarded: bool = False,
    folder_id: Optional[str] = None,
) -> List[Dict[str, Any]]:
    """Get conversations for a user from local SQLite."""
    conn = _get_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        
        query = "SELECT * FROM conversations WHERE uid = ?"
        params: List[Any] = [uid]
        
        if not include_discarded:
            query += " AND discarded = 0"
        
        if folder_id:
            query += " AND folder_id = ?"
            params.append(folder_id)
        
        query += " ORDER BY created_at DESC LIMIT ? OFFSET ?"
        params.extend([limit, offset])
        
        cursor = conn.execute(query, params)
        rows = cursor.fetchall()
        
        columns = [desc[0] for desc in conn.execute("SELECT * FROM conversations LIMIT 1").description]
        results = []
        for row in rows:
            data = dict(zip(columns, row))
            for field in ["structured_json", "transcript_segments_json", "photos_json",
                          "action_items_json", "apps_results_json", "suggested_summarization_apps_json",
                          "audio_files_json", "external_data_json"]:
                if data.get(field):
                    try:
                        data[field[:-5]] = json.loads(data[field])
                    except json.JSONDecodeError:
                        pass
            if "is_locked" in data:
                data["is_locked"] = bool(data["is_locked"])
            if "discarded" in data:
                data["discarded"] = bool(data["discarded"])
            results.append(data)
        
        return results
    except Exception as e:
        logger.warning(f"Failed to get conversations: {e}")
        return []


# ── Memories ──────────────────────────────────────────────────────────────────

def save_memories(uid: str, memories_data: List[Dict[str, Any]]) -> List[str]:
    """Save memories to local SQLite. Returns list of memory IDs."""
    conn = _get_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        memory_ids = []
        now = datetime.now(timezone.utc).isoformat()
        
        for memory in memories_data:
            memory_id = memory.get("id", str(uuid.uuid4()))
            memory_ids.append(memory_id)
            
            def to_iso(val):
                if isinstance(val, datetime):
                    return val.isoformat()
                return val
            
            conn.execute("""
                INSERT INTO memories (
                    id, uid, conversation_id, content, category,
                    is_locked, kg_extracted, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    content=excluded.content,
                    category=excluded.category,
                    is_locked=excluded.is_locked,
                    kg_extracted=excluded.kg_extracted,
                    updated_at=excluded.updated_at
            """, (
                memory_id,
                uid,
                memory.get("conversation_id"),
                memory.get("content"),
                memory.get("category"),
                int(memory.get("is_locked", False)),
                int(memory.get("kg_extracted", False)),
                to_iso(memory.get("created_at", now)),
                to_iso(memory.get("updated_at", now)),
            ))
        
        conn.commit()
        return memory_ids
    except Exception as e:
        logger.warning(f"Failed to save memories: {e}")
        return []


def get_memory(uid: str, memory_id: str) -> Optional[Dict[str, Any]]:
    """Get a memory by ID from local SQLite."""
    conn = _get_conn()
    if conn is None:
        return None
    try:
        _ensure_schema(conn)
        cursor = conn.execute(
            "SELECT * FROM memories WHERE id = ? AND uid = ?",
            (memory_id, uid)
        )
        row = cursor.fetchone()
        if row is None:
            return None
        
        columns = [desc[0] for desc in conn.execute("SELECT * FROM memories LIMIT 1").description]
        data = dict(zip(columns, row))
        
        if "is_locked" in data:
            data["is_locked"] = bool(data["is_locked"])
        if "kg_extracted" in data:
            data["kg_extracted"] = bool(data["kg_extracted"])
        
        return data
    except Exception as e:
        logger.warning(f"Failed to get memory: {e}")
        return None


def get_memory_ids_for_conversation(uid: str, conversation_id: str) -> List[str]:
    """Get memory IDs for a conversation."""
    conn = _get_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        cursor = conn.execute(
            "SELECT id FROM memories WHERE uid = ? AND conversation_id = ?",
            (uid, conversation_id)
        )
        return [row[0] for row in cursor.fetchall()]
    except Exception as e:
        logger.warning(f"Failed to get memory IDs: {e}")
        return []


def delete_memory(uid: str, memory_id: str) -> bool:
    """Delete a memory."""
    conn = _get_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        conn.execute("DELETE FROM memories WHERE id = ? AND uid = ?", (memory_id, uid))
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to delete memory: {e}")
        return False


def delete_memories_for_conversation(uid: str, conversation_id: str) -> bool:
    """Delete all memories for a conversation."""
    conn = _get_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        conn.execute("DELETE FROM memories WHERE uid = ? AND conversation_id = ?", (uid, conversation_id))
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to delete memories: {e}")
        return False


# ── Action Items ─────────────────────────────────────────────────────────────

def create_action_items_batch(
    uid: str, 
    action_items_data: List[Dict[str, Any]]
) -> List[str]:
    """Create action items in batch. Returns list of action item IDs."""
    conn = _get_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        item_ids = []
        now = datetime.now(timezone.utc).isoformat()
        
        for item in action_items_data:
            item_id = item.get("id", str(uuid.uuid4()))
            item_ids.append(item_id)
            
            def to_iso(val):
                if isinstance(val, datetime):
                    return val.isoformat()
                return val
            
            conn.execute("""
                INSERT INTO action_items (
                    id, uid, conversation_id, description, completed,
                    due_at, created_at, updated_at, completed_at, is_locked
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    description=excluded.description,
                    completed=excluded.completed,
                    due_at=excluded.due_at,
                    updated_at=excluded.updated_at,
                    completed_at=excluded.completed_at
            """, (
                item_id,
                uid,
                item.get("conversation_id"),
                item.get("description"),
                int(item.get("completed", False)),
                to_iso(item.get("due_at")),
                to_iso(item.get("created_at", now)),
                to_iso(item.get("updated_at", now)),
                to_iso(item.get("completed_at")),
                int(item.get("is_locked", False)),
            ))
        
        conn.commit()
        return item_ids
    except Exception as e:
        logger.warning(f"Failed to create action items: {e}")
        return []


def delete_action_items_for_conversation(uid: str, conversation_id: str) -> bool:
    """Delete action items for a conversation."""
    conn = _get_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        conn.execute(
            "DELETE FROM action_items WHERE uid = ? AND conversation_id = ?", 
            (uid, conversation_id)
        )
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to delete action items: {e}")
        return False


# ── Utility ──────────────────────────────────────────────────────────────────

def is_local_db_available() -> bool:
    """True when local database is available."""
    return _get_conn() is not None
