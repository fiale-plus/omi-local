// Repository traits - Unified data access interface
// Defines the contract for all storage backends (Firestore, SQLite, etc.)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;

/// Result type for repository operations
pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Pagination parameters
#[derive(Debug, Clone, Default)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}

/// Filter criteria for queries
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub category: Option<String>,
    pub tags: Option<String>,
    pub include_dismissed: bool,
}

/// Base document trait - common fields across all stored entities
pub trait Document {
    fn id(&self) -> &str;
    fn uid(&self) -> &str;
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
}

/// Memory repository operations
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    async fn create_memory(&self, uid: &str, memory: MemoryInput) -> RepoResult<String>;
    async fn get_memory(&self, uid: &str, memory_id: &str) -> RepoResult<MemoryOutput>;
    async fn list_memories(&self, uid: &str, filter: QueryFilter, pagination: Pagination) -> RepoResult<Vec<MemoryOutput>>;
    async fn update_memory(&self, uid: &str, memory_id: &str, update: MemoryUpdate) -> RepoResult<()>;
    async fn delete_memory(&self, uid: &str, memory_id: &str) -> RepoResult<()>;
    async fn review_memory(&self, uid: &str, memory_id: &str, approved: bool) -> RepoResult<()>;
}

/// Memory input for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInput {
    pub content: String,
    pub visibility: String,
    pub category: Option<String>,
    pub confidence: Option<f64>,
    pub source_app: Option<String>,
    pub context_summary: Option<String>,
    pub tags: Vec<String>,
    pub reasoning: Option<String>,
    pub current_activity: Option<String>,
    pub source: Option<String>,
    pub window_title: Option<String>,
}

/// Memory output from repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryOutput {
    pub id: String,
    pub uid: String,
    pub content: String,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub conversation_id: Option<String>,
    pub reviewed: bool,
    pub user_review: Option<bool>,
    pub visibility: String,
    pub manually_added: bool,
    pub scoring: Option<String>,
    pub confidence: Option<f64>,
    pub source_app: Option<String>,
    pub context_summary: Option<String>,
    pub is_read: bool,
    pub is_dismissed: bool,
    pub tags: Vec<String>,
    pub reasoning: Option<String>,
    pub current_activity: Option<String>,
    pub window_title: Option<String>,
}

/// Memory update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryUpdate {
    pub content: Option<String>,
    pub visibility: Option<String>,
    pub is_read: Option<bool>,
    pub is_dismissed: Option<bool>,
    pub tags: Option<Vec<String>>,
}

/// Conversation repository operations
#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn create_conversation(&self, uid: &str, conversation: ConversationInput) -> RepoResult<String>;
    async fn get_conversation(&self, uid: &str, conversation_id: &str) -> RepoResult<ConversationOutput>;
    async fn list_conversations(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<ConversationOutput>>;
    async fn update_conversation(&self, uid: &str, conversation_id: &str, update: ConversationUpdate) -> RepoResult<()>;
    async fn delete_conversation(&self, uid: &str, conversation_id: &str) -> RepoResult<()>;
}

/// Conversation input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInput {
    pub title: String,
    pub source: Option<String>,
    pub geolocation: Option<GeolocationInput>,
}

/// Geolocation input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeolocationInput {
    pub latitude: f64,
    pub longitude: f64,
    pub place_name: Option<String>,
}

/// Conversation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationOutput {
    pub id: String,
    pub uid: String,
    pub title: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source: Option<String>,
    pub geolocation: Option<GeolocationOutput>,
    pub last_message_at: Option<DateTime<Utc>>,
}

/// Geolocation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeolocationOutput {
    pub latitude: f64,
    pub longitude: f64,
    pub place_name: Option<String>,
}

/// Conversation update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationUpdate {
    pub title: Option<String>,
    pub status: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
}

/// Message repository operations
#[async_trait]
pub trait MessageRepository: Send + Sync {
    async fn create_message(&self, uid: &str, conversation_id: &str, message: MessageInput) -> RepoResult<String>;
    async fn get_message(&self, uid: &str, conversation_id: &str, message_id: &str) -> RepoResult<MessageOutput>;
    async fn list_messages(&self, uid: &str, conversation_id: &str, pagination: Pagination) -> RepoResult<Vec<MessageOutput>>;
    async fn update_message(&self, uid: &str, conversation_id: &str, message_id: &str, update: MessageUpdate) -> RepoResult<()>;
    async fn delete_message(&self, uid: &str, conversation_id: &str, message_id: &str) -> RepoResult<()>;
    async fn rate_message(&self, uid: &str, conversation_id: &str, message_id: &str, rating: i32) -> RepoResult<()>;
}

/// Message input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInput {
    pub role: String,
    pub content: String,
    pub audio_url: Option<String>,
    pub transcript: Option<String>,
}

/// Message output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageOutput {
    pub id: String,
    pub uid: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub audio_url: Option<String>,
    pub transcript: Option<String>,
    pub created_at: DateTime<Utc>,
    pub rating: Option<i32>,
}

/// Message update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageUpdate {
    pub content: Option<String>,
    pub transcript: Option<String>,
}

/// Action item repository operations
#[async_trait]
pub trait ActionItemRepository: Send + Sync {
    async fn create_action_item(&self, uid: &str, action_item: ActionItemInput) -> RepoResult<String>;
    async fn get_action_item(&self, uid: &str, action_item_id: &str) -> RepoResult<ActionItemOutput>;
    async fn list_action_items(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<ActionItemOutput>>;
    async fn update_action_item(&self, uid: &str, action_item_id: &str, update: ActionItemUpdate) -> RepoResult<()>;
    async fn delete_action_item(&self, uid: &str, action_item_id: &str) -> RepoResult<()>;
    async fn batch_update_scores(&self, uid: &str, updates: Vec<ScoreUpdate>) -> RepoResult<()>;
}

/// Action item input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItemInput {
    pub content: String,
    pub completed: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub priority: Option<i32>,
    pub conversation_id: Option<String>,
}

/// Action item output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItemOutput {
    pub id: String,
    pub uid: String,
    pub content: String,
    pub completed: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub priority: i32,
    pub conversation_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub score: Option<f64>,
}

/// Action item update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionItemUpdate {
    pub content: Option<String>,
    pub completed: Option<bool>,
    pub due_date: Option<DateTime<Utc>>,
    pub priority: Option<i32>,
}

/// Score update for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreUpdate {
    pub id: String,
    pub score: f64,
}

/// Focus session repository operations
#[async_trait]
pub trait FocusSessionRepository: Send + Sync {
    async fn create_focus_session(&self, uid: &str, session: FocusSessionInput) -> RepoResult<String>;
    async fn get_focus_session(&self, uid: &str, session_id: &str) -> RepoResult<FocusSessionOutput>;
    async fn list_focus_sessions(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<FocusSessionOutput>>;
    async fn update_focus_session(&self, uid: &str, session_id: &str, update: FocusSessionUpdate) -> RepoResult<()>;
    async fn delete_focus_session(&self, uid: &str, session_id: &str) -> RepoResult<()>;
    async fn get_focus_stats(&self, uid: &str) -> RepoResult<FocusStatsOutput>;
}

/// Focus session input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSessionInput {
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub focus_type: Option<String>,
    pub distraction_events: Vec<String>,
}

/// Focus session output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSessionOutput {
    pub id: String,
    pub uid: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub focus_type: Option<String>,
    pub distraction_events: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Focus session update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusSessionUpdate {
    pub end_time: Option<DateTime<Utc>>,
    pub focus_type: Option<String>,
    pub distraction_events: Option<Vec<String>>,
}

/// Focus stats output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusStatsOutput {
    pub total_sessions: i32,
    pub total_minutes: i32,
    pub average_session_length: f64,
}

/// Goal repository operations
#[async_trait]
pub trait GoalRepository: Send + Sync {
    async fn create_goal(&self, uid: &str, goal: GoalInput) -> RepoResult<String>;
    async fn get_goal(&self, uid: &str, goal_id: &str) -> RepoResult<GoalOutput>;
    async fn list_goals(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<GoalOutput>>;
    async fn update_goal(&self, uid: &str, goal_id: &str, update: GoalUpdate) -> RepoResult<()>;
    async fn delete_goal(&self, uid: &str, goal_id: &str) -> RepoResult<()>;
    async fn update_progress(&self, uid: &str, goal_id: &str, progress: f64) -> RepoResult<()>;
    async fn get_goal_history(&self, uid: &str, goal_id: &str) -> RepoResult<Vec<GoalHistoryEntry>>;
}

/// Goal input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalInput {
    pub title: String,
    pub goal_type: String,
    pub target_date: Option<DateTime<Utc>>,
    pub progress: f64,
}

/// Goal output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalOutput {
    pub id: String,
    pub uid: String,
    pub title: String,
    pub goal_type: String,
    pub target_date: Option<DateTime<Utc>>,
    pub progress: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Goal update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoalUpdate {
    pub title: Option<String>,
    pub goal_type: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
    pub progress: Option<f64>,
}

/// Goal history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalHistoryEntry {
    pub date: DateTime<Utc>,
    pub progress: f64,
}

/// User settings repository operations
#[async_trait]
pub trait UserSettingsRepository: Send + Sync {
    async fn get_settings(&self, uid: &str) -> RepoResult<UserSettingsOutput>;
    async fn update_settings(&self, uid: &str, update: UserSettingsUpdate) -> RepoResult<()>;
}

/// User settings output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettingsOutput {
    pub uid: String,
    pub daily_summary_enabled: bool,
    pub notification_settings: NotificationSettingsOutput,
    pub transcription_preferences: TranscriptionPreferencesOutput,
    pub ai_user_profile: Option<AiUserProfileOutput>,
}

/// Notification settings output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettingsOutput {
    pub push_enabled: bool,
    pub email_enabled: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
}

/// Transcription preferences output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionPreferencesOutput {
    pub auto_transcribe: bool,
    pub language: Option<String>,
}

/// AI user profile output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiUserProfileOutput {
    pub name: Option<String>,
    pub bio: Option<String>,
}

/// User settings update
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserSettingsUpdate {
    pub daily_summary_enabled: Option<bool>,
    pub notification_settings: Option<NotificationSettingsOutput>,
    pub transcription_preferences: Option<TranscriptionPreferencesOutput>,
    pub ai_user_profile: Option<AiUserProfileOutput>,
}

/// Folder repository operations
#[async_trait]
pub trait FolderRepository: Send + Sync {
    async fn create_folder(&self, uid: &str, folder: FolderInput) -> RepoResult<String>;
    async fn get_folder(&self, uid: &str, folder_id: &str) -> RepoResult<FolderOutput>;
    async fn list_folders(&self, uid: &str) -> RepoResult<Vec<FolderOutput>>;
    async fn update_folder(&self, uid: &str, folder_id: &str, update: FolderUpdate) -> RepoResult<()>;
    async fn delete_folder(&self, uid: &str, folder_id: &str) -> RepoResult<()>;
    async fn reorder_folders(&self, uid: &str, folder_ids: Vec<String>) -> RepoResult<()>;
}

/// Folder input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInput {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// Folder output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderOutput {
    pub id: String,
    pub uid: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Folder update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FolderUpdate {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
}

/// Chat session repository operations
#[async_trait]
pub trait ChatSessionRepository: Send + Sync {
    async fn create_chat_session(&self, uid: &str, session: ChatSessionInput) -> RepoResult<String>;
    async fn get_chat_session(&self, uid: &str, session_id: &str) -> RepoResult<ChatSessionOutput>;
    async fn list_chat_sessions(&self, uid: &str, pagination: Pagination) -> RepoResult<Vec<ChatSessionOutput>>;
    async fn update_chat_session(&self, uid: &str, session_id: &str, update: ChatSessionUpdate) -> RepoResult<()>;
    async fn delete_chat_session(&self, uid: &str, session_id: &str) -> RepoResult<()>;
}

/// Chat session input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionInput {
    pub title: String,
    pub model_id: Option<String>,
}

/// Chat session output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionOutput {
    pub id: String,
    pub uid: String,
    pub title: String,
    pub model_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Chat session update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatSessionUpdate {
    pub title: Option<String>,
    pub model_id: Option<String>,
}

/// Persona repository operations
#[async_trait]
pub trait PersonaRepository: Send + Sync {
    async fn create_persona(&self, uid: &str, persona: PersonaInput) -> RepoResult<String>;
    async fn get_persona(&self, uid: &str, persona_id: &str) -> RepoResult<PersonaOutput>;
    async fn list_personas(&self, uid: &str) -> RepoResult<Vec<PersonaOutput>>;
    async fn update_persona(&self, uid: &str, persona_id: &str, update: PersonaUpdate) -> RepoResult<()>;
    async fn delete_persona(&self, uid: &str, persona_id: &str) -> RepoResult<()>;
}

/// Persona input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaInput {
    pub name: String,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub avatar_url: Option<String>,
    pub personality_type: Option<String>,
}

/// Persona output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaOutput {
    pub id: String,
    pub uid: String,
    pub name: String,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub avatar_url: Option<String>,
    pub personality_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persona update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonaUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub avatar_url: Option<String>,
    pub personality_type: Option<String>,
}

/// Advice repository operations
#[async_trait]
pub trait AdviceRepository: Send + Sync {
    async fn create_advice(&self, uid: &str, advice: AdviceInput) -> RepoResult<String>;
    async fn get_advice(&self, uid: &str, advice_id: &str) -> RepoResult<AdviceOutput>;
    async fn list_advice(&self, uid: &str, category: Option<String>) -> RepoResult<Vec<AdviceOutput>>;
    async fn update_advice(&self, uid: &str, advice_id: &str, update: AdviceUpdate) -> RepoResult<()>;
    async fn delete_advice(&self, uid: &str, advice_id: &str) -> RepoResult<()>;
}

/// Advice input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdviceInput {
    pub title: String,
    pub content: String,
    pub category: String,
    pub reasoning: Option<String>,
    pub source: Option<String>,
}

/// Advice output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdviceOutput {
    pub id: String,
    pub uid: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub reasoning: Option<String>,
    pub source: Option<String>,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Advice update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdviceUpdate {
    pub title: Option<String>,
    pub content: Option<String>,
    pub category: Option<String>,
    pub is_enabled: Option<bool>,
}

/// Knowledge graph repository operations
#[async_trait]
pub trait KnowledgeGraphRepository: Send + Sync {
    async fn create_node(&self, uid: &str, node: KgNodeInput) -> RepoResult<String>;
    async fn get_node(&self, uid: &str, node_id: &str) -> RepoResult<KgNodeOutput>;
    async fn list_nodes(&self, uid: &str, node_type: Option<String>) -> RepoResult<Vec<KgNodeOutput>>;
    async fn update_node(&self, uid: &str, node_id: &str, update: KgNodeUpdate) -> RepoResult<()>;
    async fn delete_node(&self, uid: &str, node_id: &str) -> RepoResult<()>;
    async fn create_edge(&self, uid: &str, edge: KgEdgeInput) -> RepoResult<String>;
    async fn list_edges(&self, uid: &str, node_id: Option<String>) -> RepoResult<Vec<KgEdgeOutput>>;
    async fn delete_edge(&self, uid: &str, edge_id: &str) -> RepoResult<()>;
    async fn rebuild_graph(&self, uid: &str) -> RepoResult<RebuildGraphOutput>;
}

/// Knowledge graph node input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgNodeInput {
    pub entity_type: String,
    pub entity_id: String,
    pub content: String,
    pub confidence: f64,
    pub source: Option<String>,
}

/// Knowledge graph node output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgNodeOutput {
    pub id: String,
    pub uid: String,
    pub entity_type: String,
    pub entity_id: String,
    pub content: String,
    pub confidence: f64,
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Knowledge graph node update
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KgNodeUpdate {
    pub content: Option<String>,
    pub confidence: Option<f64>,
}

/// Knowledge graph edge input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEdgeInput {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship: String,
    pub confidence: f64,
}

/// Knowledge graph edge output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEdgeOutput {
    pub id: String,
    pub uid: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
}

/// Rebuild graph output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildGraphOutput {
    pub nodes_created: i32,
    pub edges_created: i32,
}

/// Person repository operations
#[async_trait]
pub trait PersonRepository: Send + Sync {
    async fn create_person(&self, uid: &str, person: PersonInput) -> RepoResult<String>;
    async fn get_person(&self, uid: &str, person_id: &str) -> RepoResult<PersonOutput>;
    async fn list_people(&self, uid: &str) -> RepoResult<Vec<PersonOutput>>;
    async fn update_person(&self, uid: &str, person_id: &str, update: PersonUpdate) -> RepoResult<()>;
    async fn delete_person(&self, uid: &str, person_id: &str) -> RepoResult<()>;
    async fn assign_conversation_segments(&self, uid: &str, person_id: &str, segments: Vec<SegmentAssignment>) -> RepoResult<()>;
}

/// Person input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonInput {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    pub notes: Option<String>,
}

/// Person output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonOutput {
    pub id: String,
    pub uid: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Person update fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonUpdate {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    pub notes: Option<String>,
}

/// Segment assignment for conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentAssignment {
    pub conversation_id: String,
    pub segment_index: i32,
    pub speaker_label: Option<String>,
}

/// LLM usage repository operations
#[async_trait]
pub trait LlmUsageRepository: Send + Sync {
    async fn record_usage(&self, uid: &str, usage: LlmUsageInput) -> RepoResult<()>;
    async fn get_usage_stats(&self, uid: &str, start_date: DateTime<Utc>, end_date: DateTime<Utc>) -> RepoResult<LlmUsageStats>;
}

/// LLM usage input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsageInput {
    pub model_id: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
    pub cost_usd: Option<f64>,
}

/// LLM usage stats output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsageStats {
    pub total_requests: i32,
    pub total_input_tokens: i32,
    pub total_output_tokens: i32,
    pub total_tokens: i32,
    pub total_cost_usd: f64,
}

/// Screen activity repository operations
#[async_trait]
pub trait ScreenActivityRepository: Send + Sync {
    async fn record_activity(&self, uid: &str, activity: ScreenActivityInput) -> RepoResult<()>;
    async fn get_recent_activity(&self, uid: &str, limit: usize) -> RepoResult<Vec<ScreenActivityOutput>>;
    async fn delete_old_activity(&self, uid: &str, before: DateTime<Utc>) -> RepoResult<i32>;
}

/// Screen activity input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenActivityInput {
    pub window_title: String,
    pub app_name: String,
    pub duration_seconds: i32,
    pub timestamp: DateTime<Utc>,
}

/// Screen activity output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenActivityOutput {
    pub id: String,
    pub uid: String,
    pub window_title: String,
    pub app_name: String,
    pub duration_seconds: i32,
    pub timestamp: DateTime<Utc>,
}
