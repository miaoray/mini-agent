# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Mini-Agent is a lightweight desktop Agent application built with Tauri 2. It provides a chat interface for interacting with LLMs (MiniMax M2.5 first, OpenAI/Anthropic-compatible), with built-in tools for web search, content fetching, and file operations that require user approval.

## Common Commands

```bash
# Frontend development
npm run dev              # Run Vite dev server
npm run build            # Build frontend (tsc + vite build)

# Tauri development
npm run tauri dev        # Run full Tauri app in dev mode
npm run tauri build     # Build production Tauri app

# Testing
npm test                 # Frontend unit tests (Vitest)
npm run test:watch       # Watch mode for unit tests
npm run test:e2e         # E2E tests (Playwright)
npm run test:e2e:ui      # E2E tests with UI
npm run test:e2e:headed  # E2E tests with browser visible

# Rust testing
cd src-tauri && cargo test
```

## Architecture

### Frontend (`src/`)
- **React 19** with TypeScript and Vite 7
- **Zustand** for state management (`src/stores/`)
- Components in `src/components/`, organized by feature
- Event bridge for Tauri IPC communication (`src/eventBridge.ts`)

### Backend (`src-tauri/src/`)
- **Tauri 2** with Rust
- **Modules**:
  - `agent/` - Agent loop and conversation handling
  - `commands/` - Tauri command handlers
  - `db/` - SQLite database operations (conversations, messages, providers)
  - `llm/` - LLM integration (MiniMax, OpenAI-compatible)
  - `tools/` - Tool implementations (web_search, fetch_url, create_directory, write_file, get_time)
  - `approval/` - User approval workflow for file operations

### Data Flow
1. User sends message via React UI
2. Frontend calls Tauri command via event bridge
3. Rust backend processes through agent loop
4. LLM responds with text and/or tool calls
5. Tools requiring approval trigger `NeedsApproval` branch; others execute immediately
6. Results streamed back to frontend

### Configuration
- `.env` file for API keys and settings (see `.env.example`)
- Key variables: `MINIMAX_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_BASE_URL`

## Key Implementation Details

- **Streaming**: LLM responses stream token-by-token to frontend
- **Multi-session**: Each conversation has isolated history stored in SQLite
- **User Approval**: File write/create operations require explicit Accept/Reject (Cursor-style)
- **Safety Guards**: `MAX_TOOL_LOOP_STEPS=20` prevents runaway loops, `MAX_IDENTICAL_TOOL_CALLS=2` blocks repetitive tool use
