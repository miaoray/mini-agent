PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS provider (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conversation (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT 'New Chat',
    user_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS message (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversation(id)
);

CREATE TABLE IF NOT EXISTS agent_turn (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES message(id),
    FOREIGN KEY (provider_id) REFERENCES provider(id)
);

CREATE TABLE IF NOT EXISTS tool (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    impl_ref TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_invocation (
    id TEXT PRIMARY KEY,
    tool_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    result_text TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (tool_id) REFERENCES tool(id),
    FOREIGN KEY (turn_id) REFERENCES agent_turn(id)
);

CREATE TABLE IF NOT EXISTS pending_approval (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    action_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversation(id),
    FOREIGN KEY (turn_id) REFERENCES agent_turn(id)
);

CREATE INDEX IF NOT EXISTS idx_message_conversation
    ON message (conversation_id);

CREATE INDEX IF NOT EXISTS idx_pending_approval_status
    ON pending_approval (status);

CREATE TABLE IF NOT EXISTS llm_debug_log (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL,
    request_json TEXT NOT NULL,
    response_json TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (turn_id) REFERENCES agent_turn(id)
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_llm_debug_log_turn
    ON llm_debug_log (turn_id);
