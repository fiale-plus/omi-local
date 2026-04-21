"""
Local FTS5 search fallback for LOCAL_MODE.
Uses SQLite FTS5 to provide lexical search without Pinecone.
Stores FTS5 tables alongside the existing local_db SQLite data.
"""

import logging
import os
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import List, Optional

logger = logging.getLogger(__name__)

# Path to the local SQLite database (shared with local_db.rs)
_LOCAL_DB_DIR = os.path.expanduser("~/.omi/local")
_LOCAL_DB_PATH = os.path.join(_LOCAL_DB_DIR, "omi_local.db")

# In-memory fallback for when no persisted DB exists yet
_MEMORY_FTS: Optional["_MemoryFTS"] = None


def _get_fts_conn() -> Optional[sqlite3.Connection]:
    """Get a connection to the local FTS5 database, creating if needed."""
    try:
        Path(_LOCAL_DB_DIR).mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(_LOCAL_DB_PATH, timeout=10.0)
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA foreign_keys=ON")
        return conn
    except Exception as e:
        logger.warning(f"Could not open local FTS database: {e}")
        return None


def _ensure_schema(conn: sqlite3.Connection):
    """Create FTS5 tables if they don't exist."""
    # FTS5 table for memories search
    conn.execute(
        """
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            memory_id,
            content,
            category,
            uid,
            created_at,
            tokenize='porter unicode61'
        )
        """
    )
    # FTS5 table for conversation search
    conn.execute(
        """
        CREATE VIRTUAL TABLE IF NOT EXISTS conversations_fts USING fts5(
            conversation_id,
            title,
            transcript,
            uid,
            created_at,
            tokenize='porter unicode61'
        )
        """
    )
    # FTS5 table for screen_activity OCR text search
    conn.execute(
        """
        CREATE VIRTUAL TABLE IF NOT EXISTS screen_activity_fts USING fts5(
            screenshot_id,
            ocr_text,
            app_name,
            window_title,
            uid,
            timestamp,
            tokenize='porter unicode61'
        )
        """
    )
    conn.commit()


def is_local_fts_available() -> bool:
    """True when local FTS storage is available."""
    return _get_fts_conn() is not None


# ── Memories FTS ──────────────────────────────────────────────────────────────

def upsert_memory_fts(
    uid: str,
    memory_id: str,
    content: str,
    category: str,
) -> bool:
    """Index a memory in local FTS5. Returns True on success."""
    conn = _get_fts_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        now = datetime.now(timezone.utc).isoformat()
        conn.execute(
            """
            INSERT INTO memories_fts (memory_id, content, category, uid, created_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(memory_id) DO UPDATE SET
                content=excluded.content,
                category=excluded.category,
                created_at=excluded.created_at
            """,
            (memory_id, content, category, uid, now),
        )
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to upsert memory FTS: {e}")
        return False


def search_memories_fts(uid: str, query: str, limit: int = 5) -> List[str]:
    """
    Lexical search over memories using FTS5.
    Returns list of memory_ids ordered by BM25 relevance.
    """
    conn = _get_fts_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        cursor = conn.execute(
            """
            SELECT memory_id FROM memories_fts
            WHERE uid = ? AND memories_fts MATCH ?
            ORDER BY bm25(memories_fts) LIMIT ?
            """,
            (uid, query, limit),
        )
        return [row[0] for row in cursor.fetchall()]
    except Exception as e:
        logger.warning(f"memories_fts search failed: {e}")
        return []


def delete_memory_fts(memory_id: str) -> bool:
    """Remove a memory from local FTS index."""
    conn = _get_fts_conn()
    if conn is None:
        return False
    try:
        conn.execute("DELETE FROM memories_fts WHERE memory_id = ?", (memory_id,))
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to delete memory FTS: {e}")
        return False


# ── Conversations FTS ────────────────────────────────────────────────────────

def upsert_conversation_fts(
    uid: str,
    conversation_id: str,
    title: str,
    transcript: str,
) -> bool:
    """Index a conversation in local FTS5."""
    conn = _get_fts_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        now = datetime.now(timezone.utc).isoformat()
        conn.execute(
            """
            INSERT INTO conversations_fts (conversation_id, title, transcript, uid, created_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(conversation_id) DO UPDATE SET
                title=excluded.title,
                transcript=excluded.transcript,
                created_at=excluded.created_at
            """,
            (conversation_id, title, transcript or "", uid, now),
        )
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to upsert conversation FTS: {e}")
        return False


def search_conversations_fts(uid: str, query: str, limit: int = 5) -> List[str]:
    """Lexical search over conversations. Returns list of conversation_ids."""
    conn = _get_fts_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        cursor = conn.execute(
            """
            SELECT conversation_id FROM conversations_fts
            WHERE uid = ? AND conversations_fts MATCH ?
            ORDER BY bm25(conversations_fts) LIMIT ?
            """,
            (uid, query, limit),
        )
        return [row[0] for row in cursor.fetchall()]
    except Exception as e:
        logger.warning(f"conversations_fts search failed: {e}")
        return []


# ── Screen Activity FTS ──────────────────────────────────────────────────────

def upsert_screen_activity_fts(
    uid: str,
    screenshot_id: str,
    ocr_text: str,
    app_name: str,
    window_title: str,
    timestamp: str,
) -> bool:
    """Index a screen activity row in local FTS5."""
    conn = _get_fts_conn()
    if conn is None:
        return False
    try:
        _ensure_schema(conn)
        conn.execute(
            """
            INSERT INTO screen_activity_fts
                (screenshot_id, ocr_text, app_name, window_title, uid, timestamp)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(screenshot_id) DO UPDATE SET
                ocr_text=excluded.ocr_text,
                app_name=excluded.app_name,
                window_title=excluded.window_title,
                timestamp=excluded.timestamp
            """,
            (screenshot_id, ocr_text or "", app_name or "", window_title or "", uid, timestamp),
        )
        conn.commit()
        return True
    except Exception as e:
        logger.warning(f"Failed to upsert screen_activity FTS: {e}")
        return False


def search_screen_activity_fts(
    uid: str,
    query: str,
    start_date: Optional[str] = None,
    end_date: Optional[str] = None,
    app_filter: Optional[str] = None,
    limit: int = 10,
) -> List[dict]:
    """
    Lexical search over screen activity OCR text.
    Returns list of dicts with screenshot_id, timestamp, app_name, score.
    """
    conn = _get_fts_conn()
    if conn is None:
        return []
    try:
        _ensure_schema(conn)
        conditions = ["uid = ?", "screen_activity_fts MATCH ?"]
        params: list = [uid, query]
        if start_date:
            conditions.append("timestamp >= ?")
            params.append(start_date)
        if end_date:
            conditions.append("timestamp <= ?")
            params.append(end_date)
        if app_filter:
            conditions.append("app_name = ?")
            params.append(app_filter)
        params.append(limit)
        sql = f"""
            SELECT screenshot_id, timestamp, app_name,
                   bm25(screen_activity_fts) as score
            FROM screen_activity_fts
            WHERE {' AND '.join(conditions)}
            ORDER BY score LIMIT ?
        """
        cursor = conn.execute(sql, params)
        return [
            {
                "screenshot_id": row[0],
                "timestamp": row[1],
                "app_name": row[2],
                "score": -row[3],  # BM25 lower = better; negate for consistency
            }
            for row in cursor.fetchall()
        ]
    except Exception as e:
        logger.warning(f"screen_activity_fts search failed: {e}")
        return []
