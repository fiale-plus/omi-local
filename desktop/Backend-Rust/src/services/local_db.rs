// Local SQLite database for LOCAL_MODE (offline-first without Firestore)
// Stores all data locally; sync to Firestore when online.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use ulid::Ulid;

use crate::models::{
    ActionItemDB, ActionItemsListResponse, BatchUpdateScoresRequest, ChatSessionDB,
    Conversation, CreateActionItemRequest, CreateConversationRequest, CreateFolderRequest,
    CreateGoalRequest, CreateMemoryRequest, CreateMemoryResponse, DailyScore, DailyScoreQuery,
    Event, Folder, GoalDB, GoalsListResponse, Memory, MemoryDB, MessageDB, ScoreData,
    ScoreResponse,
};
use crate::models::screen_activity::ScreenActivityRow;

/// Staged task row type (matches the staged_tasks table schema).
/// Defined here since no separate staged_task.rs model exists.
#[derive(Debug, Clone)]
pub struct StagedTaskDB {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub description: String,
    pub relevance_score: i32,
    pub created_at: String,
    pub metadata: Option<String>,
}

/// Handle to the local SQLite database
#[derive(Clone)]
pub struct LocalDb {
    pool: Arc<RwLock<Connection>>,
}

impl LocalDb {
    /// Open (or create) the local SQLite database at the given path.
    pub async fn open(path: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { pool: Arc::new(RwLock::new(conn)) })
    }

    /// Open an in-memory database (for testing).
    #[allow(dead_code)]
    pub async fn in_memory() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { pool: Arc::new(RwLock::new(conn)) })
    }

    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                folder_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_message_time TEXT,
                conversation_type TEXT NOT NULL DEFAULT 'personal',
                metadata TEXT,
                is_starred INTEGER NOT NULL DEFAULT 0,
                visibility TEXT NOT NULL DEFAULT 'private',
                share_link TEXT,
                transcript TEXT
            );

            CREATE TABLE IF NOT EXISTS folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '9b9b9b',
                icon TEXT NOT NULL DEFAULT 'folder',
                created_at TEXT NOT NULL,
                sort_order INTEGER NOT NULL DEFAULT 0,
                user_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                content TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_read INTEGER NOT NULL DEFAULT 0,
                is_dismissed INTEGER NOT NULL DEFAULT 0,
                visibility TEXT NOT NULL DEFAULT 'private',
                relevance_score INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS action_items (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                due_date TEXT,
                completed_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                relevance_score INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'manual',
                metadata TEXT
            );

            CREATE TABLE IF NOT EXISTS staged_tasks (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                relevance_score INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                metadata TEXT
            );

            CREATE TABLE IF NOT EXISTS goals (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                goal_type TEXT NOT NULL DEFAULT 'binary',
                target_date TEXT,
                current_value REAL NOT NULL DEFAULT 0,
                target_value REAL NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT
            );

            CREATE TABLE IF NOT EXISTS chat_sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                app_id TEXT
            );

            CREATE TABLE IF NOT EXISTS user_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS focus_sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                start_time TEXT NOT NULL,
                end_time TEXT,
                duration_secs INTEGER NOT NULL DEFAULT 0,
                distraction_count INTEGER NOT NULL DEFAULT 0,
                metadata TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_conversations_folder ON conversations(folder_id);
            CREATE INDEX IF NOT EXISTS idx_conversations_starred ON conversations(is_starred);
            CREATE INDEX IF NOT EXISTS idx_memories_user ON memories(user_id);
            CREATE INDEX IF NOT EXISTS idx_action_items_user ON action_items(user_id);
            CREATE INDEX IF NOT EXISTS idx_action_items_status ON action_items(status);
            CREATE INDEX IF NOT EXISTS idx_staged_tasks_user ON staged_tasks(user_id);
            CREATE INDEX IF NOT EXISTS idx_goals_user ON goals(user_id);
            CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_chat_sessions_user ON chat_sessions(user_id);

            CREATE TABLE IF NOT EXISTS screen_activity (
                id INTEGER PRIMARY KEY,
                user_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                app_name TEXT NOT NULL DEFAULT '',
                window_title TEXT NOT NULL DEFAULT '',
                ocr_text TEXT NOT NULL DEFAULT '',
                embedding BLOB
            );

            CREATE INDEX IF NOT EXISTS idx_screen_activity_user ON screen_activity(user_id);
            "#,
        )
    }

    // ── Conversations ────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_conversations(&self, user_id: &str) -> Result<Vec<Conversation>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, title, folder_id, created_at, updated_at, last_message_time, conversation_type, metadata, is_starred, visibility, share_link, transcript FROM conversations WHERE 1=1 ORDER BY last_message_time DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                folder_id: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                last_message_time: row.get(5)?,
                conversation_type: row.get(6)?,
                metadata: row.get(7)?,
                is_starred: row.get::<_, i32>(8)? != 0,
                visibility: row.get(9)?,
                share_link: row.get(10)?,
                transcript: row.get(11)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn get_conversation(&self, user_id: &str, id: &str) -> Result<Option<Conversation>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, title, folder_id, created_at, updated_at, last_message_time, conversation_type, metadata, is_starred, visibility, share_link, transcript FROM conversations WHERE id = ?",
        )?;
        let mut rows = stmt.query_map([id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                folder_id: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                last_message_time: row.get(5)?,
                conversation_type: row.get(6)?,
                metadata: row.get(7)?,
                is_starred: row.get::<_, i32>(8)? != 0,
                visibility: row.get(9)?,
                share_link: row.get(10)?,
                transcript: row.get(11)?,
            })
        })?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    #[allow(dead_code)]
    pub async fn create_conversation(&self, user_id: &str, req: CreateConversationRequest) -> Result<Conversation, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO conversations (id, title, folder_id, created_at, updated_at, last_message_time, conversation_type, metadata, is_starred, visibility) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![id, req.title, req.folder_id, now, now, now, req.conversation_type.unwrap_or_else(|| "personal".to_string()), "", 0, "private"],
        )?;
        drop(pool);
        self.get_conversation(user_id, &id).await?.ok_or_else(|| "created conversation not found".into())
    }

    #[allow(dead_code)]
    pub async fn update_conversation_title(&self, user_id: &str, id: &str, title: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        pool.execute("UPDATE conversations SET title = ?, updated_at = ? WHERE id = ?", params![title, now, id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn set_conversation_starred(&self, user_id: &str, id: &str, starred: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("UPDATE conversations SET is_starred = ? WHERE id = ?", params![starred as i32, id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn set_conversation_visibility(&self, user_id: &str, id: &str, visibility: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("UPDATE conversations SET visibility = ? WHERE id = ?", params![visibility, id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn delete_conversation(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM conversations WHERE id = ?", [id])?;
        Ok(())
    }

    // ── Folders ──────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_folders(&self, user_id: &str) -> Result<Vec<Folder>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, name, color, icon, created_at, sort_order FROM folders WHERE user_id = ? ORDER BY sort_order",
        )?;
        let rows = stmt.query_map([user_id], |row| {
            Ok(Folder {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                icon: row.get(3)?,
                created_at: row.get(4)?,
                sort_order: row.get(5)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn create_folder(&self, user_id: &str, req: CreateFolderRequest) -> Result<Folder, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO folders (id, name, color, icon, created_at, sort_order, user_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![id, req.name, req.color.unwrap_or_else(|| "9b9b9b".to_string()), req.icon.unwrap_or_else(|| "folder".to_string()), now, 0, user_id],
        )?;
        drop(pool);
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare("SELECT id, name, color, icon, created_at, sort_order FROM folders WHERE id = ?")?;
        let mut rows = stmt.query_map([&id], |row| {
            Ok(Folder { id: row.get(0)?, name: row.get(1)?, color: row.get(2)?, icon: row.get(3)?, created_at: row.get(4)?, sort_order: row.get(5)? })
        })?;
        rows.next().unwrap().map_err(Into::into)
    }

    #[allow(dead_code)]
    pub async fn delete_folder(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM folders WHERE id = ? AND user_id = ?", params![id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn move_conversation_to_folder(&self, user_id: &str, conversation_id: &str, folder_id: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("UPDATE conversations SET folder_id = ? WHERE id = ?", params![folder_id, conversation_id])?;
        Ok(())
    }

    // ── Memories ─────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_memories(&self, user_id: &str, limit: i32, offset: i32) -> Result<Vec<Memory>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, content, category, created_at, updated_at, is_read, is_dismissed, visibility, relevance_score FROM memories WHERE user_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )?;
        let rows = stmt.query_map(params![user_id, limit, offset], |row| {
            Ok(Memory {
                id: row.get(0)?,
                user_id: row.get(1)?,
                content: row.get(2)?,
                category: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                is_read: row.get::<_, i32>(6)? != 0,
                is_dismissed: row.get::<_, i32>(7)? != 0,
                visibility: row.get(8)?,
                relevance_score: row.get(9)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn create_memory(&self, user_id: &str, req: CreateMemoryRequest) -> Result<CreateMemoryResponse, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO memories (id, user_id, content, category, created_at, updated_at, visibility, relevance_score) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![id, user_id, req.content, req.category.unwrap_or_else(|| "general".to_string()), now, now, req.visibility.unwrap_or_else(|| "private".to_string()), 0],
        )?;
        Ok(CreateMemoryResponse { id })
    }

    #[allow(dead_code)]
    pub async fn delete_memory(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM memories WHERE id = ? AND user_id = ?", params![id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn update_memory_read_status(&self, user_id: &str, id: &str, is_read: bool, is_dismissed: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute(
            "UPDATE memories SET is_read = ?, is_dismissed = ? WHERE id = ? AND user_id = ?",
            params![is_read as i32, is_dismissed as i32, id, user_id],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn mark_all_memories_read(&self, user_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("UPDATE memories SET is_read = 1 WHERE user_id = ?", [user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn delete_all_memories(&self, user_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM memories WHERE user_id = ?", [user_id])?;
        Ok(())
    }

    // ── Action Items ─────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_action_items(&self, user_id: &str, limit: i32, offset: i32) -> Result<ActionItemsListResponse, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, title, description, due_date, completed_at, created_at, updated_at, status, relevance_score, source, metadata FROM action_items WHERE user_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )?;
        let rows = stmt.query_map(params![user_id, limit, offset], |row| {
            Ok(ActionItemDB {
                id: row.get(0)?,
                user_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                due_date: row.get(4)?,
                completed_at: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                status: row.get(8)?,
                relevance_score: row.get(9)?,
                source: row.get(10)?,
                metadata: row.get(11)?,
            })
        })?;
        let mut items = Vec::new();
        for r in rows { items.push(r?); }
        drop(pool);
        Ok(ActionItemsListResponse { items })
    }

    #[allow(dead_code)]
    pub async fn get_action_item_by_id(&self, user_id: &str, id: &str) -> Result<Option<ActionItemDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, title, description, due_date, completed_at, created_at, updated_at, status, relevance_score, source, metadata FROM action_items WHERE id = ? AND user_id = ?",
        )?;
        let mut rows = stmt.query_map(params![id, user_id], |row| {
            Ok(ActionItemDB {
                id: row.get(0)?, user_id: row.get(1)?, title: row.get(2)?, description: row.get(3)?,
                due_date: row.get(4)?, completed_at: row.get(5)?, created_at: row.get(6)?,
                updated_at: row.get(7)?, status: row.get(8)?, relevance_score: row.get(9)?,
                source: row.get(10)?, metadata: row.get(11)?,
            })
        })?;
        if let Some(r) = rows.next() { Ok(Some(r?)) } else { Ok(None) }
    }

    #[allow(dead_code)]
    pub async fn create_action_item(&self, user_id: &str, req: CreateActionItemRequest) -> Result<ActionItemDB, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO action_items (id, user_id, title, description, due_date, created_at, updated_at, status, relevance_score, source, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![id, user_id, req.title, req.description.unwrap_or_default(), req.due_date, now, now, "pending", req.relevance_score.unwrap_or(0), req.source.unwrap_or_else(|| "manual".to_string()), ""],
        )?;
        drop(pool);
        self.get_action_item_by_id(user_id, &id).await?.ok_or_else(|| "created action item not found".into())
    }

    #[allow(dead_code)]
    pub async fn update_action_item(&self, user_id: &str, id: &str, title: Option<&str>, description: Option<&str>, due_date: Option<&str>, status: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        if let Some(t) = title {
            pool.execute("UPDATE action_items SET title = ?, updated_at = ? WHERE id = ? AND user_id = ?", params![t, now, id, user_id])?;
        }
        if let Some(d) = description {
            pool.execute("UPDATE action_items SET description = ?, updated_at = ? WHERE id = ? AND user_id = ?", params![d, now, id, user_id])?;
        }
        if let Some(dd) = due_date {
            pool.execute("UPDATE action_items SET due_date = ?, updated_at = ? WHERE id = ? AND user_id = ?", params![dd, now, id, user_id])?;
        }
        if let Some(s) = status {
            let completed_at = if s == "completed" { Some(now.clone()) } else { None };
            pool.execute("UPDATE action_items SET status = ?, completed_at = ?, updated_at = ? WHERE id = ? AND user_id = ?", params![s, completed_at, now, id, user_id])?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn delete_action_item(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM action_items WHERE id = ? AND user_id = ?", params![id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn batch_update_scores(&self, user_id: &str, scores: &[(String, i32)]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        for (id, score) in scores {
            pool.execute("UPDATE action_items SET relevance_score = ? WHERE id = ? AND user_id = ?", params![score, id, user_id])?;
        }
        Ok(())
    }

    // ── Staged Tasks ─────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_staged_tasks(&self, user_id: &str, limit: i32, offset: i32) -> Result<Vec<StagedTaskDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, title, description, relevance_score, created_at, metadata FROM staged_tasks WHERE user_id = ? ORDER BY relevance_score DESC LIMIT ? OFFSET ?",
        )?;
        let rows = stmt.query_map(params![user_id, limit, offset], |row| {
            Ok(StagedTaskDB {
                id: row.get(0)?,
                user_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                relevance_score: row.get(4)?,
                created_at: row.get(5)?,
                metadata: row.get(6)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn create_staged_task(&self, user_id: &str, title: &str, description: &str, relevance_score: i32) -> Result<StagedTaskDB, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO staged_tasks (id, user_id, title, description, relevance_score, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![id, user_id, title, description, relevance_score, now, ""],
        )?;
        drop(pool);
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare("SELECT id, user_id, title, description, relevance_score, created_at, metadata FROM staged_tasks WHERE id = ?")?;
        let mut rows = stmt.query_map([&id], |row| {
            Ok(StagedTaskDB { id: row.get(0)?, user_id: row.get(1)?, title: row.get(2)?, description: row.get(3)?, relevance_score: row.get(4)?, created_at: row.get(5)?, metadata: row.get(6)? })
        })?;
        rows.next().unwrap().map_err(Into::into)
    }

    #[allow(dead_code)]
    pub async fn delete_staged_task(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM staged_tasks WHERE id = ? AND user_id = ?", params![id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn batch_update_staged_scores(&self, user_id: &str, scores: &[(String, i32)]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        for (id, score) in scores {
            pool.execute("UPDATE staged_tasks SET relevance_score = ? WHERE id = ? AND user_id = ?", params![score, id, user_id])?;
        }
        Ok(())
    }

    // ── Goals ─────────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_goals(&self, user_id: &str) -> Result<Vec<GoalDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, title, goal_type, target_date, current_value, target_value, status, created_at, updated_at, metadata FROM goals WHERE user_id = ? AND status = 'active' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([user_id], |row| {
            Ok(GoalDB {
                id: row.get(0)?,
                user_id: row.get(1)?,
                title: row.get(2)?,
                goal_type: row.get(3)?,
                target_date: row.get(4)?,
                current_value: row.get(5)?,
                target_value: row.get(6)?,
                status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                metadata: row.get(10)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn create_goal(&self, user_id: &str, req: CreateGoalRequest) -> Result<GoalDB, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO goals (id, user_id, title, goal_type, target_date, current_value, target_value, status, created_at, updated_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![id, user_id, req.title, req.goal_type.unwrap_or_else(|| "binary".to_string()), req.target_date, 0.0, req.target_value.unwrap_or(1.0), "active", now, now, ""],
        )?;
        drop(pool);
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare("SELECT id, user_id, title, goal_type, target_date, current_value, target_value, status, created_at, updated_at, metadata FROM goals WHERE id = ?")?;
        let mut rows = stmt.query_map([&id], |row| {
            Ok(GoalDB { id: row.get(0)?, user_id: row.get(1)?, title: row.get(2)?, goal_type: row.get(3)?, target_date: row.get(4)?, current_value: row.get(5)?, target_value: row.get(6)?, status: row.get(7)?, created_at: row.get(8)?, updated_at: row.get(9)?, metadata: row.get(10)? })
        })?;
        rows.next().unwrap().map_err(Into::into)
    }

    #[allow(dead_code)]
    pub async fn update_goal_progress(&self, user_id: &str, id: &str, current_value: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        pool.execute("UPDATE goals SET current_value = ?, updated_at = ? WHERE id = ? AND user_id = ?", params![current_value, now, id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn complete_goal(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        pool.execute("UPDATE goals SET status = 'completed', current_value = target_value, updated_at = ? WHERE id = ? AND user_id = ?", params![now, id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn delete_goal(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM goals WHERE id = ? AND user_id = ?", params![id, user_id])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_completed_goals(&self, user_id: &str) -> Result<Vec<GoalDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, title, goal_type, target_date, current_value, target_value, status, created_at, updated_at, metadata FROM goals WHERE user_id = ? AND status = 'completed' ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([user_id], |row| {
            Ok(GoalDB {
                id: row.get(0)?, user_id: row.get(1)?, title: row.get(2)?, goal_type: row.get(3)?,
                target_date: row.get(4)?, current_value: row.get(5)?, target_value: row.get(6)?,
                status: row.get(7)?, created_at: row.get(8)?, updated_at: row.get(9)?, metadata: row.get(10)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    // ── Scores ────────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_scores(&self, user_id: &str, query: DailyScoreQuery) -> Result<ScoreResponse, Box<dyn std::error::Error + Send + Sync>> {
        let date = match &query.date {
            Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap_or_else(|_| Utc::now().date_naive()),
            None => Utc::now().date_naive(),
        };
        let date_str = date.format("%Y-%m-%d").to_string();
        let today_start = format!("{}T00:00:00Z", date_str);
        let today_end = format!("{}T23:59:59.999Z", date_str);
        let week_ago = date - chrono::Duration::days(7);
        let week_start = format!("{}T00:00:00Z", week_ago.format("%Y-%m-%d"));

        let pool = self.pool.read().await;

        // Daily score: completed/total action items due today
        let today_completed: i32 = pool.query_row(
            "SELECT COUNT(*) FROM action_items WHERE user_id = ? AND status = 'completed' AND due_date >= ? AND due_date <= ?",
            params![user_id, today_start, today_end],
            |r| r.get(0),
        ).unwrap_or(0);
        let today_total: i32 = pool.query_row(
            "SELECT COUNT(*) FROM action_items WHERE user_id = ? AND due_date >= ? AND due_date <= ?",
            params![user_id, today_start, today_end],
            |r| r.get(0),
        ).unwrap_or(0);

        // Weekly score
        let week_completed: i32 = pool.query_row(
            "SELECT COUNT(*) FROM action_items WHERE user_id = ? AND status = 'completed' AND due_date >= ? AND due_date <= ?",
            params![user_id, week_start, today_end],
            |r| r.get(0),
        ).unwrap_or(0);
        let week_total: i32 = pool.query_row(
            "SELECT COUNT(*) FROM action_items WHERE user_id = ? AND due_date >= ? AND due_date <= ?",
            params![user_id, week_start, today_end],
            |r| r.get(0),
        ).unwrap_or(0);

        // Overall score
        let overall_completed: i32 = pool.query_row(
            "SELECT COUNT(*) FROM action_items WHERE user_id = ? AND status = 'completed'",
            [user_id],
            |r| r.get(0),
        ).unwrap_or(0);
        let overall_total: i32 = pool.query_row(
            "SELECT COUNT(*) FROM action_items WHERE user_id = ?",
            [user_id],
            |r| r.get(0),
        ).unwrap_or(0);

        let make_score = |completed, total| ScoreData {
            score: if total > 0 { (completed as f64 / total as f64) * 100.0 } else { 0.0 },
            completed_tasks: completed,
            total_tasks: total,
        };

        let daily = make_score(today_completed, today_total);
        let weekly = make_score(week_completed, week_total);
        let overall = make_score(overall_completed, overall_total);

        let default_tab = if daily.total_tasks > 0 && daily.score >= weekly.score && daily.score >= overall.score {
            "daily"
        } else if weekly.score >= overall.score { "weekly" } else { "overall" }.to_string();

        Ok(ScoreResponse { daily, weekly, overall, default_tab, date: date_str })
    }

    // ── Messages ─────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_messages(&self, conversation_id: &str, limit: i32, offset: i32) -> Result<Vec<MessageDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, conversation_id, role, content, created_at, updated_at, metadata FROM messages WHERE conversation_id = ? ORDER BY created_at ASC LIMIT ? OFFSET ?",
        )?;
        let rows = stmt.query_map(params![conversation_id, limit, offset], |row| {
            Ok(MessageDB {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                metadata: row.get(6)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn save_message(&self, conversation_id: &str, role: &str, content: &str) -> Result<MessageDB, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at, updated_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![id, conversation_id, role, content, now, now, ""],
        )?;
        drop(pool);
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare("SELECT id, conversation_id, role, content, created_at, updated_at, metadata FROM messages WHERE id = ?")?;
        let mut rows = stmt.query_map([&id], |row| {
            Ok(MessageDB { id: row.get(0)?, conversation_id: row.get(1)?, role: row.get(2)?, content: row.get(3)?, created_at: row.get(4)?, updated_at: row.get(5)?, metadata: row.get(6)? })
        })?;
        rows.next().unwrap().map_err(Into::into)
    }

    // ── Chat Sessions ────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_chat_sessions(&self, user_id: &str) -> Result<Vec<ChatSessionDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, title, created_at, updated_at, app_id FROM chat_sessions WHERE user_id = ? ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([user_id], |row| {
            Ok(ChatSessionDB {
                id: row.get(0)?,
                user_id: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                app_id: row.get(5)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn create_chat_session(&self, user_id: &str, title: &str, app_id: Option<&str>) -> Result<ChatSessionDB, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let now = Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        pool.execute(
            "INSERT INTO chat_sessions (id, user_id, title, created_at, updated_at, app_id) VALUES (?, ?, ?, ?, ?, ?)",
            params![id, user_id, title, now, now, app_id],
        )?;
        drop(pool);
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare("SELECT id, user_id, title, created_at, updated_at, app_id FROM chat_sessions WHERE id = ?")?;
        let mut rows = stmt.query_map([&id], |row| {
            Ok(ChatSessionDB { id: row.get(0)?, user_id: row.get(1)?, title: row.get(2)?, created_at: row.get(3)?, updated_at: row.get(4)?, app_id: row.get(5)? })
        })?;
        rows.next().unwrap().map_err(Into::into)
    }

    #[allow(dead_code)]
    pub async fn delete_chat_session(&self, user_id: &str, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        pool.execute("DELETE FROM chat_sessions WHERE id = ? AND user_id = ?", params![id, user_id])?;
        Ok(())
    }

    // ── Focus Sessions ───────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_focus_sessions(&self, user_id: &str, start_date: Option<&str>, end_date: Option<&str>) -> Result<Vec<crate::models::FocusSessionDB>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare(
            "SELECT id, user_id, start_time, end_time, duration_secs, distraction_count, metadata FROM focus_sessions WHERE user_id = ? ORDER BY start_time DESC",
        )?;
        let rows = stmt.query_map([user_id], |row| {
            Ok(crate::models::FocusSessionDB {
                id: row.get(0)?,
                user_id: row.get(1)?,
                start_time: row.get(2)?,
                end_time: row.get(3)?,
                duration_secs: row.get(4)?,
                distraction_count: row.get(5)?,
                metadata: row.get(6)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows { result.push(r?); }
        Ok(result)
    }

    // ── User Settings ────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn get_user_profile(&self, user_id: &str) -> Result<Option<crate::models::UserProfile>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut stmt = pool.prepare("SELECT value FROM user_settings WHERE key = ?")?;
        let mut rows = stmt.query_map([format!("profile_{}", user_id)], |row| {
            let value: String = row.get(0)?;
            Ok(value)
        })?;
        if let Some(r) = rows.next() {
            let value = r?;
            let profile: crate::models::UserProfile = serde_json::from_str(&value).map_err(|e| e.to_string())?;
            Ok(Some(profile))
        } else {
            Ok(None)
        }
    }

    #[allow(dead_code)]
    pub async fn save_user_profile(&self, user_id: &str, profile: &crate::models::UserProfile) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let json = serde_json::to_string(profile).map_err(|e| e.to_string())?;
        pool.execute(
            "INSERT OR REPLACE INTO user_settings (key, value) VALUES (?, ?)",
            params![format!("profile_{}", user_id), json],
        )?;
        Ok(())
    }

    // ── Screen Activity ─────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn upsert_screen_activity(
        &self,
        user_id: &str,
        rows: &[crate::models::screen_activity::ScreenActivityRow],
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut written = 0;
        for row in rows {
            let embedding_blob: Option<Vec<u8>> = row.embedding.as_ref().map(|v| {
                // Serialize Vec<f64> as bytes for storage
                let mut bytes = Vec::with_capacity(v.len() * 8);
                for f in v {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                bytes
            });
            pool.execute(
                r#"INSERT INTO screen_activity (id, user_id, timestamp, app_name, window_title, ocr_text, embedding)
                   VALUES (?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(id) DO UPDATE SET
                       timestamp=excluded.timestamp,
                       app_name=excluded.app_name,
                       window_title=excluded.window_title,
                       ocr_text=excluded.ocr_text,
                       embedding=excluded.embedding"#,
                params![
                    row.id,
                    user_id,
                    row.timestamp,
                    row.app_name,
                    row.window_title,
                    row.ocr_text,
                    embedding_blob,
                ],
            )?;
            written += 1;
        }
        Ok(written)
    }

    #[allow(dead_code)]
    pub async fn get_screen_activity(
        &self,
        user_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        app_filter: Option<&str>,
        limit: i32,
    ) -> Result<Vec<crate::models::screen_activity::ScreenActivityRow>, Box<dyn std::error::Error + Send + Sync>> {
        let pool = self.pool.read().await;
        let mut sql = "SELECT id, timestamp, app_name, window_title, ocr_text, embedding FROM screen_activity WHERE user_id = ?".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(user_id.to_string())];

        if let Some(start) = start_date {
            sql.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(start.to_string()));
        }
        if let Some(end) = end_date {
            sql.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(end.to_string()));
        }
        if let Some(app) = app_filter {
            sql.push_str(" AND app_name = ?");
            params_vec.push(Box::new(app.to_string()));
        }
        sql.push_str(" ORDER BY timestamp ASC LIMIT ?");
        params_vec.push(Box::new(limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = pool.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let embedding_bytes: Option<Vec<u8>> = row.get(5)?;
            let embedding: Option<Vec<f64>> = embedding_bytes.map(|bytes| {
                bytes
                    .chunks_exact(8)
                    .map(|chunk| f64::from_le_bytes(chunk.try_into().unwrap()))
                    .collect()
            });
            Ok(crate::models::screen_activity::ScreenActivityRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                app_name: row.get(2)?,
                window_title: row.get(3)?,
                ocr_text: row.get(4)?,
                embedding,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }
}