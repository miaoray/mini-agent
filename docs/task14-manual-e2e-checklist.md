# Task 14 Manual E2E Checklist

This checklist validates real user flows in the desktop app (`npm run tauri dev`) beyond mocked Playwright coverage.

## Environment

- OS: macOS / Windows
- App mode: Tauri desktop (`npm run tauri dev`)
- API config: `.env` with valid `MINIMAX_API_KEY`
- Optional: set `MINIMAX_BASE_URL` if using Anthropic-compatible endpoint

## Pass/Fail Rules

- Mark each item as `PASS` or `FAIL`.
- For any `FAIL`, record exact repro steps and screenshots/log excerpts.
- A scenario is complete only when expected UI behavior and backend side effects both match.

## Checklist


| ID     | Scenario                             | Steps                                               | Expected Result                                                       | Status | Notes |
| ------ | ------------------------------------ | --------------------------------------------------- | --------------------------------------------------------------------- | ------ | ----- |
| T14-01 | No API key guard                     | Remove API key and launch app                       | Config banner is shown; send flow is blocked or clearly warned        | TODO   |       |
| T14-02 | Single turn chat                     | Restore API key, send one normal prompt             | Assistant response streams and finishes cleanly                       | TODO   |       |
| T14-03 | `web_search` tool path               | Ask for latest topic requiring search               | Tool executes and final assistant answer includes search-derived info | RETEST | Hit max tool loop error once; backend guard/fallback fix applied, awaiting rerun |
| T14-04 | `fetch_url` tool path                | Ask agent to summarize `https://www.rust-lang.org/` | Tool succeeds; assistant returns fetched summary                      | TODO   |       |
| T14-05 | Approval reject (`write_file`)       | Ask agent to write file, click Reject               | No file is written; assistant acknowledges rejection                  | TODO   |       |
| T14-06 | Approval accept (`write_file`)       | Ask agent to write file, click Approve              | File is created with expected content; assistant confirms success     | TODO   |       |
| T14-07 | Approval reject (`create_directory`) | Ask agent to create directory, click Reject         | No directory is created; assistant acknowledges rejection             | TODO   |       |
| T14-08 | Approval accept (`create_directory`) | Ask agent to create directory, click Approve        | Directory is created at expected path                                 | TODO   |       |
| T14-09 | Multi-session isolation              | Create two chats, send different prompts            | Messages stay isolated per conversation                               | TODO   |       |
| T14-10 | Restart persistence                  | Close app and reopen                                | Conversation list and message history persist                         | TODO   |       |
| T14-11 | Submit race protection               | Rapidly press send on same prompt                   | Only one user message is sent; no duplicate turn                      | TODO   |       |
| T14-12 | Error surfacing                      | Trigger network error (invalid base URL)            | UI shows actionable error and app stays usable                        | TODO   |       |
| T14-13 | SSRF guard sanity                    | Ask to fetch localhost/private URL                  | Request is blocked with clear safety error                            | TODO   |       |
| T14-14 | Path safety guard                    | Ask for absolute/traversal file path write          | Request is rejected before approval with clear error                  | TODO   |       |


## Current Execution Progress


| ID     | Item                                 | Result      | Notes                                                               |
| ------ | ------------------------------------ | ----------- | ------------------------------------------------------------------- |
| PRE-01 | Tauri runtime startup                | PASS        | `npm run tauri dev` succeeds; if restarted while already running, Vite reports port 1420 in use |
| PRE-02 | Frontend unit regression             | PASS        | `npm test -- --run` => 16/16 passed                                 |
| PRE-03 | Backend unit/integration regression  | PASS        | `cd src-tauri && cargo test -q` => all passed                       |
| PRE-04 | Existing E2E suite regression        | PASS        | `npm run test:e2e` => 5/5 passed (mocked Tauri harness)             |
| PRE-05 | Manual desktop interaction scenarios | IN PROGRESS | Requires real in-app interaction for T14-01..T14-14                 |
| PRE-06 | Tool-loop regression automation      | PASS        | Added 3 new Rust regression tests for repeated tool-call suppression and loop-exhaust fallback |


## Follow-up Fix Log

Use this section to log fixes discovered during manual E2E:

- 2026-03-01: T14-03 failed with `agent turn reached max tool loop steps (6) without final assistant content`.
  - Fix: updated `src-tauri/src/lib.rs` to (1) suppress repeated identical tool calls after 2 executions with a model steering message, and (2) downgrade max-loop exhaustion from hard error to user-visible fallback assistant content including latest tool result.
  - Validation: `cd src-tauri && cargo test -q` passed after fix.
  - Added automated regression tests:
    - `should_skip_redundant_tool_call_allows_then_blocks_identical_calls`
    - `build_loop_exhausted_fallback_uses_latest_tool_result_when_present`
    - `build_loop_exhausted_fallback_handles_missing_tool_result`
- 2026-03-01: Added root-cause diagnostics for T14-03 and improved search robustness.
  - Root cause from logs: `web_search` frequently returned `"No results found."` for the query, causing the model to repeatedly call the same tool and never converge to final content.
  - Added runtime diagnostics:
    - `run_agent_turn` now logs step index, tool-call count, tool name, and result length.
    - `web_search` now logs query, SDK result count, and fallback result count.
  - Added `web_search` fallback behavior:
    - If `duckduckgo_search` SDK returns empty, perform HTML fallback search via DuckDuckGo HTML endpoint and parse result titles/snippets/URLs.

