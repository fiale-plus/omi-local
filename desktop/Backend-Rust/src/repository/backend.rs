// Storage backend selection and configuration
// Provides runtime backend selection based on configuration

use std::sync::Arc;
use std::path::PathBuf;
use async_trait::async_trait;

use super::traits::*;
use super::sqlite::SqliteStorage;

/// Storage backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    /// SQLite local storage
    Sqlite,
    /// Firestore (future)
    Firestore,
    /// Hybrid mode - SQLite for local, Firestore for sync
    Hybrid,
}

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Backend type to use
    pub backend: StorageBackend,
    /// SQLite database path (if using Sqlite)
    pub sqlite_path: Option<PathBuf>,
    /// Enable offline-first mode
    pub offline_first: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Sqlite,
            sqlite_path: None,
            offline_first: true,
        }
    }
}

/// Unified repository container
/// Provides access to all repository traits through a single interface
pub struct Repository {
    pub memories: Arc<dyn MemoryRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub messages: Arc<dyn MessageRepository>,
    pub action_items: Arc<dyn ActionItemRepository>,
    pub focus_sessions: Arc<dyn FocusSessionRepository>,
    pub goals: Arc<dyn GoalRepository>,
    pub user_settings: Arc<dyn UserSettingsRepository>,
    pub folders: Arc<dyn FolderRepository>,
    pub chat_sessions: Arc<dyn ChatSessionRepository>,
    pub personas: Arc<dyn PersonaRepository>,
    pub advice: Arc<dyn AdviceRepository>,
    pub knowledge_graph: Arc<dyn KnowledgeGraphRepository>,
    pub people: Arc<dyn PersonRepository>,
    pub llm_usage: Arc<dyn LlmUsageRepository>,
    pub screen_activity: Arc<dyn ScreenActivityRepository>,
}

impl Repository {
    /// Create a new repository with SQLite backend
    pub async fn new_sqlite(db_path: PathBuf) -> RepoResult<Self> {
        let sqlite = SqliteStorage::new(db_path).await?;

        Ok(Self {
            memories: Arc::new(sqlite.clone()),
            conversations: Arc::new(sqlite.clone()),
            messages: Arc::new(sqlite.clone()),
            action_items: Arc::new(sqlite.clone()),
            focus_sessions: Arc::new(sqlite.clone()),
            goals: Arc::new(sqlite.clone()),
            user_settings: Arc::new(sqlite.clone()),
            folders: Arc::new(sqlite.clone()),
            chat_sessions: Arc::new(sqlite.clone()),
            personas: Arc::new(sqlite.clone()),
            advice: Arc::new(sqlite.clone()),
            knowledge_graph: Arc::new(sqlite.clone()),
            people: Arc::new(sqlite.clone()),
            llm_usage: Arc::new(sqlite.clone()),
            screen_activity: Arc::new(sqlite.clone()),
        })
    }

    /// Create a new repository with default SQLite path
    pub async fn new_default_sqlite() -> RepoResult<Self> {
        let db_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("omi-desktop")
            .join("data")
            .join("omi.db");
        
        Self::new_sqlite(db_path).await
    }

    /// Get the configured storage backend
    pub fn backend_type(&self) -> StorageBackend {
        StorageBackend::Sqlite
    }
}

// Implement Clone for SqliteStorage to allow Arc::new(sqlite.clone())
impl Clone for SqliteStorage {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            db_path: self.db_path.clone(),
        }
    }
}

/// Storage backend trait for dynamic switching
#[async_trait]
pub trait StorageBackendTrait: Send + Sync {
    /// Get the backend type
    fn backend_type(&self) -> StorageBackend;
    
    /// Check if the backend is available (e.g., network connectivity)
    async fn is_available(&self) -> bool;
    
    /// Get the repository
    fn repository(&self) -> &Repository;
}

#[async_trait]
impl StorageBackendTrait for Repository {
    fn backend_type(&self) -> StorageBackend {
        StorageBackend::Sqlite
    }

    async fn is_available(&self) -> bool {
        // SQLite is always available
        true
    }

    fn repository(&self) -> &Repository {
        self
    }
}
