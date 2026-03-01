# Mini-Agent

A lightweight desktop Agent app: chat with LLMs, web search, fetch content, and file operations (with user approval). Supports multiple sessions and streaming output.

## Features

- **Multi-session**: Multiple independent conversations with isolated history
- **Streaming output**: Typewriter-style, token-by-token display
- **Native tools**: `web_search`, `fetch_url`, `create_directory`, `write_file`
- **User approval**: Cursor-style Accept/Reject before writing files or creating directories
- **Multi-provider**: MiniMax M2.5 first, switchable to other OpenAI/Anthropic-compatible LLMs
- **Local-only**: No login; data stored in local SQLite

## Tech stack

| Layer   | Choice                    |
|---------|---------------------------|
| Desktop | Tauri 2                   |
| Frontend| React + TypeScript + Vite |
| State   | Zustand                   |
| Storage | SQLite (rusqlite)         |
| LLM     | OpenAI-compatible API (base_url) |
| Config  | `.env` (API keys, etc.)   |

## Quick start

> Implementation may live on a separate branch or git worktree. The steps below assume the repo root contains the frontend and Tauri app.

1. **Clone and enter the project**
   ```bash
   git clone <repo-url> mini-agent && cd mini-agent
   ```

2. **Configure API key**
   ```bash
   cp .env.example .env
   # Edit .env and set MINIMAX_API_KEY etc.
   ```

3. **Install and run**
   ```bash
   npm install
   npm run tauri dev
   ```

4. **Tests**
   ```bash
   npm test                    # Frontend unit tests (Vitest)
   cd src-tauri && cargo test  # Rust unit tests
   npx playwright test         # E2E (requires app running or mock env)
   ```

## Project structure (reference)

- `src/`: React frontend
- `src-tauri/`: Tauri 2 + Rust backend (LLM, tools, SQLite)
- `.env.example`: Environment variable template (do not commit `.env`)

## Docs and plans

- **Design**: `docs/plans/2026-03-01-mini-agent-design.md` (domain model, architecture, acceptance scenarios)
- **Implementation plan**: `docs/plans/2026-03-01-mini-agent-impl.md` (phased tasks and steps)
- **Execution log**: `docs/plans/2026-03-01-mini-agent-impl-execution-log.md` (task status and remaining work)

## Platforms and status

- **Target platforms**: macOS, Windows (Linux out of scope for now)
- **In progress**: CI setup, manual E2E checklist and follow-up fixes

## License

Unspecified; add as needed.
