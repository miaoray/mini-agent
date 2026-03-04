# Mini-Agent Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a lightweight desktop Agent app with Tauri 2 + React: chat with LLM (MiniMax M2.5), web search, fetch, file operations (with user approval), streaming, multi-session.

**Architecture:** Tauri 2 with Rust backend (LLM calls, tools, SQLite) and React frontend (UI). Rust emits events for streaming; frontend listens. Tools: native implementations first (web_search via DuckDuckGo, fetch_url, create_directory, write_file).

**Tech Stack:** Tauri 2, React, TypeScript, Vite, SQLite (rusqlite), reqwest, OpenAI-compatible SDK for MiniMax, duckduckgo-search (Rust), Zustand. Tests: cargo test (Rust), Vitest (frontend), Playwright (E2E).

---

## Phase 1: Project Scaffold

### Task 1: Create Tauri 2 + React + TypeScript project

**Files:**
- Create: (via CLI) entire project structure
- Create: `.env.example`
- Create: `.gitignore` (add `.env`)

**Step 1: Run create-tauri-app**

```bash
cd /Users/popo/mini-agent
npx create-tauri-app@latest
```

When prompted: App name `mini-agent`, Template `react-ts`, Package manager `npm`. This creates `src/`, `src-tauri/`, `package.json`, etc. in current dir. If folder already has `docs/`, the CLI will add Tauri files alongside.

**Step 2: Create .env.example**

Create `mini-agent/.env.example`:

```
MINIMAX_API_KEY=your_minimax_api_key_here
MINIMAX_BASE_URL=https://api.minimax.chat/v1
```

**Step 3: Update .gitignore**

Add to `.gitignore`:

```
.env
```

**Step 4: Verify project runs**

```bash
npm install
npm run tauri dev
```

Expected: App window opens with default React template.

**Step 5: Commit**

```bash
git add .
git commit -m "chore: scaffold Tauri 2 + React + TypeScript"
```

---

### Task 1b: Set up test infrastructure

**Files:**
- Modify: `package.json` (add vitest, @playwright/test)
- Create: `vitest.config.ts`
- Create: `playwright.config.ts`
- Modify: `src-tauri/Cargo.toml` (ensure dev-dependencies for test)

**Step 1: Add Vitest for frontend unit tests**

```bash
npm install -D vitest @vitejs/plugin-react jsdom
```

Add to `package.json` scripts: `"test": "vitest run"`, `"test:watch": "vitest"`.

Create `vitest.config.ts`:

```ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()],
  test: { environment: 'jsdom', include: ['src/**/*.test.{ts,tsx}'] },
});
```

**Step 2: Add Playwright for E2E**

```bash
npm install -D @playwright/test
npx playwright install
```

Create `playwright.config.ts` (base config; Tauri dev URL or custom as needed).

**Step 3: Verify Rust tests**

```bash
cd src-tauri && cargo test
```

Expected: No tests yet, but `cargo test` runs without error.

**Step 4: Write placeholder frontend test**

Create `src/App.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';
describe('App', () => {
  it('renders', () => {
    render(<App />);
    expect(screen.getByRole('main') || document.body).toBeTruthy();
  });
});
```

Run: `npm test`. Expected: PASS (adapt selector to actual App output).

**Step 5: Commit**

```bash
git commit -m "chore: add Vitest, Playwright, and placeholder test"
```

---

## Phase 2: Storage & Domain Models

### Task 2: Add SQLite and create schema

**Files:**
- Modify: `src-tauri/Cargo.toml` (add rusqlite, dotenvy)
- Create: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/db/schema.sql`
- Modify: `src-tauri/src/lib.rs` (init db)

**Step 1: Add dependencies to Cargo.toml**

Add to `src-tauri/Cargo.toml`:

```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
dotenvy = "0.15"
```

**Step 2: Create schema.sql**

Create `src-tauri/src/db/schema.sql`:

```sql
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
    provider_id TEXT NOT NULL,
    user_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (provider_id) REFERENCES provider(id)
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

CREATE INDEX idx_message_conversation ON message(conversation_id);
CREATE INDEX idx_pending_approval_status ON pending_approval(status);
```

**Step 3: Create db/mod.rs**

Create `src-tauri/src/db/mod.rs`:

```rust
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

pub fn init_db(app_handle: &tauri::AppHandle) -> Result<Connection, rusqlite::Error> {
    let app_data = tauri::api::path::app_data_dir(&app_handle.config())
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&app_data).ok();
    let db_path = app_data.join("mini-agent.db");
    let conn = Connection::open(db_path)?;
    let schema = include_str!("schema.sql");
    conn.execute_batch(schema)?;
    Ok(conn)
}
```

Note: Call `init_db` in `setup()` hook when `App` is available. Store `Connection` in `tauri::State`. For Tauri 2, verify `tauri::api::path::app_data_dir` API.

**Step 4: Wire db init in lib.rs**

In `src-tauri/src/lib.rs`, before `run()`, call `db::init_db()` and store in state. Add `tauri::Manager` state for the connection.

**Step 5: Add unit test for schema**

Create `src-tauri/src/db/mod.rs` with a `#[cfg(test)]` module, or create `src-tauri/src/db/schema_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    #[test]
    fn schema_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='conversation'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }
}
```

**Step 6: Run test**

```bash
cd src-tauri && cargo test
```

Expected: PASS.

**Step 7: Commit**

```bash
git add .
git commit -m "feat: add SQLite schema and db init"
```

---

### Task 3: Seed default provider and implement CRUD for conversations

**Files:**
- Create: `src-tauri/src/db/conversation.rs`
- Create: `src-tauri/src/db/provider.rs`
- Modify: `src-tauri/src/db/mod.rs` (re-export)
- Modify: `src-tauri/src/lib.rs` (seed provider on first run)

**Step 1: Create provider.rs**

Implement `insert_default_provider`, `get_provider_by_id`, `list_providers`. Default: MiniMax M2.5, read base_url and model from env.

**Step 2: Create conversation.rs**

Implement `create_conversation`, `list_conversations`, `get_conversation`, `update_conversation_title`.

**Step 3: Seed default provider in init_db**

After running schema, insert default provider if none exists.

**Step 4: Expose Tauri commands**

- `create_conversation` -> returns new conversation id
- `list_conversations` -> returns Vec<Conversation>
- `get_conversation(id)` -> returns Conversation

Register in `lib.rs` and add to `capabilities/default.json`.

**Step 5: Add unit tests for conversation CRUD**

In `src-tauri/src/db/conversation.rs` or `conversation_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    fn test_conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("schema.sql")).unwrap();
        // Seed provider
        c.execute("INSERT INTO provider (id,name,type,base_url,model_id,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            ["minimax","MiniMax M2.5","openai","https://api.minimax.chat/v1","abab6.5", &chrono::Utc::now().timestamp().to_string()]).unwrap();
        c
    }
    #[test]
    fn create_and_list_conversations() {
        let conn = test_conn();
        let id1 = create_conversation(&conn, "minimax").unwrap();
        let id2 = create_conversation(&conn, "minimax").unwrap();
        let list = list_conversations(&conn).unwrap();
        assert!(list.len() >= 2);
        assert!(list.iter().any(|c| c.id == id1));
    }
}
```

Adjust `create_conversation` / `list_conversations` signatures to accept `&Connection` for testability. Run: `cargo test -p mini-agent` (or crate name).

**Step 6: Run tests**

```bash
cd src-tauri && cargo test
```

Expected: PASS.

**Step 7: Manual verify**

Call from frontend or DevTools: `invoke('create_conversation')`.

**Step 8: Commit**

```bash
git commit -m "feat: add provider seed and conversation CRUD"
```

---

## Phase 3: LLM Provider Abstraction

### Task 4: Implement LLM client (OpenAI-compatible, MiniMax)

**Files:**
- Create: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/src/llm/minimax.rs`
- Modify: `src-tauri/Cargo.toml` (add reqwest, tokio, serde, futures)

**Step 1: Add dependencies**

```toml
reqwest = { version = "0.11", features = ["json", "stream"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
futures = "0.3"
```

**Step 2: Implement chat completion (non-streaming first)**

- Load `MINIMAX_API_KEY`, `MINIMAX_BASE_URL` from env
- POST to `{base_url}/chat/completions` with OpenAI-compatible body
- Parse response, return assistant content

**Step 3: Implement streaming**

- Use `reqwest::Client::post().body().send()` with `stream: true`
- Return `impl Stream<Item = Result<String, _>>` or use callback/emit
- For now, collect chunks and return full string (streaming to frontend in Task 8)

**Step 4: Add unit test with WireMock**

Add dev-dependency: `wiremock = "0.5"`. Create `src-tauri/src/llm/mod_test.rs` or `#[cfg(test)]` in mod.rs:

```rust
#[cfg(test)]
mod tests {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path};
    #[tokio::test]
    async fn parses_openai_compatible_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"choices":[{"message":{"content":"hi"}}]}"#))
            .mount(&server).await;
        // Call client with server.uri() as base_url, assert "hi" in response
    }
}
```

Or simpler: test that `build_messages_json()` produces valid JSON. Run: `cargo test`.

**Step 5: Commit**

```bash
git commit -m "feat: add MiniMax LLM client (OpenAI-compatible)"
```

---

## Phase 4: Tool Registry & Implementations

### Task 5: Tool abstraction and registry

**Files:**
- Create: `src-tauri/src/tools/mod.rs`
- Create: `src-tauri/src/tools/registry.rs`
- Create: `src-tauri/src/tools/types.rs`

**Step 1: Define Tool trait and ToolCall schema**

```rust
pub struct ToolDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

pub trait ToolImpl {
    fn definition(&self) -> ToolDef;
    async fn execute(&self, args: serde_json::Value) -> Result<String, String>;
}
```

**Step 2: Implement ToolRegistry**

- `register(tool: impl ToolImpl)`
- `get_tools_for_llm() -> Vec<OpenAI-format tool def>`
- `execute(name: &str, args: Value) -> Result<String, String>`

**Step 3: Add unit test**

Create a stub tool that returns "ok" for execute. Test:
- `register` then `get_tools_for_llm()` returns non-empty
- `execute("stub_tool", {"x":"y"})` returns `Ok("ok")`
- `execute("unknown_tool", {})` returns `Err`

Run: `cargo test`.

**Step 4: Commit**

```bash
git commit -m "feat: add tool abstraction and registry"
```

---

### Task 6: Implement web_search tool (DuckDuckGo)

**Files:**
- Modify: `src-tauri/Cargo.toml` (add duckduckgo_search or similar)
- Create: `src-tauri/src/tools/web_search.rs`
- Modify: `src-tauri/src/tools/mod.rs` (register)

**Step 1: Add crate**

Check crates.io for `duckduckgo_search` or `duckduckgo`. Use `duckduckgo_search = "0.1"` or equivalent.

**Step 2: Implement WebSearchTool**

- Query DuckDuckGo
- Format top 3-5 results as concise text (~150 chars each)
- Return string for LLM context

**Step 3: Register in main**

**Step 4: Add unit test for result formatting**

Extract a pure function `format_search_results(results: &[SearchResult], max_per_result: usize) -> String`. Test:
- Empty results -> empty or "No results"
- 3 results -> output has 3 items, each truncated to ~150 chars
- 5+ results -> at most 5 in output

Run: `cargo test`. (Skip live DuckDuckGo call in test; mock or use formatting-only.)

**Step 5: Commit**

```bash
git commit -m "feat: add web_search tool via DuckDuckGo"
```

---

### Task 7: Implement fetch_url, create_directory, write_file tools

**Files:**
- Create: `src-tauri/src/tools/fetch_url.rs`
- Create: `src-tauri/src/tools/create_directory.rs`
- Create: `src-tauri/src/tools/write_file.rs`
- Modify: `src-tauri/src/tools/mod.rs`

**Step 1: fetch_url**

- Use reqwest to GET URL
- For HTML: extract text (use `scraper` or regex), truncate to ~2000 chars
- Return summary-friendly string

**Step 2: create_directory**

- Validate path (no traversal outside user dirs)
- Return "PENDING_APPROVAL" with payload { path } — do NOT create yet
- Actual creation happens in approval handler (Phase 6)

**Step 3: write_file**

- Validate path
- Return "PENDING_APPROVAL" with payload { path, content }
- Actual write in approval handler

**Step 4: Add unit tests**

- **fetch_url**: Use WireMock to serve HTML, assert returned text is truncated (~2000 chars) and contains expected content
- **create_directory**: Call with path `"/tmp/test_create_dir"` (or temp dir); assert returns `PENDING_APPROVAL` and directory NOT created; assert path `"../../../etc"` returns `Err`
- **write_file**: Same PENDING check; assert path traversal returns `Err`

Run: `cargo test`.

**Step 5: Commit**

```bash
git commit -m "feat: add fetch_url, create_directory, write_file tools"
```

---

## Phase 5: Agent Orchestrator

### Task 8: Agent loop with tool calls and streaming

**Files:**
- Create: `src-tauri/src/agent/mod.rs`
- Create: `src-tauri/src/agent/loop.rs`
- Modify: `src-tauri/src/lib.rs` (expose send_message command)
- Modify: `src-tauri/capabilities/default.json` (allow invoke, listen)

**Step 1: Implement agent loop**

1. Load conversation messages
2. Build messages array for LLM
3. Call LLM with tools + stream
4. On stream chunk: emit `chat-delta` event with `{ messageId, delta }`
5. On tool_calls: for each call:
   - If create_directory/write_file: create PendingApproval row, emit `pending-approval` event, pause
   - Else: execute tool, append result to messages, continue loop
6. On completion: persist message, emit `chat-done`

**Step 2: Tauri command `send_message`**

- Params: conversation_id, content
- Spawn async task; command returns message_id immediately
- Task runs agent loop, emits events
- App handle must be passed to task for `app.emit()`

**Step 3: Frontend listener**

In React: `listen('chat-delta', ...)` and `listen('chat-done', ...)`, append to current message state.

**Step 4: Add integration test for message building**

Unit test: `build_messages_for_llm(conn, conversation_id)` returns correct JSON structure (roles, content). Mock DB or use in-memory.

**Step 5: Manual verify streaming**

Send a message, see tokens appear in UI.

**Step 6: Commit**

```bash
git commit -m "feat: agent loop with tool calls and streaming"
```

---

## Phase 6: User Approval Flow

### Task 9: Approval backend and UI

**Files:**
- Create: `src-tauri/src/approval/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add approve_*, reject_* commands)
- Create: `src/components/ApprovalCard.tsx`
- Modify: `src/App.tsx` or chat view (render approval cards)

**Step 1: Implement approve and reject commands**

- `approve_action(approval_id)`: read PendingApproval, execute (create dir / write file), update status, resume agent
- `reject_action(approval_id)`: update status, inject "User rejected" into messages, resume agent

**Step 2: Emit pending-approval on tool call**

When create_directory/write_file returns PENDING, insert row, emit `pending-approval` with payload.

**Step 3: Create ApprovalCard component**

Display: action type, path, content preview (for write_file). Buttons: Accept, Reject. On click, call `invoke('approve_action', { id })` or `invoke('reject_action', { id })`.

**Step 4: Integrate in chat view**

When `pending-approval` received, add to local state, render ApprovalCard above input or inline in message.

**Step 5: Add unit test for approval execution**

Test `execute_approval(conn, approval_id)`: given a PendingApproval row for create_directory, calls `fs::create_dir_all`, updates status to `approved`. Use temp dir. For reject: status becomes `rejected`, no fs op.

**Step 6: Manual verify**

Trigger create_directory from Agent, see card, Accept, verify dir created.

**Step 7: Commit**

```bash
git commit -m "feat: user approval flow for create_directory and write_file"
```

---

## Phase 7: UI Polish

### Task 10: Conversation list, message list, and layout

**Files:**
- Create: `src/components/Sidebar.tsx`
- Create: `src/components/ChatView.tsx`
- Create: `src/components/MessageList.tsx`
- Create: `src/components/MessageBubble.tsx`
- Create: `src/stores/conversationStore.ts` (Zustand)
- Modify: `src/App.tsx`

**Step 1: Sidebar**

- List conversations (`list_conversations`)
- "New Chat" button -> `create_conversation`
- Click conversation -> set current conversation id

**Step 2: ChatView**

- Message list for current conversation
- Input at bottom
- On submit -> `send_message`, listen for `chat-delta` / `chat-done`

**Step 3: MessageBubble**

- Role (user/assistant), content, streaming indicator

**Step 4: Zustand store**

- currentConversationId, conversations, messages, addMessage, appendDelta, etc.

**Step 5: Add component tests**

- `Sidebar.test.tsx`: Renders "New Chat" button; when conversations provided, renders list
- `MessageBubble.test.tsx`: Renders user/assistant content, shows streaming indicator when `streaming={true}`

Run: `npm test`.

**Step 6: Commit**

```bash
git commit -m "feat: conversation sidebar and chat layout"
```

---

### Task 11: Loading .env and API key check

**Files:**
- Modify: `src-tauri/src/lib.rs` (load dotenvy at startup)
- Create: `src-tauri/src/commands/check_config.rs`
- Create: `src/components/ConfigBanner.tsx`
- Modify: `src/App.tsx` (show banner if no API key)

**Step 1: Load .env in Rust**

At start of `main` or before db init: `dotenvy::dotenv().ok();`

**Step 2: Add `check_config` command**

Returns `{ hasApiKey: bool }` by checking env.

**Step 3: ConfigBanner**

If `!hasApiKey`, show "请配置 .env 中的 MINIMAX_API_KEY" with link to .env.example.

**Step 4: Add unit test**

`ConfigBanner.test.tsx`: When `hasApiKey={false}`, renders warning text; when `hasApiKey={true}`, renders nothing or hides.

Run: `npm test`.

**Step 5: Commit**

```bash
git commit -m "feat: .env loading and API key check UI"
```

---

## Phase 8: Integration & Scenarios

### Task 12: Playwright E2E automation

**Files:**
- Create: `e2e/s1-single-turn.spec.ts`
- Create: `e2e/s6-no-apikey.spec.ts`
- Create: `e2e/s7-persistence.spec.ts`
- Modify: `playwright.config.ts` (Tauri app URL, e.g. `tauri://localhost` or dev server)

**Step 1: Configure Playwright for Tauri**

Tauri 2 runs in native window. Use `tauri-driver` or `tauri-test-runner` if available; otherwise target the webview URL when running `tauri dev` (e.g. `http://localhost:1420`). Set `baseURL` in playwright.config.

**Step 2: Write E2E for S6.1 (no API key)**

- Start app with env that has no MINIMAX_API_KEY
- Assert: ConfigBanner or warning message is visible

**Step 3: Write E2E for S1.1 (single turn, if runnable without real API)**

- If using mock: intercept LLM, return stub; send message; assert assistant reply appears
- Else: mark as manual-only, add `test.skip()` with TODO

**Step 4: Write E2E for S7.1 (persistence)**

- Create conversation, send message
- Restart app (or reload)
- Assert: conversation still in list, messages visible

**Step 5: Add npm script**

`"test:e2e": "playwright test"`. Run: `npm run test:e2e`. Fix any flakiness.

**Step 6: Commit**

```bash
git commit -m "feat: add Playwright E2E for key scenarios"
```

---

### Task 13: CI configuration

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Create GitHub Actions workflow**

```yaml
name: CI
on: [push, pull_request]
jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cd src-tauri && cargo test
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '20' }
      - run: npm ci
      - run: npm test
  build:
    runs-on: ubuntu-latest
    needs: [rust, frontend]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npm run tauri build
```

**Step 2: Add cache for cargo and npm** (optional, speeds CI)

**Step 3: Verify**

Push to branch, check Actions tab. All jobs pass.

**Step 4: Commit**

```bash
git add .github/
git commit -m "ci: add GitHub Actions for tests and build"
```

---

### Task 14: Manual E2E checklist and fixes

**Run through remaining design doc scenarios manually:**

- S1.2: 多轮上下文
- S1.3: 多会话切换
- S2.1: web_search 调用
- S3.1: fetch 网页
- S4.1/S4.2: 确认流程
- S5.1: 流式打字机
- S6.2, S6.3: 限流/超时、切换 Provider

**Step 1: Document results**

Create `docs/validation/2026-03-01-results.md` with pass/fail per scenario.

**Step 2: Fix any failures, re-verify**

**Step 3: Commit**

```bash
git commit -m "chore: E2E validation and fixes"
```

---

## Execution Handoff

**Plan complete and saved to `docs/plans/2026-03-01-mini-agent-impl.md`.**

**Two execution options:**

1. **Subagent-Driven (this session)** — I dispatch a fresh subagent per task, review between tasks, iterate quickly.

2. **Parallel Session (separate)** — Open a new session with executing-plans in a dedicated worktree for batch execution with checkpoints.

**Which approach?**
