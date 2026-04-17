# Worklog

Plan doc: [SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md](/Users/aelaguiz/workspace/codex/docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md)

## Initial entry
- Run started.
- Current phase: Phase 1 — Add the canonical live model-update owner path.
- Focus: land the post-start core event and shared apply-and-emit path before exposing new tool or app-server surfaces.

## Implementation progress
- Phases 1 through 5 landed in code:
  - shared core apply-and-emit helper over `SessionSettingsUpdate`
  - persisted `SessionModelUpdated`
  - model-visible built-in tools `list_available_models` and `update_session_model`
  - app-server `thread/model/set` and `thread/model/updated`
  - TUI and MCP consumers updated to the post-start model event
- Generated app-server protocol artifacts were refreshed with `just write-app-server-schema`.
- Added implementation-focused tests for:
  - tool schemas and tool handlers
  - state extraction from `SessionModelUpdated`
  - TUI live-session refresh paths
  - app-server `thread/model/set` notification, persistence, next-turn request payload, provider invariance, and ChatGPT-authenticated provider flow

## Verification progress
- `cargo test -p codex-tui` passed after the live-update UI changes landed.
- A broader backend sweep was started for:
  - `codex-tools`
  - `codex-rollout`
  - `codex-state`
  - `codex-core`
  - `codex-app-server-protocol`
  - `codex-app-server`
  - `codex-mcp-server`
- The first broad run exposed only stale app-server schema fixtures; that was repaired by regenerating the schema before rerunning the backend sweep.
