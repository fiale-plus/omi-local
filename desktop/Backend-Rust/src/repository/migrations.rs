// Database migrations
// Handles schema versioning and upgrades

use rusqlite::{Connection, Result};

/// Current schema version - bump on any schema changes
pub const CURRENT_SCHEMA_VERSION: i32 = 1;

/// Run all pending migrations
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    // Get current version
    let current_version: i32 = conn.query_row(
        "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    if current_version < CURRENT_SCHEMA_VERSION {
        // Run migration from 0 to 1
        if current_version < 1 {
            migrate_to_v1(conn)?;
        }

        // Add more migrations here as needed
        // if current_version < 2 {
        //     migrate_to_v2(conn)?;
        // }
    }

    Ok(())
}

/// Migration to schema version 1
fn migrate_to_v1(conn: &Connection) -> Result<()> {
    tracing::info!("Running migration to schema v1");

    // Users table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT,
            display_name TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )?;

    // Memories table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            content TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT 'manual',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            conversation_id TEXT,
            reviewed INTEGER NOT NULL DEFAULT 0,
            user_review INTEGER,
            visibility TEXT NOT NULL DEFAULT 'private',
            manually_added INTEGER NOT NULL DEFAULT 1,
            scoring TEXT,
            confidence REAL,
            source_app TEXT,
            context_summary TEXT,
            is_read INTEGER NOT NULL DEFAULT 0,
            is_dismissed INTEGER NOT NULL DEFAULT 0,
            tags TEXT NOT NULL DEFAULT '[]',
            reasoning TEXT,
            current_activity TEXT,
            window_title TEXT,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Conversations table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            title TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            source TEXT,
            geolocation TEXT,
            last_message_at TEXT,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Messages table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            conversation_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            audio_url TEXT,
            transcript TEXT,
            created_at TEXT NOT NULL,
            rating INTEGER,
            person_id TEXT,
            FOREIGN KEY (uid) REFERENCES users(id),
            FOREIGN KEY (conversation_id) REFERENCES conversations(id)
        )",
        [],
    )?;

    // Action items table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS action_items (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            content TEXT NOT NULL,
            completed INTEGER NOT NULL DEFAULT 0,
            due_date TEXT,
            priority INTEGER NOT NULL DEFAULT 0,
            conversation_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            score REAL,
            FOREIGN KEY (uid) REFERENCES users(id),
            FOREIGN KEY (conversation_id) REFERENCES conversations(id)
        )",
        [],
    )?;

    // Focus sessions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS focus_sessions (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT,
            focus_type TEXT,
            distraction_events TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Goals table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            title TEXT NOT NULL,
            goal_type TEXT NOT NULL,
            target_date TEXT,
            progress REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Goal history table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS goal_history (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            goal_id TEXT NOT NULL,
            date TEXT NOT NULL,
            progress REAL NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id),
            FOREIGN KEY (goal_id) REFERENCES goals(id)
        )",
        [],
    )?;

    // User settings table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS user_settings (
            uid TEXT PRIMARY KEY,
            daily_summary_enabled INTEGER NOT NULL DEFAULT 1,
            notification_settings TEXT NOT NULL DEFAULT '{}',
            transcription_preferences TEXT NOT NULL DEFAULT '{}',
            ai_user_profile TEXT,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Folders table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS folders (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            name TEXT NOT NULL,
            icon TEXT,
            color TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Chat sessions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chat_sessions (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            title TEXT NOT NULL,
            model_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Personas table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS personas (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            prompt TEXT,
            avatar_url TEXT,
            personality_type TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Advice table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS advice (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            category TEXT NOT NULL,
            reasoning TEXT,
            source TEXT,
            is_enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Knowledge graph nodes table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kg_nodes (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            content TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 1.0,
            source TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Knowledge graph edges table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kg_edges (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            source_node_id TEXT NOT NULL,
            target_node_id TEXT NOT NULL,
            relationship TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 1.0,
            created_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id),
            FOREIGN KEY (source_node_id) REFERENCES kg_nodes(id),
            FOREIGN KEY (target_node_id) REFERENCES kg_nodes(id)
        )",
        [],
    )?;

    // People table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS people (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            name TEXT NOT NULL,
            email TEXT,
            phone TEXT,
            avatar_url TEXT,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // LLM usage table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS llm_usage (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            model_id TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL,
            cost_usd REAL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Screen activity table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS screen_activity (
            id TEXT PRIMARY KEY,
            uid TEXT NOT NULL,
            window_title TEXT NOT NULL,
            app_name TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL,
            timestamp TEXT NOT NULL,
            FOREIGN KEY (uid) REFERENCES users(id)
        )",
        [],
    )?;

    // Create indexes for common queries
    conn.execute("CREATE INDEX IF NOT EXISTS idx_memories_uid ON memories(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_memories_uid_category ON memories(uid, category)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_memories_uid_created ON memories(uid, created_at DESC)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_conversations_uid ON conversations(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_conversations_uid_last_message ON conversations(uid, last_message_at DESC)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_messages_uid_conv ON messages(uid, conversation_id)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_messages_uid_created ON messages(uid, created_at)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_action_items_uid ON action_items(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_action_items_uid_completed ON action_items(uid, completed)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_focus_sessions_uid ON focus_sessions(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_focus_sessions_uid_start ON focus_sessions(uid, start_time DESC)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_goals_uid ON goals(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_folders_uid ON folders(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_folders_uid_sort ON folders(uid, sort_order)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_chat_sessions_uid ON chat_sessions(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_personas_uid ON personas(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_advice_uid ON advice(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_kg_nodes_uid ON kg_nodes(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_kg_edges_uid ON kg_edges(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_people_uid ON people(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_llm_usage_uid ON llm_usage(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_screen_activity_uid ON screen_activity(uid)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_screen_activity_uid_timestamp ON screen_activity(uid, timestamp DESC)", [])?;

    // Record migration
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (1)",
        [],
    )?;

    tracing::info!("Migration to schema v1 completed");
    Ok(())
}
