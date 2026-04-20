// SQLite storage backend
// Provides local SQLite storage as an alternative to Firestore

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::traits::*;
use crate::repository::migrations::{run_migrations, CURRENT_SCHEMA_VERSION};

/// SQLite storage backend
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl SqliteStorage {
    /// Create a new SQLite storage instance
    pub async fn new(db_path: PathBuf) -> RepoResult<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        
        // Enable foreign keys and WAL mode for better concurrency
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;"
        )?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        };

        // Run migrations
        {
            let conn = storage.conn.lock().await;
            run_migrations(&conn)?;
        }

        Ok(storage)
    }

    /// Get the current schema version
    pub async fn get_schema_version(&self) -> RepoResult<i32> {
        let conn = self.conn.lock().await;
        let version: i32 = conn.query_row(
            "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        Ok(version)
    }

    /// Close the database connection
    pub async fn close(&self) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.close();
        Ok(())
    }
}

// ============================================================================
// Memory Repository Implementation
// ============================================================================

#[async_trait]
impl MemoryRepository for SqliteStorage {
    async fn create_memory(&self, uid: &str, memory: MemoryInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        let category = memory.category.unwrap_or_else(|| "manual".to_string());
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO memories (id, uid, content, category, created_at, updated_at, 
                visibility, manually_added, tags, reasoning, current_activity, 
                source, window_title, source_app, context_summary, confidence)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
            params![
                id,
                uid,
                memory.content,
                category,
                now,
                now,
                memory.visibility,
                true, // manually_added
                serde_json::to_string(&memory.tags)?,
                memory.reasoning,
                memory.current_activity,
                memory.source,
                memory.window_title,
                memory.source_app,
                memory.context_summary,
                memory.confidence,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_memory(&self, uid: &str, memory_id: &str) -> RepoResult<MemoryOutput> {
        let conn = self.conn.lock().await;
        let memory = conn.query_row(
            r#"
            SELECT id, uid, content, category, created_at, updated_at, conversation_id,
                reviewed, user_review, visibility, manually_added, scoring, confidence,
                source_app, context_summary, is_read, is_dismissed, tags, reasoning,
                current_activity, window_title
            FROM memories WHERE id = ?1 AND uid = ?2
            "#,
            params![memory_id, uid],
            |row| {
                Ok(MemoryOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    content: row.get(2)?,
                    category: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    conversation_id: row.get(6)?,
                    reviewed: row.get(7)?,
                    user_review: row.get(8)?,
                    visibility: row.get(9)?,
                    manually_added: row.get(10)?,
                    scoring: row.get(11)?,
                    confidence: row.get(12)?,
                    source_app: row.get(13)?,
                    context_summary: row.get(14)?,
                    is_read: row.get(15)?,
                    is_dismissed: row.get(16)?,
                    tags: serde_json::from_str(&row.get::<_, String>(17)?).unwrap_or_default(),
                    reasoning: row.get(18)?,
                    current_activity: row.get(19)?,
                    window_title: row.get(20)?,
                })
            },
        ).map_err(|e| format!("Memory not found: {}", e))?;
        
        Ok(memory)
    }

    async fn list_memories(&self, uid: &str, filter: QueryFilter, pagination: Pagination) -> RepoResult<Vec<MemoryOutput>> {
        let conn = self.conn.lock().await;
        
        let mut sql = String::from(
            r#"
            SELECT id, uid, content, category, created_at, updated_at, conversation_id,
                reviewed, user_review, visibility, manually_added, scoring, confidence,
                source_app, context_summary, is_read, is_dismissed, tags, reasoning,
                current_activity, window_title
            FROM memories WHERE uid = ?1
            "#
        );
        
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(uid.to_string())];
        let mut param_idx = 2;
        
        if !filter.include_dismissed {
            sql.push_str(" AND is_dismissed = 0");
        }
        
        if let Some(ref cat) = filter.category {
            sql.push_str(&format!(" AND category = ?{}", param_idx));
            params_vec.push(Box::new(cat.clone()));
            param_idx += 1;
        }
        
        if let Some(ref tags) = filter.tags {
            sql.push_str(&format!(" AND tags LIKE ?{}", param_idx));
            params_vec.push(Box::new(format!("%{}%", tags)));
            param_idx += 1;
        }
        
        sql.push_str(" ORDER BY created_at DESC");
        sql.push_str(&format!(" LIMIT {} OFFSET {}", pagination.limit, pagination.offset));
        
        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        
        let memories = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(MemoryOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                content: row.get(2)?,
                category: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                conversation_id: row.get(6)?,
                reviewed: row.get(7)?,
                user_review: row.get(8)?,
                visibility: row.get(9)?,
                manually_added: row.get(10)?,
                scoring: row.get(11)?,
                confidence: row.get(12)?,
                source_app: row.get(13)?,
                context_summary: row.get(14)?,
                is_read: row.get(15)?,
                is_dismissed: row.get(16)?,
                tags: serde_json::from_str(&row.get::<_, String>(17)?).unwrap_or_default(),
                reasoning: row.get(18)?,
                current_activity: row.get(19)?,
                window_title: row.get(20)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(memories)
    }

    async fn update_memory(&self, uid: &str, memory_id: &str, update: MemoryUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE memories SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref content) = update.content {
            sql.push_str(&format!(", content = ?{}", param_idx));
            params_vec.push(Box::new(content.clone()));
            param_idx += 1;
        }
        
        if let Some(ref visibility) = update.visibility {
            sql.push_str(&format!(", visibility = ?{}", param_idx));
            params_vec.push(Box::new(visibility.clone()));
            param_idx += 1;
        }
        
        if let Some(is_read) = update.is_read {
            sql.push_str(&format!(", is_read = ?{}", param_idx));
            params_vec.push(Box::new(is_read));
            param_idx += 1;
        }
        
        if let Some(is_dismissed) = update.is_dismissed {
            sql.push_str(&format!(", is_dismissed = ?{}", param_idx));
            params_vec.push(Box::new(is_dismissed));
            param_idx += 1;
        }
        
        if let Some(ref tags) = update.tags {
            sql.push_str(&format!(", tags = ?{}", param_idx));
            params_vec.push(Box::new(serde_json::to_string(tags)?));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(memory_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_memory(&self, uid: &str, memory_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM memories WHERE id = ?1 AND uid = ?2",
            params![memory_id, uid],
        )?;
        Ok(())
    }

    async fn review_memory(&self, uid: &str, memory_id: &str, approved: bool) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        conn.execute(
            "UPDATE memories SET reviewed = 1, user_review = ?1, updated_at = ?2 WHERE id = ?3 AND uid = ?4",
            params![approved, now, memory_id, uid],
        )?;
        Ok(())
    }
}

// ============================================================================
// Conversation Repository Implementation
// ============================================================================

#[async_trait]
impl ConversationRepository for SqliteStorage {
    async fn create_conversation(&self, uid: &str, conversation: ConversationInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let geolocation_json = conversation.geolocation.as_ref()
            .map(|g| serde_json::to_string(g).ok())
            .flatten();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO conversations (id, uid, title, status, created_at, updated_at, 
                source, geolocation, last_message_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                id,
                uid,
                conversation.title,
                "active",
                now,
                now,
                conversation.source,
                geolocation_json,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_conversation(&self, uid: &str, conversation_id: &str) -> RepoResult<ConversationOutput> {
        let conn = self.conn.lock().await;
        let conv = conn.query_row(
            r#"
            SELECT id, uid, title, status, created_at, updated_at, source, geolocation, last_message_at
            FROM conversations WHERE id = ?1 AND uid = ?2
            "#,
            params![conversation_id, uid],
            |row| {
                let geolocation_json: Option<String> = row.get(7)?;
                let geolocation = geolocation_json.and_then(|g| {
                    serde_json::from_str::<GeolocationInput>(&g).ok()
                }).map(|g| GeolocationOutput {
                    latitude: g.latitude,
                    longitude: g.longitude,
                    place_name: g.place_name,
                });
                
                Ok(ConversationOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    source: row.get(6)?,
                    geolocation,
                    last_message_at: row.get(8)?,
                })
            },
        ).map_err(|e| format!("Conversation not found: {}", e))?;
        
        Ok(conv)
    }

    async fn list_conversations(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<ConversationOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, title, status, created_at, updated_at, source, geolocation, last_message_at
            FROM conversations WHERE uid = ?1
            ORDER BY last_message_at DESC NULLS LAST, updated_at DESC
            LIMIT ?2 OFFSET ?3
            "#
        )?;
        
        let conversations = stmt.query_map(params![uid, pagination.limit, pagination.offset], |row| {
            let geolocation_json: Option<String> = row.get(7)?;
            let geolocation = geolocation_json.and_then(|g| {
                serde_json::from_str::<GeolocationInput>(&g).ok()
            }).map(|g| GeolocationOutput {
                latitude: g.latitude,
                longitude: g.longitude,
                place_name: g.place_name,
            });
            
            Ok(ConversationOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                source: row.get(6)?,
                geolocation,
                last_message_at: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(conversations)
    }

    async fn update_conversation(&self, uid: &str, conversation_id: &str, update: ConversationUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE conversations SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref title) = update.title {
            sql.push_str(&format!(", title = ?{}", param_idx));
            params_vec.push(Box::new(title.clone()));
            param_idx += 1;
        }
        
        if let Some(ref status) = update.status {
            sql.push_str(&format!(", status = ?{}", param_idx));
            params_vec.push(Box::new(status.clone()));
            param_idx += 1;
        }
        
        if let Some(ref last_message_at) = update.last_message_at {
            sql.push_str(&format!(", last_message_at = ?{}", param_idx));
            params_vec.push(Box::new(last_message_at));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(conversation_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_conversation(&self, uid: &str, conversation_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM conversations WHERE id = ?1 AND uid = ?2",
            params![conversation_id, uid],
        )?;
        Ok(())
    }
}

// ============================================================================
// Message Repository Implementation
// ============================================================================

#[async_trait]
impl MessageRepository for SqliteStorage {
    async fn create_message(&self, uid: &str, conversation_id: &str, message: MessageInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO messages (id, uid, conversation_id, role, content, audio_url, 
                transcript, created_at, rating)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
            "#,
            params![
                id,
                uid,
                conversation_id,
                message.role,
                message.content,
                message.audio_url,
                message.transcript,
                now,
            ],
        )?;
        
        // Update conversation's last_message_at
        conn.execute(
            "UPDATE conversations SET last_message_at = ?1, updated_at = ?1 WHERE id = ?2 AND uid = ?3",
            params![now, conversation_id, uid],
        )?;
        
        Ok(id)
    }

    async fn get_message(&self, uid: &str, conversation_id: &str, message_id: &str) -> RepoResult<MessageOutput> {
        let conn = self.conn.lock().await;
        let message = conn.query_row(
            r#"
            SELECT id, uid, conversation_id, role, content, audio_url, transcript, created_at, rating
            FROM messages WHERE id = ?1 AND uid = ?2 AND conversation_id = ?3
            "#,
            params![message_id, uid, conversation_id],
            |row| {
                Ok(MessageOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    conversation_id: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    audio_url: row.get(5)?,
                    transcript: row.get(6)?,
                    created_at: row.get(7)?,
                    rating: row.get(8)?,
                })
            },
        ).map_err(|e| format!("Message not found: {}", e))?;
        
        Ok(message)
    }

    async fn list_messages(&self, uid: &str, conversation_id: &str, pagination: Pagination) -> RepoResult<Vec<MessageOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, conversation_id, role, content, audio_url, transcript, created_at, rating
            FROM messages WHERE uid = ?1 AND conversation_id = ?2
            ORDER BY created_at ASC
            LIMIT ?3 OFFSET ?4
            "#
        )?;
        
        let messages = stmt.query_map(params![uid, conversation_id, pagination.limit, pagination.offset], |row| {
            Ok(MessageOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                conversation_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                audio_url: row.get(5)?,
                transcript: row.get(6)?,
                created_at: row.get(7)?,
                rating: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(messages)
    }

    async fn update_message(&self, uid: &str, conversation_id: &str, message_id: &str, update: MessageUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        let mut sql = String::from("UPDATE messages SET 1 = 1"); // Dummy to start with AND
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        let mut param_idx = 1;
        
        // Note: SQLite doesn't allow true in UPDATE without values, so we restructure
        sql = String::from("UPDATE messages SET ".to_string());
        let mut updates = String::new();
        
        if let Some(ref content) = update.content {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("content = ?{}", param_idx));
            params_vec.push(Box::new(content.clone()));
            param_idx += 1;
        }
        
        if let Some(ref transcript) = update.transcript {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("transcript = ?{}", param_idx));
            params_vec.push(Box::new(transcript.clone()));
            param_idx += 1;
        }
        
        if updates.is_empty() {
            return Ok(()); // Nothing to update
        }
        
        sql.push_str(&updates);
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{} AND conversation_id = ?{}", param_idx, param_idx + 1, param_idx + 2));
        params_vec.push(Box::new(message_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        params_vec.push(Box::new(conversation_id.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_message(&self, uid: &str, conversation_id: &str, message_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM messages WHERE id = ?1 AND uid = ?2 AND conversation_id = ?3",
            params![message_id, uid, conversation_id],
        )?;
        Ok(())
    }

    async fn rate_message(&self, uid: &str, conversation_id: &str, message_id: &str, rating: i32) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE messages SET rating = ?1 WHERE id = ?2 AND uid = ?3 AND conversation_id = ?4",
            params![rating, message_id, uid, conversation_id],
        )?;
        Ok(())
    }
}

// ============================================================================
// Action Item Repository Implementation
// ============================================================================

#[async_trait]
impl ActionItemRepository for SqliteStorage {
    async fn create_action_item(&self, uid: &str, action_item: ActionItemInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO action_items (id, uid, content, completed, due_date, priority, 
                conversation_id, created_at, updated_at, score)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)
            "#,
            params![
                id,
                uid,
                action_item.content,
                action_item.completed,
                action_item.due_date,
                action_item.priority.unwrap_or(0),
                action_item.conversation_id,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_action_item(&self, uid: &str, action_item_id: &str) -> RepoResult<ActionItemOutput> {
        let conn = self.conn.lock().await;
        let item = conn.query_row(
            r#"
            SELECT id, uid, content, completed, due_date, priority, conversation_id, created_at, updated_at, score
            FROM action_items WHERE id = ?1 AND uid = ?2
            "#,
            params![action_item_id, uid],
            |row| {
                Ok(ActionItemOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    content: row.get(2)?,
                    completed: row.get(3)?,
                    due_date: row.get(4)?,
                    priority: row.get(5)?,
                    conversation_id: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    score: row.get(9)?,
                })
            },
        ).map_err(|e| format!("Action item not found: {}", e))?;
        
        Ok(item)
    }

    async fn list_action_items(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<ActionItemOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, content, completed, due_date, priority, conversation_id, created_at, updated_at, score
            FROM action_items WHERE uid = ?1
            ORDER BY created_at DESC
            LIMIT ?2 OFFSET ?3
            "#
        )?;
        
        let items = stmt.query_map(params![uid, pagination.limit, pagination.offset], |row| {
            Ok(ActionItemOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                content: row.get(2)?,
                completed: row.get(3)?,
                due_date: row.get(4)?,
                priority: row.get(5)?,
                conversation_id: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                score: row.get(9)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(items)
    }

    async fn update_action_item(&self, uid: &str, action_item_id: &str, update: ActionItemUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE action_items SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref content) = update.content {
            sql.push_str(&format!(", content = ?{}", param_idx));
            params_vec.push(Box::new(content.clone()));
            param_idx += 1;
        }
        
        if let Some(completed) = update.completed {
            sql.push_str(&format!(", completed = ?{}", param_idx));
            params_vec.push(Box::new(completed));
            param_idx += 1;
        }
        
        if let Some(ref due_date) = update.due_date {
            sql.push_str(&format!(", due_date = ?{}", param_idx));
            params_vec.push(Box::new(due_date));
            param_idx += 1;
        }
        
        if let Some(priority) = update.priority {
            sql.push_str(&format!(", priority = ?{}", param_idx));
            params_vec.push(Box::new(priority));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(action_item_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_action_item(&self, uid: &str, action_item_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM action_items WHERE id = ?1 AND uid = ?2",
            params![action_item_id, uid],
        )?;
        Ok(())
    }

    async fn batch_update_scores(&self, uid: &str, updates: Vec<ScoreUpdate>) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        for update in updates {
            conn.execute(
                "UPDATE action_items SET score = ?1 WHERE id = ?2 AND uid = ?3",
                params![update.score, update.id, uid],
            )?;
        }
        
        Ok(())
    }
}

// ============================================================================
// Focus Session Repository Implementation
// ============================================================================

#[async_trait]
impl FocusSessionRepository for SqliteStorage {
    async fn create_focus_session(&self, uid: &str, session: FocusSessionInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let distraction_json = serde_json::to_string(&session.distraction_events)?;
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO focus_sessions (id, uid, start_time, end_time, focus_type, 
                distraction_events, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id,
                uid,
                session.start_time,
                session.end_time,
                session.focus_type,
                distraction_json,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_focus_session(&self, uid: &str, session_id: &str) -> RepoResult<FocusSessionOutput> {
        let conn = self.conn.lock().await;
        let session = conn.query_row(
            r#"
            SELECT id, uid, start_time, end_time, focus_type, distraction_events, created_at
            FROM focus_sessions WHERE id = ?1 AND uid = ?2
            "#,
            params![session_id, uid],
            |row| {
                let distraction_json: String = row.get(5)?;
                let distraction_events = serde_json::from_str(&distraction_json).unwrap_or_default();
                
                Ok(FocusSessionOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    start_time: row.get(2)?,
                    end_time: row.get(3)?,
                    focus_type: row.get(4)?,
                    distraction_events,
                    created_at: row.get(6)?,
                })
            },
        ).map_err(|e| format!("Focus session not found: {}", e))?;
        
        Ok(session)
    }

    async fn list_focus_sessions(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<FocusSessionOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, start_time, end_time, focus_type, distraction_events, created_at
            FROM focus_sessions WHERE uid = ?1
            ORDER BY start_time DESC
            LIMIT ?2 OFFSET ?3
            "#
        )?;
        
        let sessions = stmt.query_map(params![uid, pagination.limit, pagination.offset], |row| {
            let distraction_json: String = row.get(5)?;
            let distraction_events = serde_json::from_str(&distraction_json).unwrap_or_default();
            
            Ok(FocusSessionOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                start_time: row.get(2)?,
                end_time: row.get(3)?,
                focus_type: row.get(4)?,
                distraction_events,
                created_at: row.get(6)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(sessions)
    }

    async fn update_focus_session(&self, uid: &str, session_id: &str, update: FocusSessionUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        let mut sql = String::from("UPDATE focus_sessions SET 1 = 1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        let mut param_idx = 1;
        
        sql = String::from("UPDATE focus_sessions SET ");
        let mut updates = String::new();
        
        if let Some(ref end_time) = update.end_time {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("end_time = ?{}", param_idx));
            params_vec.push(Box::new(end_time));
            param_idx += 1;
        }
        
        if let Some(ref focus_type) = update.focus_type {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("focus_type = ?{}", param_idx));
            params_vec.push(Box::new(focus_type.clone()));
            param_idx += 1;
        }
        
        if let Some(ref distraction_events) = update.distraction_events {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("distraction_events = ?{}", param_idx));
            params_vec.push(Box::new(serde_json::to_string(distraction_events)?));
            param_idx += 1;
        }
        
        if updates.is_empty() {
            return Ok(());
        }
        
        sql.push_str(&updates);
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(session_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_focus_session(&self, uid: &str, session_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM focus_sessions WHERE id = ?1 AND uid = ?2",
            params![session_id, uid],
        )?;
        Ok(())
    }

    async fn get_focus_stats(&self, uid: &str) -> RepoResult<FocusStatsOutput> {
        let conn = self.conn.lock().await;
        
        let stats = conn.query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(CAST((julianday(end_time) - julianday(start_time)) * 24 * 60 AS INTEGER)), 0)
            FROM focus_sessions WHERE uid = ?1 AND end_time IS NOT NULL
            "#,
            params![uid],
            |row| {
                let total_sessions: i32 = row.get(0)?;
                let total_minutes: i32 = row.get(1)?;
                let average = if total_sessions > 0 {
                    total_minutes as f64 / total_sessions as f64
                } else {
                    0.0
                };
                
                Ok(FocusStatsOutput {
                    total_sessions,
                    total_minutes,
                    average_session_length: average,
                })
            },
        ).map_err(|e| format!("Failed to get focus stats: {}", e))?;
        
        Ok(stats)
    }
}

// ============================================================================
// Goal Repository Implementation
// ============================================================================

#[async_trait]
impl GoalRepository for SqliteStorage {
    async fn create_goal(&self, uid: &str, goal: GoalInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO goals (id, uid, title, goal_type, target_date, progress, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                id,
                uid,
                goal.title,
                goal.goal_type,
                goal.target_date,
                goal.progress,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_goal(&self, uid: &str, goal_id: &str) -> RepoResult<GoalOutput> {
        let conn = self.conn.lock().await;
        let goal = conn.query_row(
            r#"
            SELECT id, uid, title, goal_type, target_date, progress, created_at, updated_at
            FROM goals WHERE id = ?1 AND uid = ?2
            "#,
            params![goal_id, uid],
            |row| {
                Ok(GoalOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    goal_type: row.get(3)?,
                    target_date: row.get(4)?,
                    progress: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        ).map_err(|e| format!("Goal not found: {}", e))?;
        
        Ok(goal)
    }

    async fn list_goals(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<GoalOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, title, goal_type, target_date, progress, created_at, updated_at
            FROM goals WHERE uid = ?1
            ORDER BY created_at DESC
            LIMIT ?2 OFFSET ?3
            "#
        )?;
        
        let goals = stmt.query_map(params![uid, pagination.limit, pagination.offset], |row| {
            Ok(GoalOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                title: row.get(2)?,
                goal_type: row.get(3)?,
                target_date: row.get(4)?,
                progress: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(goals)
    }

    async fn update_goal(&self, uid: &str, goal_id: &str, update: GoalUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE goals SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref title) = update.title {
            sql.push_str(&format!(", title = ?{}", param_idx));
            params_vec.push(Box::new(title.clone()));
            param_idx += 1;
        }
        
        if let Some(ref goal_type) = update.goal_type {
            sql.push_str(&format!(", goal_type = ?{}", param_idx));
            params_vec.push(Box::new(goal_type.clone()));
            param_idx += 1;
        }
        
        if let Some(ref target_date) = update.target_date {
            sql.push_str(&format!(", target_date = ?{}", param_idx));
            params_vec.push(Box::new(target_date));
            param_idx += 1;
        }
        
        if let Some(progress) = update.progress {
            sql.push_str(&format!(", progress = ?{}", param_idx));
            params_vec.push(Box::new(progress));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(goal_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_goal(&self, uid: &str, goal_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM goals WHERE id = ?1 AND uid = ?2",
            params![goal_id, uid],
        )?;
        Ok(())
    }

    async fn update_progress(&self, uid: &str, goal_id: &str, progress: f64) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        conn.execute(
            "UPDATE goals SET progress = ?1, updated_at = ?2 WHERE id = ?3 AND uid = ?4",
            params![progress, now, goal_id, uid],
        )?;
        Ok(())
    }

    async fn get_goal_history(&self, uid: &str, goal_id: &str) -> RepoResult<Vec<GoalHistoryEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT date, progress FROM goal_history WHERE uid = ?1 AND goal_id = ?2
            ORDER BY date DESC
            "#
        )?;
        
        let history = stmt.query_map(params![uid, goal_id], |row| {
            Ok(GoalHistoryEntry {
                date: row.get(0)?,
                progress: row.get(1)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(history)
    }
}

// ============================================================================
// User Settings Repository Implementation
// ============================================================================

#[async_trait]
impl UserSettingsRepository for SqliteStorage {
    async fn get_settings(&self, uid: &str) -> RepoResult<UserSettingsOutput> {
        let conn = self.conn.lock().await;
        let settings = conn.query_row(
            r#"
            SELECT uid, daily_summary_enabled, notification_settings, transcription_preferences, ai_user_profile
            FROM user_settings WHERE uid = ?1
            "#,
            params![uid],
            |row| {
                let notification_json: String = row.get(2)?;
                let transcription_json: String = row.get(3)?;
                let ai_profile_json: Option<String> = row.get(4)?;
                
                Ok(UserSettingsOutput {
                    uid: row.get(0)?,
                    daily_summary_enabled: row.get(1)?,
                    notification_settings: serde_json::from_str(&notification_json).unwrap_or(NotificationSettingsOutput {
                        push_enabled: true,
                        email_enabled: false,
                        quiet_hours_start: None,
                        quiet_hours_end: None,
                    }),
                    transcription_preferences: serde_json::from_str(&transcription_json).unwrap_or(TranscriptionPreferencesOutput {
                        auto_transcribe: true,
                        language: None,
                    }),
                    ai_user_profile: ai_profile_json.and_then(|j| serde_json::from_str(&j).ok()),
                })
            },
        ).map_err(|e| format!("User settings not found: {}", e))?;
        
        Ok(settings)
    }

    async fn update_settings(&self, uid: &str, update: UserSettingsUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        let mut sql = String::from("UPDATE user_settings SET uid = uid"); // Dummy to ensure row exists
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        let mut param_idx = 1;
        
        sql = String::from("UPDATE user_settings SET ");
        let mut updates = String::new();
        
        if let Some(daily_summary) = update.daily_summary_enabled {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("daily_summary_enabled = ?{}", param_idx));
            params_vec.push(Box::new(daily_summary));
            param_idx += 1;
        }
        
        if let Some(ref notification_settings) = update.notification_settings {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("notification_settings = ?{}", param_idx));
            params_vec.push(Box::new(serde_json::to_string(notification_settings)?));
            param_idx += 1;
        }
        
        if let Some(ref transcription_prefs) = update.transcription_preferences {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("transcription_preferences = ?{}", param_idx));
            params_vec.push(Box::new(serde_json::to_string(transcription_prefs)?));
            param_idx += 1;
        }
        
        if let Some(ref ai_profile) = update.ai_user_profile {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("ai_user_profile = ?{}", param_idx));
            params_vec.push(Box::new(serde_json::to_string(ai_profile)?));
            param_idx += 1;
        }
        
        if updates.is_empty() {
            return Ok(());
        }
        
        sql.push_str(&updates);
        sql.push_str(&format!(" WHERE uid = ?{}", param_idx));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }
}

// ============================================================================
// Folder Repository Implementation
// ============================================================================

#[async_trait]
impl FolderRepository for SqliteStorage {
    async fn create_folder(&self, uid: &str, folder: FolderInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        
        // Get max sort_order
        let max_order: Option<i32> = conn.query_row(
            "SELECT MAX(sort_order) FROM folders WHERE uid = ?1",
            params![uid],
            |row| row.get(0),
        )?;
        let sort_order = max_order.unwrap_or(0) + 1;
        
        conn.execute(
            r#"
            INSERT INTO folders (id, uid, name, icon, color, sort_order, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                id,
                uid,
                folder.name,
                folder.icon,
                folder.color,
                sort_order,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_folder(&self, uid: &str, folder_id: &str) -> RepoResult<FolderOutput> {
        let conn = self.conn.lock().await;
        let folder = conn.query_row(
            r#"
            SELECT id, uid, name, icon, color, sort_order, created_at, updated_at
            FROM folders WHERE id = ?1 AND uid = ?2
            "#,
            params![folder_id, uid],
            |row| {
                Ok(FolderOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    name: row.get(2)?,
                    icon: row.get(3)?,
                    color: row.get(4)?,
                    sort_order: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        ).map_err(|e| format!("Folder not found: {}", e))?;
        
        Ok(folder)
    }

    async fn list_folders(&self, uid: &str) -> RepoResult<Vec<FolderOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, name, icon, color, sort_order, created_at, updated_at
            FROM folders WHERE uid = ?1
            ORDER BY sort_order ASC
            "#
        )?;
        
        let folders = stmt.query_map(params![uid], |row| {
            Ok(FolderOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                name: row.get(2)?,
                icon: row.get(3)?,
                color: row.get(4)?,
                sort_order: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(folders)
    }

    async fn update_folder(&self, uid: &str, folder_id: &str, update: FolderUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE folders SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref name) = update.name {
            sql.push_str(&format!(", name = ?{}", param_idx));
            params_vec.push(Box::new(name.clone()));
            param_idx += 1;
        }
        
        if let Some(ref icon) = update.icon {
            sql.push_str(&format!(", icon = ?{}", param_idx));
            params_vec.push(Box::new(icon.clone()));
            param_idx += 1;
        }
        
        if let Some(ref color) = update.color {
            sql.push_str(&format!(", color = ?{}", param_idx));
            params_vec.push(Box::new(color.clone()));
            param_idx += 1;
        }
        
        if let Some(sort_order) = update.sort_order {
            sql.push_str(&format!(", sort_order = ?{}", param_idx));
            params_vec.push(Box::new(sort_order));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(folder_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_folder(&self, uid: &str, folder_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM folders WHERE id = ?1 AND uid = ?2",
            params![folder_id, uid],
        )?;
        Ok(())
    }

    async fn reorder_folders(&self, uid: &str, folder_ids: Vec<String>) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        for (idx, folder_id) in folder_ids.iter().enumerate() {
            conn.execute(
                "UPDATE folders SET sort_order = ?1 WHERE id = ?2 AND uid = ?3",
                params![idx as i32, folder_id, uid],
            )?;
        }
        
        Ok(())
    }
}

// ============================================================================
// Chat Session Repository Implementation
// ============================================================================

#[async_trait]
impl ChatSessionRepository for SqliteStorage {
    async fn create_chat_session(&self, uid: &str, session: ChatSessionInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO chat_sessions (id, uid, title, model_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                id,
                uid,
                session.title,
                session.model_id,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_chat_session(&self, uid: &str, session_id: &str) -> RepoResult<ChatSessionOutput> {
        let conn = self.conn.lock().await;
        let session = conn.query_row(
            r#"
            SELECT id, uid, title, model_id, created_at, updated_at
            FROM chat_sessions WHERE id = ?1 AND uid = ?2
            "#,
            params![session_id, uid],
            |row| {
                Ok(ChatSessionOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    model_id: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        ).map_err(|e| format!("Chat session not found: {}", e))?;
        
        Ok(session)
    }

    async fn list_chat_sessions(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<ChatSessionOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, title, model_id, created_at, updated_at
            FROM chat_sessions WHERE uid = ?1
            ORDER BY updated_at DESC
            LIMIT ?2 OFFSET ?3
            "#
        )?;
        
        let sessions = stmt.query_map(params![uid, pagination.limit, pagination.offset], |row| {
            Ok(ChatSessionOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                title: row.get(2)?,
                model_id: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(sessions)
    }

    async fn update_chat_session(&self, uid: &str, session_id: &str, update: ChatSessionUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE chat_sessions SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref title) = update.title {
            sql.push_str(&format!(", title = ?{}", param_idx));
            params_vec.push(Box::new(title.clone()));
            param_idx += 1;
        }
        
        if let Some(ref model_id) = update.model_id {
            sql.push_str(&format!(", model_id = ?{}", param_idx));
            params_vec.push(Box::new(model_id.clone()));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(session_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_chat_session(&self, uid: &str, session_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM chat_sessions WHERE id = ?1 AND uid = ?2",
            params![session_id, uid],
        )?;
        Ok(())
    }
}

// ============================================================================
// Persona Repository Implementation
// ============================================================================

#[async_trait]
impl PersonaRepository for SqliteStorage {
    async fn create_persona(&self, uid: &str, persona: PersonaInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO personas (id, uid, name, description, prompt, avatar_url, 
                personality_type, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                id,
                uid,
                persona.name,
                persona.description,
                persona.prompt,
                persona.avatar_url,
                persona.personality_type,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_persona(&self, uid: &str, persona_id: &str) -> RepoResult<PersonaOutput> {
        let conn = self.conn.lock().await;
        let persona = conn.query_row(
            r#"
            SELECT id, uid, name, description, prompt, avatar_url, personality_type, created_at, updated_at
            FROM personas WHERE id = ?1 AND uid = ?2
            "#,
            params![persona_id, uid],
            |row| {
                Ok(PersonaOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    prompt: row.get(4)?,
                    avatar_url: row.get(5)?,
                    personality_type: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        ).map_err(|e| format!("Persona not found: {}", e))?;
        
        Ok(persona)
    }

    async fn list_personas(&self, uid: &str) -> RepoResult<Vec<PersonaOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, name, description, prompt, avatar_url, personality_type, created_at, updated_at
            FROM personas WHERE uid = ?1
            ORDER BY created_at DESC
            "#
        )?;
        
        let personas = stmt.query_map(params![uid], |row| {
            Ok(PersonaOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                prompt: row.get(4)?,
                avatar_url: row.get(5)?,
                personality_type: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(personas)
    }

    async fn update_persona(&self, uid: &str, persona_id: &str, update: PersonaUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE personas SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref name) = update.name {
            sql.push_str(&format!(", name = ?{}", param_idx));
            params_vec.push(Box::new(name.clone()));
            param_idx += 1;
        }
        
        if let Some(ref description) = update.description {
            sql.push_str(&format!(", description = ?{}", param_idx));
            params_vec.push(Box::new(description.clone()));
            param_idx += 1;
        }
        
        if let Some(ref prompt) = update.prompt {
            sql.push_str(&format!(", prompt = ?{}", param_idx));
            params_vec.push(Box::new(prompt.clone()));
            param_idx += 1;
        }
        
        if let Some(ref avatar_url) = update.avatar_url {
            sql.push_str(&format!(", avatar_url = ?{}", param_idx));
            params_vec.push(Box::new(avatar_url.clone()));
            param_idx += 1;
        }
        
        if let Some(ref personality_type) = update.personality_type {
            sql.push_str(&format!(", personality_type = ?{}", param_idx));
            params_vec.push(Box::new(personality_type.clone()));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(persona_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_persona(&self, uid: &str, persona_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM personas WHERE id = ?1 AND uid = ?2",
            params![persona_id, uid],
        )?;
        Ok(())
    }
}

// ============================================================================
// Advice Repository Implementation
// ============================================================================

#[async_trait]
impl AdviceRepository for SqliteStorage {
    async fn create_advice(&self, uid: &str, advice: AdviceInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO advice (id, uid, title, content, category, reasoning, source, 
                is_enabled, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                id,
                uid,
                advice.title,
                advice.content,
                advice.category,
                advice.reasoning,
                advice.source,
                true,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_advice(&self, uid: &str, advice_id: &str) -> RepoResult<AdviceOutput> {
        let conn = self.conn.lock().await;
        let advice = conn.query_row(
            r#"
            SELECT id, uid, title, content, category, reasoning, source, is_enabled, created_at, updated_at
            FROM advice WHERE id = ?1 AND uid = ?2
            "#,
            params![advice_id, uid],
            |row| {
                Ok(AdviceOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    category: row.get(4)?,
                    reasoning: row.get(5)?,
                    source: row.get(6)?,
                    is_enabled: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        ).map_err(|e| format!("Advice not found: {}", e))?;
        
        Ok(advice)
    }

    async fn list_advice(&self, uid: &str, category: Option<String>) -> RepoResult<Vec<AdviceOutput>> {
        let conn = self.conn.lock().await;
        
        let sql = if category.is_some() {
            r#"
            SELECT id, uid, title, content, category, reasoning, source, is_enabled, created_at, updated_at
            FROM advice WHERE uid = ?1 AND category = ?2 AND is_enabled = 1
            ORDER BY created_at DESC
            "#
        } else {
            r#"
            SELECT id, uid, title, content, category, reasoning, source, is_enabled, created_at, updated_at
            FROM advice WHERE uid = ?1 AND is_enabled = 1
            ORDER BY created_at DESC
            "#
        };
        
        let mut stmt = conn.prepare(sql)?;
        
        let advices = if let Some(ref cat) = category {
            stmt.query_map(params![uid, cat], |row| {
                Ok(AdviceOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    category: row.get(4)?,
                    reasoning: row.get(5)?,
                    source: row.get(6)?,
                    is_enabled: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![uid], |row| {
                Ok(AdviceOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    category: row.get(4)?,
                    reasoning: row.get(5)?,
                    source: row.get(6)?,
                    is_enabled: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?
        };
        
        Ok(advices)
    }

    async fn update_advice(&self, uid: &str, advice_id: &str, update: AdviceUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE advice SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref title) = update.title {
            sql.push_str(&format!(", title = ?{}", param_idx));
            params_vec.push(Box::new(title.clone()));
            param_idx += 1;
        }
        
        if let Some(ref content) = update.content {
            sql.push_str(&format!(", content = ?{}", param_idx));
            params_vec.push(Box::new(content.clone()));
            param_idx += 1;
        }
        
        if let Some(ref category) = update.category {
            sql.push_str(&format!(", category = ?{}", param_idx));
            params_vec.push(Box::new(category.clone()));
            param_idx += 1;
        }
        
        if let Some(is_enabled) = update.is_enabled {
            sql.push_str(&format!(", is_enabled = ?{}", param_idx));
            params_vec.push(Box::new(is_enabled));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(advice_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_advice(&self, uid: &str, advice_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM advice WHERE id = ?1 AND uid = ?2",
            params![advice_id, uid],
        )?;
        Ok(())
    }
}

// ============================================================================
// Knowledge Graph Repository Implementation
// ============================================================================

#[async_trait]
impl KnowledgeGraphRepository for SqliteStorage {
    async fn create_node(&self, uid: &str, node: KgNodeInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO kg_nodes (id, uid, entity_type, entity_id, content, confidence, 
                source, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                id,
                uid,
                node.entity_type,
                node.entity_id,
                node.content,
                node.confidence,
                node.source,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_node(&self, uid: &str, node_id: &str) -> RepoResult<KgNodeOutput> {
        let conn = self.conn.lock().await;
        let node = conn.query_row(
            r#"
            SELECT id, uid, entity_type, entity_id, content, confidence, source, created_at
            FROM kg_nodes WHERE id = ?1 AND uid = ?2
            "#,
            params![node_id, uid],
            |row| {
                Ok(KgNodeOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    entity_type: row.get(2)?,
                    entity_id: row.get(3)?,
                    content: row.get(4)?,
                    confidence: row.get(5)?,
                    source: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        ).map_err(|e| format!("Node not found: {}", e))?;
        
        Ok(node)
    }

    async fn list_nodes(&self, uid: &str, node_type: Option<String>) -> RepoResult<Vec<KgNodeOutput>> {
        let conn = self.conn.lock().await;
        
        let sql = if node_type.is_some() {
            r#"
            SELECT id, uid, entity_type, entity_id, content, confidence, source, created_at
            FROM kg_nodes WHERE uid = ?1 AND entity_type = ?2
            ORDER BY created_at DESC
            "#
        } else {
            r#"
            SELECT id, uid, entity_type, entity_id, content, confidence, source, created_at
            FROM kg_nodes WHERE uid = ?1
            ORDER BY created_at DESC
            "#
        };
        
        let mut stmt = conn.prepare(sql)?;
        
        let nodes = if let Some(ref nt) = node_type {
            stmt.query_map(params![uid, nt], |row| {
                Ok(KgNodeOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    entity_type: row.get(2)?,
                    entity_id: row.get(3)?,
                    content: row.get(4)?,
                    confidence: row.get(5)?,
                    source: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![uid], |row| {
                Ok(KgNodeOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    entity_type: row.get(2)?,
                    entity_id: row.get(3)?,
                    content: row.get(4)?,
                    confidence: row.get(5)?,
                    source: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?
        };
        
        Ok(nodes)
    }

    async fn update_node(&self, uid: &str, node_id: &str, update: KgNodeUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        let mut sql = String::from("UPDATE kg_nodes SET 1 = 1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        let mut param_idx = 1;
        
        sql = String::from("UPDATE kg_nodes SET ");
        let mut updates = String::new();
        
        if let Some(ref content) = update.content {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("content = ?{}", param_idx));
            params_vec.push(Box::new(content.clone()));
            param_idx += 1;
        }
        
        if let Some(confidence) = update.confidence {
            if !updates.is_empty() { updates.push_str(", "); }
            updates.push_str(&format!("confidence = ?{}", param_idx));
            params_vec.push(Box::new(confidence));
            param_idx += 1;
        }
        
        if updates.is_empty() {
            return Ok(());
        }
        
        sql.push_str(&updates);
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(node_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_node(&self, uid: &str, node_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        // Delete edges first
        conn.execute(
            "DELETE FROM kg_edges WHERE source_node_id = ?1 OR target_node_id = ?1",
            params![node_id],
        )?;
        conn.execute(
            "DELETE FROM kg_nodes WHERE id = ?1 AND uid = ?2",
            params![node_id, uid],
        )?;
        Ok(())
    }

    async fn create_edge(&self, uid: &str, edge: KgEdgeInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO kg_edges (id, uid, source_node_id, target_node_id, relationship, 
                confidence, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id,
                uid,
                edge.source_node_id,
                edge.target_node_id,
                edge.relationship,
                edge.confidence,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn list_edges(&self, uid: &str, node_id: Option<String>) -> RepoResult<Vec<KgEdgeOutput>> {
        let conn = self.conn.lock().await;
        
        let sql = if node_id.is_some() {
            r#"
            SELECT id, uid, source_node_id, target_node_id, relationship, confidence, created_at
            FROM kg_edges WHERE uid = ?1 AND (source_node_id = ?2 OR target_node_id = ?2)
            ORDER BY created_at DESC
            "#
        } else {
            r#"
            SELECT id, uid, source_node_id, target_node_id, relationship, confidence, created_at
            FROM kg_edges WHERE uid = ?1
            ORDER BY created_at DESC
            "#
        };
        
        let mut stmt = conn.prepare(sql)?;
        
        let edges = if let Some(ref nid) = node_id {
            stmt.query_map(params![uid, nid], |row| {
                Ok(KgEdgeOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    source_node_id: row.get(2)?,
                    target_node_id: row.get(3)?,
                    relationship: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![uid], |row| {
                Ok(KgEdgeOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    source_node_id: row.get(2)?,
                    target_node_id: row.get(3)?,
                    relationship: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?
        };
        
        Ok(edges)
    }

    async fn delete_edge(&self, uid: &str, edge_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM kg_edges WHERE id = ?1 AND uid = ?2",
            params![edge_id, uid],
        )?;
        Ok(())
    }

    async fn rebuild_graph(&self, uid: &str) -> RepoResult<RebuildGraphOutput> {
        // This is a placeholder - actual implementation would involve
        // extracting knowledge from conversations and rebuilding the graph
        Ok(RebuildGraphOutput {
            nodes_created: 0,
            edges_created: 0,
        })
    }
}

// ============================================================================
// Person Repository Implementation
// ============================================================================

#[async_trait]
impl PersonRepository for SqliteStorage {
    async fn create_person(&self, uid: &str, person: PersonInput) -> RepoResult<String> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO people (id, uid, name, email, phone, avatar_url, notes, 
                created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                id,
                uid,
                person.name,
                person.email,
                person.phone,
                person.avatar_url,
                person.notes,
                now,
                now,
            ],
        )?;
        
        Ok(id)
    }

    async fn get_person(&self, uid: &str, person_id: &str) -> RepoResult<PersonOutput> {
        let conn = self.conn.lock().await;
        let person = conn.query_row(
            r#"
            SELECT id, uid, name, email, phone, avatar_url, notes, created_at, updated_at
            FROM people WHERE id = ?1 AND uid = ?2
            "#,
            params![person_id, uid],
            |row| {
                Ok(PersonOutput {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    name: row.get(2)?,
                    email: row.get(3)?,
                    phone: row.get(4)?,
                    avatar_url: row.get(5)?,
                    notes: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        ).map_err(|e| format!("Person not found: {}", e))?;
        
        Ok(person)
    }

    async fn list_people(&self, uid: &str) -> RepoResult<Vec<PersonOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, name, email, phone, avatar_url, notes, created_at, updated_at
            FROM people WHERE uid = ?1
            ORDER BY name ASC
            "#
        )?;
        
        let people = stmt.query_map(params![uid], |row| {
            Ok(PersonOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                name: row.get(2)?,
                email: row.get(3)?,
                phone: row.get(4)?,
                avatar_url: row.get(5)?,
                notes: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(people)
    }

    async fn update_person(&self, uid: &str, person_id: &str, update: PersonUpdate) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now();
        
        let mut sql = String::from("UPDATE people SET updated_at = ?1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut param_idx = 2;
        
        if let Some(ref name) = update.name {
            sql.push_str(&format!(", name = ?{}", param_idx));
            params_vec.push(Box::new(name.clone()));
            param_idx += 1;
        }
        
        if let Some(ref email) = update.email {
            sql.push_str(&format!(", email = ?{}", param_idx));
            params_vec.push(Box::new(email.clone()));
            param_idx += 1;
        }
        
        if let Some(ref phone) = update.phone {
            sql.push_str(&format!(", phone = ?{}", param_idx));
            params_vec.push(Box::new(phone.clone()));
            param_idx += 1;
        }
        
        if let Some(ref avatar_url) = update.avatar_url {
            sql.push_str(&format!(", avatar_url = ?{}", param_idx));
            params_vec.push(Box::new(avatar_url.clone()));
            param_idx += 1;
        }
        
        if let Some(ref notes) = update.notes {
            sql.push_str(&format!(", notes = ?{}", param_idx));
            params_vec.push(Box::new(notes.clone()));
            param_idx += 1;
        }
        
        sql.push_str(&format!(" WHERE id = ?{} AND uid = ?{}", param_idx, param_idx + 1));
        params_vec.push(Box::new(person_id.to_string()));
        params_vec.push(Box::new(uid.to_string()));
        
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
        
        Ok(())
    }

    async fn delete_person(&self, uid: &str, person_id: &str) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM people WHERE id = ?1 AND uid = ?2",
            params![person_id, uid],
        )?;
        Ok(())
    }

    async fn assign_conversation_segments(&self, uid: &str, person_id: &str, segments: Vec<SegmentAssignment>) -> RepoResult<()> {
        let conn = self.conn.lock().await;
        
        for segment in segments {
            conn.execute(
                r#"
                UPDATE messages SET person_id = ?1 
                WHERE uid = ?2 AND conversation_id = ?3 AND rowid = ?4
                "#,
                params![person_id, uid, segment.conversation_id, segment.segment_index],
            )?;
        }
        
        Ok(())
    }
}

// ============================================================================
// LLM Usage Repository Implementation
// ============================================================================

#[async_trait]
impl LlmUsageRepository for SqliteStorage {
    async fn record_usage(&self, uid: &str, usage: LlmUsageInput) -> RepoResult<()> {
        let id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO llm_usage (id, uid, model_id, input_tokens, output_tokens, 
                total_tokens, cost_usd, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                id,
                uid,
                usage.model_id,
                usage.input_tokens,
                usage.output_tokens,
                usage.total_tokens,
                usage.cost_usd,
                now,
            ],
        )?;
        
        Ok(())
    }

    async fn get_usage_stats(&self, uid: &str, start_date: DateTime<Utc>, end_date: DateTime<Utc>) -> RepoResult<LlmUsageStats> {
        let conn = self.conn.lock().await;
        
        let stats = conn.query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0), COALESCE(SUM(cost_usd), 0)
            FROM llm_usage WHERE uid = ?1 AND created_at >= ?2 AND created_at <= ?3
            "#,
            params![uid, start_date, end_date],
            |row| {
                Ok(LlmUsageStats {
                    total_requests: row.get(0)?,
                    total_input_tokens: row.get(1)?,
                    total_output_tokens: row.get(2)?,
                    total_tokens: row.get(3)?,
                    total_cost_usd: row.get(4)?,
                })
            },
        ).map_err(|e| format!("Failed to get LLM usage stats: {}", e))?;
        
        Ok(stats)
    }
}

// ============================================================================
// Screen Activity Repository Implementation
// ============================================================================

#[async_trait]
impl ScreenActivityRepository for SqliteStorage {
    async fn record_activity(&self, uid: &str, activity: ScreenActivityInput) -> RepoResult<()> {
        let id = ulid::Ulid::new().to_string();
        
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO screen_activity (id, uid, window_title, app_name, duration_seconds, timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                id,
                uid,
                activity.window_title,
                activity.app_name,
                activity.duration_seconds,
                activity.timestamp,
            ],
        )?;
        
        Ok(())
    }

    async fn get_recent_activity(&self, uid: &str, limit: usize) -> RepoResult<Vec<ScreenActivityOutput>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, uid, window_title, app_name, duration_seconds, timestamp
            FROM screen_activity WHERE uid = ?1
            ORDER BY timestamp DESC
            LIMIT ?2
            "#
        )?;
        
        let activities = stmt.query_map(params![uid, limit], |row| {
            Ok(ScreenActivityOutput {
                id: row.get(0)?,
                uid: row.get(1)?,
                window_title: row.get(2)?,
                app_name: row.get(3)?,
                duration_seconds: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(activities)
    }

    async fn delete_old_activity(&self, uid: &str, before: DateTime<Utc>) -> RepoResult<i32> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute(
            "DELETE FROM screen_activity WHERE uid = ?1 AND timestamp < ?2",
            params![uid, before],
        )?;
        Ok(deleted as i32)
    }
}
