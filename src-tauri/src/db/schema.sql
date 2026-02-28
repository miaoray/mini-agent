PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS provider (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    base_url TEXT,
    api_key_env_var TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS conversation (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT,
    title TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS message (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversation(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS agent_turn (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    error TEXT,
    FOREIGN KEY (conversation_id) REFERENCES conversation(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tool (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE IF NOT EXISTS tool_invocation (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_turn_id INTEGER NOT NULL,
    tool_id INTEGER,
    invocation_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    result_json TEXT,
    status TEXT NOT NULL DEFAULT 'completed',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (agent_turn_id) REFERENCES agent_turn(id) ON DELETE CASCADE,
    FOREIGN KEY (tool_id) REFERENCES tool(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS pending_approval (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    tool_invocation_id INTEGER,
    status TEXT NOT NULL DEFAULT 'pending',
    request_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TEXT,
    FOREIGN KEY (conversation_id) REFERENCES conversation(id) ON DELETE CASCADE,
    FOREIGN KEY (tool_invocation_id) REFERENCES tool_invocation(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_message_conversation
    ON message (conversation_id);

CREATE INDEX IF NOT EXISTS idx_pending_approval_status
    ON pending_approval (status);
