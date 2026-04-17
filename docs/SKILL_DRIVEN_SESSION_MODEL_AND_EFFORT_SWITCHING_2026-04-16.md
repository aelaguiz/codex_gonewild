---
title: "Codex - Skill-Driven Session Model and Effort Switching - Architecture Plan"
date: 2026-04-16
status: active
fallback_policy: forbidden
owners: [aelaguiz]
reviewers: []
doc_type: architectural_change
related:
  - docs/config.md
  - docs/skills.md
  - codex-rs/app-server/README.md
  - codex-rs/core/src/codex.rs
  - codex-rs/protocol/src/protocol.rs
  - docs/examples/skills/session-model-control-example/SKILL.md
---

# TL;DR

Outcome

To make it possible for a skill running inside an existing Codex session to change the parent thread's current model and reasoning effort, with the live runtime, persisted thread metadata, and all surfaced UI/API views staying in sync without introducing a second settings system.

Problem

Codex already supports model and reasoning changes through user-facing control surfaces like `turn/start` overrides and the TUI's `/model` flow, and the core runtime already has a live mutation primitive via `Op::OverrideTurnContext` and `Session::update_settings()`. But that mutation path is not available to the agent/tool layer, so skills can recommend a model switch but cannot execute one on the parent thread. If we add a new skill-driven settings path carelessly, we risk creating parallel sources of truth across core session state, thread metadata, TUI session header/status lines, app-server notifications, MCP event output, and SDK/client control surfaces.

Approach

Reuse the existing live-session settings override path as the canonical owner. Skills themselves remain instruction text, not executable host code, so the runtime must expose model-visible built-in tools that the model can see in its tool list and call on the skill's behalf. For this workflow, that means a read tool for the live model catalog plus a narrow write tool for parent-thread session settings mutation, both backed by existing runtime truth. Route the write path through the same `SessionSettingsUpdate` / `OverrideTurnContext` machinery the TUI already uses, emit a first-class runtime change event that existing clients can render, and expose the same capability explicitly across app-server, SDK, and TUI so skill-driven changes and user-driven changes share one behavior contract.

Plan

1. Research and deep-dive the existing settings owner path, thread persistence, and rendering surfaces.
2. Add model-visible built-in tools for this workflow: `list_available_models` for catalog reads and `update_session_model` for root-thread writes, plus matching `thread/model/set` and `thread/model/updated` protocol coverage on the non-agent client side. Follow-on branch work later added `get_current_session_model` as the dedicated explicit current-state read tool while preserving the existing `list_available_models` shape for compatibility.
3. Wire TUI, app-server, SDK, and MCP-facing surfaces to the same operation and add tests proving live updates stay visible and persisted correctly.

Non-negotiables

- Reuse `Op::OverrideTurnContext` / `SessionSettingsUpdate` as the canonical mutation path unless research proves it cannot safely own this.
- No parallel settings store, no skill-only shadow state, and no "tool changed it but UI catches up later" drift.
- Model and reasoning changes must be reflected in live session UI, persisted thread metadata, history/session headers, status displays, and exposed protocol/control surfaces.
- Skills may change only allowed runtime settings; they must not silently bypass model capability validation or role locks.
- Fail loud on invalid model or effort requests; do not auto-normalize an incompatible model/effort pair when the caller omitted effort.

<!-- arch_skill:block:planning_passes:start -->
<!--
arch_skill:planning_passes
deep_dive_pass_1: done 2026-04-16
external_research_grounding: not started
deep_dive_pass_2: done 2026-04-16
recommended_flow: deep dive -> external research grounding -> deep dive again -> phase plan -> implement
note: This block tracks stage order only. It never overrides readiness blockers caused by unresolved decisions.
-->
<!-- arch_skill:block:planning_passes:end -->

# 0) Holistic North Star

## 0.1 The claim (falsifiable)

If Codex exposes the existing live session override machinery to the agent/tool layer and broadcasts resulting settings changes as first-class runtime events, then a skill can switch the current parent thread's model and reasoning effort between turns and every supported control surface will show the same new values without duplicate state or manual refresh hacks.

## 0.2 In scope

- Adding a skill-callable internal mechanism to change parent-thread session settings for model and reasoning effort.
- Exposing model-visible built-in tools so a skill can tell the model when to read valid model/effort choices and when to mutate the root session.
- Reusing the core session settings update path (`SessionSettingsUpdate`, `Session::update_settings`, `Op::OverrideTurnContext`) rather than inventing a second path.
- Defining the confirmed live-update semantic for in-flight work: a skill-driven model or effort change updates the parent thread's current defaults for the next model request / subsequent turn and does not retarget the already-issued in-flight request.
- Ensuring existing UI and control surfaces reflect these changes neatly:
  - TUI session header, history, status, footer, and model-selection surfaces.
  - app-server notifications, RPCs, and persisted thread metadata.
  - MCP-facing session event output where current settings are surfaced.
  - SDK/client surfaces that inspect or resume threads.
- Defining validation and constraints for invalid requested model or effort combinations and role-locked settings.
- Minimal adjacent-surface updates needed to keep docs and generated protocol/client artifacts honest.

## 0.3 Out of scope

- New user-facing product capability beyond skill-driven model and reasoning switching on the current thread.
- General arbitrary skill-driven mutation of all session settings unless later research proves model and effort cannot be isolated cleanly.
- Mid-thread model-provider switching. The current thread keeps its active provider; changing providers remains a thread start/resume/fork concern.
- New collaboration modes, profiles, or config file semantics.
- Silent auto-selection heuristics for "best model".
- Changing safety reroute policy itself beyond making surfaced state truthful when reroutes happen.

## 0.4 Definition of done (acceptance evidence)

- A skill/tool call can update the live parent thread model and reasoning effort without creating a new thread.
- A skill can list the current session's valid model and reasoning-effort choices through a model-visible built-in read tool backed by the live catalog rather than guessing from memory.
- The next turn uses the new settings, and the current thread's persisted metadata reflects them.
- TUI shows the changed settings in the same places it already shows user-driven model and effort changes.
- App-server exposes the capability and/or resulting state changes on all relevant surfaces, and SDK/generated types stay aligned.
- MCP/session event output does not lie about current session settings after a skill-driven change.
- Invalid or unsupported changes fail loudly with a user-visible explanation.
- Tests cover live update behavior, persistence, protocol serialization, and UI snapshots where user-visible output changes.

## 0.5 Key invariants (fix immediately if violated)

- No new parallel settings path for live model or reasoning mutation.
- No silent behavior drift between user-driven and skill-driven settings changes.
- No stale UI or protocol surface after a live settings change.
- No bypass of model capability validation, role locks, or existing safety reroute behavior.
- No runtime fallback shim that hides a rejected model or effort request.

# 1) Key Design Considerations (what matters most)

## 1.1 Priorities (ranked)

1. Keep one single owner path for live session settings mutation.
2. Keep all user-visible and machine-visible surfaces truthful immediately after mutation.
3. Make the fork minimal by reusing existing runtime and protocol machinery.
4. Reject unsupported or locked changes loudly and consistently.

## 1.2 Constraints

- The current live-session mutation path already exists via `Op::OverrideTurnContext` and `Session::update_settings()`.
- `turn/steer` does not currently accept model or effort changes, so hot-swapping an in-flight model request is not current behavior.
- TUI currently disables `/model` while a task is in progress, so "at any point" semantics are not yet exposed to users.
- Thread metadata and resume behavior already persist and reuse latest observed model and reasoning state.
- The live thread provider is not a current post-start mutation surface; app-server request handling still treats provider as part of the thread config contract rather than a live per-turn override path.

## 1.3 Architectural principles (rules we will enforce)

- Reuse `SessionSettingsUpdate` and `OverrideTurnContext` as the only live mutation path.
- Additive agent capability is acceptable; a second config or profile layer is not.
- Evented truth beats client polling or UI heuristics.
- Validation must go through the same model-catalog and role-lock checks used by existing model selection paths.
- Prefer the repo's existing "model" contract family, which already bundles reasoning effort in `/model`, `PersistModelSelection`, and related UI/protocol surfaces, over inventing a new generic session-settings product concept.

## 1.4 Known tradeoffs (explicit)

- This fork will treat a mid-turn skill change as updating live defaults for subsequent requests rather than retargeting an already-issued model call.
- A dedicated event or RPC adds protocol surface area, but it keeps clients aligned and avoids ambiguous inference from generic history changes.
- Giving skills parent-thread settings authority increases power; scope must remain narrowly bounded to model and reasoning in the first cut.

# 2) Problem Statement (existing architecture + why change)

## 2.1 What exists today

- Core session state stores model and reasoning inside `SessionConfiguration.collaboration_mode` and applies updates via `Session::update_settings()` in `codex-rs/core/src/codex.rs`.
- `Op::OverrideTurnContext` already exists in `codex-rs/protocol/src/protocol.rs` and is used by TUI and app-server flows to update current thread defaults.
- `turn/start` in app-server first submits `Op::OverrideTurnContext` when overrides are present, then starts user input in `codex-rs/app-server/src/codex_message_processor.rs`.
- Thread metadata persists latest model and reasoning in `codex-rs/state/src/model/thread_metadata.rs`.
- TUI renders model and reasoning in session header/history/status surfaces in `codex-rs/tui/src/history_cell.rs` and `codex-rs/tui/src/status/helpers.rs`.

## 2.2 What's broken / missing (concrete)

- Skills have no parent-thread settings mutation tool, so they cannot actually switch the running session's model or reasoning themselves.
- Skills also have no model-visible catalog-read tool today, so they cannot honestly enumerate valid models and supported reasoning efforts on demand from runtime truth.
- No standalone app-server RPC or SDK surface exposes live settings mutation apart from piggybacking on `turn/start`.
- There is no first-class "session settings changed by tool/skill" event distinct from initial `session_configured` or model safety reroute notifications.
- Current user-facing `/model` UX is intentionally disabled while a task runs, so "at any point" semantics are unresolved for active turns.

## 2.3 Constraints implied by the problem

- We must preserve single-source-of-truth behavior between live session settings, persisted metadata, and rendered UI.
- We must not bypass provider/model capability checks or role locks.
- We must preserve the confirmed active-turn semantic consistently across tool output, UI, and protocol surfaces: the current in-flight request keeps its original model settings, and the updated defaults apply to the next request.

<!-- arch_skill:block:research_grounding:start -->
# 3) Research Grounding (external + internal "ground truth")

## 3.1 External anchors (papers, systems, prior art)

- None required currently. This is still a repo-internal control-surface convergence problem, and the first credible move is to reuse Codex's existing owner path rather than import a new runtime-control pattern.

## 3.2 Internal ground truth (code as spec)

- Authoritative behavior anchors (do not reinvent):
  - `codex-rs/core/src/codex.rs` — `SessionSettingsUpdate`, `Session::update_settings()`, and `override_turn_context()` already own live session mutation and already reject invalid updates loudly.
  - `codex-rs/protocol/src/protocol.rs` — `Op::OverrideTurnContext`, `SessionConfiguredEvent`, skill-instruction prompt tags, and tool/event envelopes define the existing session-control and skill/runtime contract.
  - `codex-rs/protocol/src/openai_models.rs` — model catalog and supported reasoning-effort data already define which model/effort combinations are valid.
  - `codex-rs/app-server-protocol/src/protocol/v2.rs` — `turn/start` already treats `model` and `effort` as overrides for this turn and subsequent turns.
  - `codex-rs/state/src/extract.rs` and `codex-rs/state/src/model/thread_metadata.rs` — persisted thread metadata already derives model and reasoning truth from turn-context/session rollout items.
- Canonical path / owner to reuse:
  - `codex-rs/core/src/codex.rs` — the only acceptable live mutation owner is the existing `SessionSettingsUpdate` -> `Session::update_settings()` -> `Op::OverrideTurnContext` path.
- Adjacent surfaces tied to the same contract family:
  - `codex-rs/app-server/src/codex_message_processor.rs` — app-server already translates `turn/start` overrides into `Op::OverrideTurnContext`; a skill-driven path must not fork that behavior.
  - `codex-rs/tui/src/chatwidget.rs`, `codex-rs/tui/src/app_event.rs`, `codex-rs/tui/src/history_cell.rs`, and `codex-rs/tui/src/status/helpers.rs` — TUI already has model/reasoning update, persistence, and rendering paths that must stay in sync with any new skill-driven mutation source.
  - `codex-rs/mcp-server/src/outgoing_message.rs` and `codex-rs/app-server/src/outgoing_message.rs` — MCP and JSON-RPC notification bridges already serialize session events, so post-start settings truth must be broadcast cleanly instead of relying on stale initial configuration.
  - `codex-rs/app-server-protocol/src/protocol/thread_history.rs` and `codex-rs/app-server/src/codex_message_processor.rs` resume/merge code — persisted thread reads and resumes already surface model/reasoning state and must reflect the same live truth.
- Compatibility posture (separate from `fallback_policy`):
  - Preserve the existing session-settings contract — skill-driven model/effort mutation should be an additive caller into the same owner path, not a clean-cut replacement or bridge that changes how user-driven updates behave.
- Existing patterns to reuse:
  - `codex-rs/app-server/src/codex_message_processor.rs` — the existing "apply overrides first, then start the turn" pattern should be reused for any new RPC or tool entrypoint.
  - `codex-rs/tui/src/chatwidget.rs` and `codex-rs/tui/src/app_event.rs` — existing `/model` selection and `AppCommand::override_turn_context(...)` flows already show how UI surfaces update current model and reasoning in memory and persist selection.
  - `codex-rs/core/tests/suite/model_switching.rs` — existing tests already prove override-driven model changes affect the next request, which is the same semantic the user approved for skill-driven updates.
- Prompt surfaces / agent contract to reuse:
  - `codex-rs/protocol/src/protocol.rs` — `<skills_instructions>` and skills list plumbing show that skill behavior is already a first-class prompt surface in the same parent session.
  - `codex-rs/core/src/codex.rs` — `TurnContext` already carries `developer_instructions`, `user_instructions`, `turn_skills`, `tools_config`, and `dynamic_tools`, so the missing capability is a bounded parent-thread mutation tool, not a second agent session model.
- Native model or agent capabilities to lean on:
  - `codex-rs/core/src/tools/router.rs` plus the existing tool registry/handler stack — the runtime already supports exposing bounded built-in tools to the agent, so this can be implemented as one narrow tool rather than a parallel plugin-only or out-of-band control plane.
- Existing grounding / tool / file exposure:
  - `codex-rs/protocol/src/protocol.rs` function/MCP/dynamic tool events and `codex-rs/core/src/tools/router.rs` model-visible tool specs — the parent agent already has a normal tool-calling path that can surface this capability without changing how skills are loaded.
  - `codex-rs/protocol/src/protocol.rs` skill tags and skill listing — skills already execute inside the same thread context and do not need a child thread or external supervisor to request the change.
- Duplicate or drifting paths relevant to this change:
  - `SessionConfiguredEvent`, `model/rerouted`, persisted thread metadata, and TUI local selection/persistence paths all mention model/reasoning state today; without a first-class post-start update path, a skill-driven mutation would create yet another truth surface and drift risk.
  - User-driven updates already arrive through both TUI `/model` and app-server `turn/start` overrides; the change must converge skill-driven updates onto the same owner path rather than create a fourth route.
- Capability-first opportunities before new tooling:
  - Reuse `Op::OverrideTurnContext` directly from a narrow built-in tool or RPC before considering any new session-settings store.
  - Reuse the existing tool router/handler infrastructure before inventing a skill-only transport or hidden settings sidecar.
  - Reuse existing session-configured/update notification patterns before adding transport-specific UI polling or manual refresh logic.
- Behavior-preservation signals already available:
  - `codex-rs/core/tests/suite/model_switching.rs` — protects next-turn model-switch semantics on the core runtime path.
  - `codex-rs/state/src/extract.rs` tests — protect metadata extraction for model and reasoning from persisted turn context.
  - Existing TUI snapshot/status tests under `codex-rs/tui/src/chatwidget/tests` and `codex-rs/tui/src/history_cell.rs` provide the natural coverage points once UI-visible text changes.

## 3.3 Decision gaps that must be resolved before implementation

- None currently. The active-turn semantic is confirmed: a skill may update live parent-thread defaults while work is in flight, but the already-issued in-flight request keeps its original model and reasoning settings.
<!-- arch_skill:block:research_grounding:end -->

<!-- arch_skill:block:current_architecture:start -->
# 4) Current Architecture (as-is)

## 4.1 On-disk structure

- Core live owner path:
  - `codex-rs/core/src/codex.rs` owns `SessionConfiguration`, `SessionSettingsUpdate`, `Session::update_settings()`, and `override_turn_context()`.
  - `codex-rs/protocol/src/protocol.rs` defines `Op::OverrideTurnContext`, `SessionConfiguredEvent`, and the event envelope consumed across clients.
- Tool exposure path:
  - `codex-rs/tools/src/tool_registry_plan.rs` and `codex-rs/tools/src/tool_registry_plan_types.rs` define which built-in tools exist and which handler kind each tool maps to.
  - `codex-rs/core/src/tools/spec.rs`, `codex-rs/core/src/tools/router.rs`, and `codex-rs/core/src/tools/handlers/*` realize that plan in the core runtime.
- App-server and remote-client path:
  - `codex-rs/app-server/src/codex_message_processor.rs` translates RPCs into core ops and builds `ThreadStartResponse`, `ThreadResumeResponse`, and `ThreadReadResponse`.
  - `codex-rs/app-server/src/bespoke_event_handling.rs` and `codex-rs/app-server/src/outgoing_message.rs` translate selected core events into v2 notifications such as `model/rerouted`.
  - `codex-rs/app-server-protocol/src/protocol/common.rs` and `codex-rs/app-server-protocol/src/protocol/v2.rs` define the public RPC and notification schema consumed by SDKs and remote TUI.
- Persistence and thread summary path:
  - `codex-rs/state/src/extract.rs` updates `ThreadMetadata` from persisted rollout items.
  - `codex-rs/state/src/model/thread_metadata.rs` and `codex-rs/state/src/runtime/threads.rs` persist model and reasoning fields used by thread list/read/resume surfaces.
- UI and event-consumer path:
  - `codex-rs/tui/src/chatwidget.rs`, `codex-rs/tui/src/app_event.rs`, `codex-rs/tui/src/history_cell.rs`, `codex-rs/tui/src/status/helpers.rs`, and `codex-rs/tui/src/app_server_session.rs` render and cache current model/reasoning state.
  - `codex-rs/mcp-server/src/outgoing_message.rs` and `codex-rs/mcp-server/src/codex_tool_runner.rs` bridge core events into MCP notifications and tool-runner behavior.

## 4.2 Control paths (runtime)

1. Session startup path
   - `ThreadManager::finalize_thread_spawn()` in `codex-rs/core/src/thread_manager.rs` requires the first event for a new thread to be `EventMsg::SessionConfigured`.
   - TUI `on_session_configured()` in `codex-rs/tui/src/chatwidget.rs` synchronizes the current model/reasoning state, header, history metadata, and status surfaces from that startup-only event.

2. User-driven settings update path
   - Embedded TUI manual changes eventually submit `Op::OverrideTurnContext`.
   - App-server `turn/start` in `codex-rs/app-server/src/codex_message_processor.rs` detects override fields and submits `Op::OverrideTurnContext` before `Op::UserInput`.
   - `override_turn_context()` in `codex-rs/core/src/codex.rs` calls `Session::update_settings()` and stops there on success; it does not currently emit a dedicated post-start settings-change event.

3. Tool availability path
- The parent agent can only do what `build_tool_registry_plan()` exposes through `codex-rs/tools/src/tool_registry_plan.rs`.
- Existing root-thread-only control tools such as `request_user_input` use handler-level session-source checks in `codex-rs/core/src/tools/handlers/request_user_input.rs`.
- No built-in tool currently exists for parent-thread session model or reasoning mutation.
- No built-in tool currently exists for exposing the current live model catalog to the model, even though the host runtime already has catalog truth through `ListModels` and app-server `model/list`.

4. Persistence and read-model path
   - `codex-rs/state/src/extract.rs` updates persisted `ThreadMetadata` model and reasoning only from `RolloutItem::TurnContext` today.
   - `codex-rs/state/src/runtime/threads.rs` stores those extracted values in the `threads.model` and `threads.reasoning_effort` columns used by list/read style queries.
   - App-server `ThreadResumeResponse`, `ThreadForkResponse`, and remote TUI `ThreadSessionState` surface model and reasoning from startup/resume responses and persisted metadata, not from a standalone live override record.

5. Special-case event path
   - The only dedicated post-start model-related event wired through app-server today is `EventMsg::ModelReroute` -> `model/rerouted`.
   - Intentional live settings changes currently have no equivalent first-class event.

6. Provider contract boundary
   - `Op::OverrideTurnContext` carries `model` and `effort`, but not `model_provider`.
   - App-server mismatch handling still treats `model_provider` as part of the thread config snapshot contract, not a live post-start mutation surface.
   - Current post-start work therefore lives inside one provider; cross-provider switching is not the minimal fork path for this feature.

## 4.3 Object model + ownership boundaries

- `SessionSettingsUpdate` is the canonical mutation payload for live session defaults.
- `CollaborationMode.settings.model` and `CollaborationMode.settings.reasoning_effort` are the live owner fields inside session configuration.
- `SessionConfiguredEvent` is a startup contract, not a general-purpose live settings sync event.
- `ThreadMetadata` is the persisted read model for thread list/read/resume style surfaces.
- Tool specs live in `codex-rs/tools`, while handler behavior and root-thread restrictions live in `codex-rs/core`.

## 4.4 Observability + failure behavior today

- `Session::update_settings()` returns `ConstraintResult<()>` and rejects invalid updates loudly.
- `override_turn_context()` emits an `ErrorEvent` on rejected updates, but emits no success event beyond the next turn eventually observing the new defaults.
- `codex-rs/mcp-server/src/codex_tool_runner.rs` treats post-start `SessionConfigured` as unexpected.
- `ThreadManager::finalize_thread_spawn()` treats any non-`SessionConfigured` first event as an error.
- Result: reusing `SessionConfiguredEvent` for live settings mutation would break existing startup assumptions, while silent mutation causes remote/UI/read-model drift until another turn persists new `TurnContextItem` data.

## 4.5 UI surfaces (ASCII mockups, if UI work)

Current:

```text
Startup:
  session configured -> header/status/history all sync to model + reasoning

After manual /model:
  local UI updates immediately
  next turn uses new defaults

After hypothetical skill-driven change today:
  no root-thread tool exists
  no dedicated live event exists
  persisted thread summary would lag until a later turn
```
<!-- arch_skill:block:current_architecture:end -->

<!-- arch_skill:block:target_architecture:start -->
# 5) Target Architecture (to-be)

## 5.1 Chosen architecture

Adopt one bounded root-thread session-settings mutation feature that every control surface shares:

- A new built-in function tool named `list_available_models`, exposed to the parent agent and therefore to skills, for reading the live model catalog that the current session is actually allowed to use.
- A new built-in function tool named `update_session_model`, exposed to the parent agent and therefore to skills, for updating session `model` and `reasoning_effort`.
- A matching dedicated v2 app-server RPC `thread/model/set` for non-agent clients that need the same capability without piggybacking on `turn/start`.
- A new persisted post-start core event `SessionModelUpdated`, plus corresponding app-server notification `thread/model/updated`, instead of reusing startup-only `SessionConfiguredEvent`.

This is additive exposure of existing runtime truth, not a second settings system: skills remain instructions, the model sees built-in tools, and those tools land on existing catalog and session-owner paths.

## 5.2 On-disk structure (future)

- Tool spec and registry:
  - Add new tool definitions in `codex-rs/tools/src/` for `list_available_models` and `update_session_model`, and register their handler kinds in `tool_registry_plan.rs` / `tool_registry_plan_types.rs`.
  - Wire both handlers in `codex-rs/core/src/tools/spec.rs` and `codex-rs/core/src/tools/handlers/mod.rs`.
- Core runtime:
  - Extend `codex-rs/protocol/src/protocol.rs` with a new post-start `SessionModelUpdated` event and source enum.
  - Add a central helper in `codex-rs/core/src/codex.rs` that wraps `Session::update_settings()`, computes old/new model snapshots, and emits the new event on success.
- Persistence:
  - Extend `codex-rs/state/src/extract.rs` and related state/runtime code so the new persisted event updates thread metadata immediately, without waiting for the next `TurnContextItem`.
- App-server and SDK:
  - Add stable `thread/model/set` and `thread/model/updated` surfaces in `codex-rs/app-server-protocol/src/protocol/v2.rs` / `common.rs`, update `codex-rs/app-server/src/codex_message_processor.rs` / `bespoke_event_handling.rs`, regenerate schema, and keep SDK clients aligned.
- UI and MCP:
  - Teach TUI and MCP consumers to handle the new event directly rather than trying to reinterpret startup configuration or model-reroute events.

## 5.3 Control paths (future)

1. Skill / tool path
   - Skills do not call Rust methods or app-server RPCs directly. They are prompt instructions that tell the model when to use model-visible built-in tools.
   - For this workflow, the model must see `get_current_session_model`, `list_available_models`, and `update_session_model` in its tool list.
   - A skill calls `get_current_session_model` when it needs the current root session's active model and reasoning effort.
   - A skill calls `list_available_models` when it needs runtime truth about valid models and supported reasoning efforts for the current session.
   - A skill calls `update_session_model` with at least one of `model` or `reasoning_effort` when it needs to change the root session.
   - `get_current_session_model` is the narrow explicit current-state surface for skills.
   - `list_available_models` is a read-only built-in wrapper over the same live catalog truth already surfaced to external clients through `model/list`; it is not a second catalog.
   - `list_available_models` returns the current session's visible models, each model's default reasoning effort, and each model's supported reasoning efforts, and it preserves its current-state fields for compatibility with the earlier branch implementation.
   - The handler rejects subagents and non-root sessions, reuses existing parsing/validation patterns, and forwards a `SessionSettingsUpdate` into the canonical core owner path.
   - The tool returns structured success output containing the applied `model`, applied `reasoning_effort`, and an explicit flag that the current in-flight turn, if any, keeps its previous model and reasoning settings.

2. Non-agent client path
   - App-server exposes the same capability via `thread/model/set` so SDKs and remote control surfaces do not have to synthesize dummy turns just to mutate session defaults.
   - `thread/model/set` accepts `thread_id`, `model: Option<String>`, and `reasoning_effort: Option<Option<ReasoningEffort>>`, mirroring the core "set / clear / preserve" semantics already used by `Op::OverrideTurnContext`.
   - The RPC response returns the applied `model`, applied `reasoning_effort`, and whether the current in-flight turn keeps its prior model and reasoning settings.

3. Core owner path
   - The new helper wraps `Session::update_settings()` rather than bypassing it.
   - On success, it emits one `EventMsg::SessionModelUpdated` containing:
     - previous and current model / reasoning values
     - change source (`tool`, `app_server`, `manual_tui`, or equivalent)
     - whether an in-flight turn remains on the old model and reasoning settings
   - `Op::OverrideTurnContext` is updated to use this helper when model or effort changed so manual/user-driven changes and skill-driven changes share the same success event, while unrelated session-setting overrides keep their existing behavior.

4. Persistence and read-model path
   - `SessionModelUpdated` is persisted as a normal `EventMsg` rollout item and becomes a first-class metadata extraction input; no new parallel rollout item type is introduced.
   - `codex-rs/state/src/extract.rs` applies the new event to `ThreadMetadata.model` and `ThreadMetadata.reasoning_effort`.
   - `thread/list`, `thread/read`, `thread/resume`, `thread/fork`, and remote TUI `ThreadSessionState` therefore stay truthful immediately after a live change instead of waiting for a later turn.

5. UI / MCP / notification path
   - Embedded TUI listens for `SessionModelUpdated`, updates current collaboration state from that event, and renders a compact history/status update when the source is a skill/tool.
   - App-server maps the core event to stable `thread/model/updated` so remote TUI and SDK clients can update live session state.
   - MCP forwards the new event as a normal notification and `codex_tool_runner` treats it as expected, rather than logging an unexpected `SessionConfigured`.

## 5.4 Contracts and boundaries

- Canonical owner path:
  - `SessionSettingsUpdate` -> central apply-and-emit helper in `codex-rs/core/src/codex.rs` -> `Session::update_settings()` -> `SessionModelUpdated`.
- Root-thread boundary:
  - Skills themselves never invoke host-side Rust methods or app-server RPCs directly; they instruct the model to use built-in tools that the runtime exposed in the prompt for that turn.
  - `get_current_session_model`, `list_available_models`, and `update_session_model` are the model-visible built-in tools for this skill workflow.
  - `get_current_session_model` is the explicit current-state read tool; `list_available_models` remains the broad catalog tool and keeps its current-state fields for compatibility.
  - `update_session_model` is root-thread-only, following the same boundary style as `request_user_input`.
  - Subagents keep their own model/reasoning controls through their existing spawn configuration and must not mutate the parent thread directly through this tool.
- Validation boundary:
  - `list_available_models` reads from the existing live model catalog truth; it must not synthesize or cache a second skill-private model list.
  - Model/effort validity continues to come from the existing model catalog and current collaboration-mode constraints.
  - If the caller changes model without explicitly providing effort, the runtime preserves the current reasoning setting only when the resulting model/effort pair is still valid.
  - If the caller changes model without explicitly providing effort and the current reasoning setting is not valid for the new model, the mutation fails loudly and instructs the caller to provide a compatible `reasoning_effort` or explicitly clear it. No model-compatible auto-fallback/default is approved for this feature.
  - Neither the tool nor `thread/model/set` accepts `model_provider`; provider switching remains outside this feature and continues to belong to thread start/resume/fork configuration.
- Compatibility posture:
  - Preserve existing `turn/start` override behavior and `/model` behavior; add a shared post-start success event and dedicated model-focused mutation surfaces rather than changing startup semantics.
- No-parallel-path stance:
  - No shadow config write, no skill-private state, no polling-based UI refresh, and no reuse of `SessionConfiguredEvent` for post-start changes.
  - No new generic "session settings" public product surface in the first cut; stay aligned with the repo's existing model-plus-reasoning contract family.

## 5.5 Invariants and UI behavior

- The active in-flight request keeps its original model and reasoning settings.
- The next request / next turn uses the updated session defaults.
- Every post-start model mutation, regardless of source, emits the same durable event and updates the same persisted read model.
- Skill-driven updates render as a user-visible but compact state change, for example:

```text
Settings updated by skill:
gpt-5.4 high -> gpt-5.2-codex low

Current turn:
still running on gpt-5.4 high

Next turn:
will use gpt-5.2-codex low
```
<!-- arch_skill:block:target_architecture:end -->

<!-- arch_skill:block:call_site_audit:start -->
# 6) Call-Site Audit (exhaustive change inventory)

## Change map (table)
| Area | File | Symbol / Call site | Current behavior | Required change | Why | New API / contract | Tests impacted |
| ---- | ---- | ------------------ | ---------------- | --------------- | --- | ------------------ | -------------- |
| Tool spec | `codex-rs/tools/src/tool_registry_plan.rs`, `codex-rs/tools/src/tool_registry_plan_types.rs` | built-in tool registration plan + handler kind enum | No parent-thread model-update tool exists | Register `update_session_model` and one matching handler kind | Skills can only call what the tool plan exposes | New root-thread model-focused tool contract | `codex-rs/tools/src/tool_registry_plan_tests.rs` |
| Tool spec | `codex-rs/tools/src/tool_registry_plan.rs`, `codex-rs/tools/src/tool_registry_plan_types.rs` | built-in tool registration plan + handler kind enum | No model-visible catalog-read tool exists | Register `list_available_models` and one matching handler kind | Skills can only read valid model/effort choices from tools the runtime exposes | New read-only model-catalog tool contract | `codex-rs/tools/src/tool_registry_plan_tests.rs` |
| Tool spec schema | `codex-rs/tools/src/lib.rs`, new file under `codex-rs/tools/src/` | tool constant + `ResponsesApiTool` definition | No reusable tool schema for this capability | Add `update_session_model`, its JSON schema, description, and exports | Keep the tool contract defined once and aligned with `/model` semantics | New built-in function tool schema with `model` and `reasoning_effort` only | Tool-spec/unit tests in `codex-rs/tools/src/*_tests.rs` |
| Tool spec schema | `codex-rs/tools/src/lib.rs`, new file under `codex-rs/tools/src/` | tool constant + `ResponsesApiTool` definition | No reusable tool schema for model-catalog reads | Add `list_available_models`, its JSON schema, description, and exports | The skill must be able to read valid choices from runtime truth, not memory | New built-in function tool schema for visible models and supported efforts | Tool-spec/unit tests in `codex-rs/tools/src/*_tests.rs` |
| Tool handler | `codex-rs/core/src/tools/handlers/mod.rs`, new handler file | handler registration and catalog-read execution | No handler exposes the live model catalog to the model | Add read-only handler that lands on the existing model catalog truth | Skills need a truthful in-band way to enumerate valid choices | Model-visible read tool backed by existing catalog/list path | New handler tests plus targeted core tool tests |
| Tool handler | `codex-rs/core/src/tools/handlers/mod.rs`, new handler file | handler registration and root-thread validation | No handler can mutate root-thread session settings | Add handler that parses args, rejects subagents, and calls shared core helper | Bound the feature to the parent thread and reuse existing handler patterns | Root-thread-only handler with loud validation errors | New handler tests plus targeted core tool tests |
| Core owner path | `codex-rs/core/src/codex.rs` | `Session::update_settings()`, `override_turn_context()` | Live settings update mutates state but does not emit a success event | Add central apply-and-emit helper and route model/effort overrides through it | Make manual and skill-driven model changes share one success path | `SessionModelUpdated` core event emitted after successful mutation | `codex-rs/core/tests/suite/model_switching.rs` and adjacent core tests |
| Core protocol event | `codex-rs/protocol/src/protocol.rs` | event enum / payload definitions | `SessionConfigured` is startup-only; `ModelReroute` is the only post-start model event | Add `SessionModelUpdated` event + source enum | Reusing `SessionConfigured` would break startup-only consumers | New persisted post-start model event carrying old/new values and source | Protocol serialization tests |
| Persistence | `codex-rs/state/src/extract.rs`, `codex-rs/state/src/model/thread_metadata.rs`, `codex-rs/state/src/runtime/threads.rs` | metadata extraction and SQLite upsert | Model/reasoning persist from `TurnContextItem` only | Teach extraction/runtime metadata to apply `SessionModelUpdated` immediately | `thread/list`, `thread/read`, and resume/fork surfaces otherwise lag until the next turn | Persisted thread metadata updates on live settings change | State extraction/runtime tests |
| App-server RPC | `codex-rs/app-server-protocol/src/protocol/v2.rs`, `codex-rs/app-server-protocol/src/protocol/common.rs`, `codex-rs/app-server/src/codex_message_processor.rs` | thread-scoped control surface | No dedicated RPC exists; clients must piggyback on `turn/start` | Add stable `thread/model/set` with thread-scoped model-plus-reasoning params | All control surfaces need the capability without synthesizing a user turn | Stable v2 RPC for live model/reasoning mutation | `cargo test -p codex-app-server-protocol`, app-server request/response tests |
| Existing app-server catalog surface | `codex-rs/app-server-protocol/src/protocol/v2.rs`, `codex-rs/app-server/src/codex_message_processor.rs` | `model/list` | Existing client-facing model catalog surface already returns valid models and supported reasoning efforts | Keep `model/list` as the external catalog truth and back `list_available_models` from the same source of truth | The skill-visible read tool must not invent a second catalog contract | No new client RPC needed for reads; reuse existing catalog truth | Existing `model/list` tests plus new read-tool tests |
| App-server notifications | `codex-rs/app-server/src/bespoke_event_handling.rs`, `codex-rs/app-server/src/outgoing_message.rs`, protocol common/v2 | event-to-notification bridge | Only `model/rerouted` exists for post-start model change | Map `SessionModelUpdated` to stable `thread/model/updated` | Remote TUI / SDK clients need live updates, not just persisted snapshots | Dedicated notification for post-start model updates | App-server notification serialization/integration tests |
| Remote TUI session state | `codex-rs/tui/src/app_server_session.rs`, `codex-rs/tui/src/app.rs`, `codex-rs/tui/src/app/app_server_adapter.rs` | `ThreadSessionState` hydration and live sync | Session state comes from start/resume/read responses and local assumptions | Update live session state from the new notification/event | Remote TUI must stay in sync without refresh hacks | Thread session state patched on live notification | TUI app/app-server adapter tests |
| Embedded TUI UI | `codex-rs/tui/src/chatwidget.rs`, `codex-rs/tui/src/app_event.rs`, `codex-rs/tui/src/history_cell.rs`, `codex-rs/tui/src/status/helpers.rs` | header/history/status rendering and local update commands | Startup sync is tied to `SessionConfigured`; skill-driven post-start changes have no event to consume | Consume `SessionModelUpdated`, update state, and render a neat skill-change line | User-visible surfaces must remain truthful and non-contradictory | One shared post-start model-update display path | TUI behavior tests and snapshot tests |
| MCP bridge | `codex-rs/mcp-server/src/outgoing_message.rs`, `codex-rs/mcp-server/src/codex_tool_runner.rs` | event forwarding and tool-runner event handling | `SessionConfigured` after startup is logged as unexpected | Forward/handle `SessionModelUpdated` as the valid post-start signal | MCP surfaces must stay truthful without breaking startup assumptions | New MCP notification/event handling path | MCP outgoing-message tests |
| Provider boundary | `codex-rs/core/src/codex.rs`, `codex-rs/app-server/src/codex_message_processor.rs`, `codex-rs/tui/src/app_server_session.rs` | model/provider thread config boundary | Provider is part of thread config snapshots and startup/resume surfaces | Keep provider fixed for the live model-update feature and reject any attempt to smuggle provider mutation into the new surfaces | Prevent widening this minimal fork into cross-provider thread migration | No `model_provider` field on `update_session_model` or `thread/model/set` | Existing config snapshot / resume tests plus new negative tests |
| Thread read/resume/fork surfaces | `codex-rs/app-server-protocol/src/protocol/v2.rs`, `codex-rs/app-server/src/codex_message_processor.rs` | `ThreadResumeResponse`, `ThreadForkResponse`, `ThreadReadResponse` and related loaders | Truth comes from persisted metadata and startup responses | Ensure these surfaces reflect the new persisted model/reasoning immediately after live change | All control surfaces means read/resume/fork too, not just active UI | No API shape change required if persisted metadata is updated correctly | App-server resume/read tests |
| Docs and generated artifacts | `codex-rs/app-server/README.md`, `docs/config.md`, `docs/skills.md`, generated schema/types under `codex-rs/app-server-protocol/schema/*`, `sdk/python/src/codex_app_server/*` | product and protocol truth surfaces | Docs describe existing manual/startup-driven behavior only | Update docs and regenerate protocol artifacts | Avoid stale truth after shipping the new control surface | Document skill-driven session-setting changes and new v2 surfaces | Schema-generation checks and SDK regeneration |

## Migration notes

* Canonical owner path / shared code path:
  * `SessionSettingsUpdate` remains the single live mutation payload, wrapped by one shared apply-and-emit helper in `codex-rs/core/src/codex.rs` for model/effort updates.
* Deprecated APIs (if any):
  * None in the first cut. Existing `turn/start` overrides remain supported.
* Delete list (what must be removed; include superseded shims/parallel paths if any):
  * Do not add or keep any attempt to reuse `SessionConfiguredEvent` for post-start changes.
  * Do not add polling-based UI refresh logic or a skill-private shadow settings store.
  * Do not add a generic public session-settings mutation surface when the shipped behavior is still model-plus-reasoning only.
  * Rewrite any touched docs/comments that imply only startup or manual user flows can change model/reasoning on the current thread.
* Adjacent surfaces tied to the same contract family:
  * `/model`, `model/list`, `list_available_models`, `update_session_model`, `turn/start`, `thread/model/set`, `thread/model/updated`, `thread/resume`, `thread/fork`, `thread/read`, remote TUI `ThreadSessionState`, embedded TUI header/history/status, MCP outgoing events, and persisted thread metadata.
* Compatibility posture / cutover plan:
  * Additive exposure of the same live mutation path; existing user-driven flows stay intact, but every successful post-start model mutation now emits the new durable event.
* Capability-replacing harnesses to delete or justify:
  * No plugin-only side channel, no prompt-only fake “model changed” narration, and no out-of-band config mutation to simulate live state changes.
* Live docs/comments/instructions to update or delete:
  * `codex-rs/app-server/README.md`, `docs/config.md`, `docs/skills.md`, and any touched in-product help text around `/model`, `update_session_model`, or skill capabilities.
* Behavior-preservation signals for refactors:
  * `codex-rs/core/tests/suite/model_switching.rs`
  * `codex-rs/state/src/extract.rs` tests
  * app-server protocol serialization/tests
  * TUI snapshot coverage for session header/history/status

## Pattern Consolidation Sweep (anti-blinders; scoped by plan)
| Area | File / Symbol | Pattern to adopt | Why (drift prevented) | Proposed scope (include/defer/exclude/blocker question) |
| ---- | ------------- | ---------------- | ---------------------- | ------------------------------------- |
| Root-thread control tools | `codex-rs/core/src/tools/handlers/request_user_input.rs` | Root-thread-only handler guard for parent-session mutation tools | Prevent subagents from mutating the parent thread through a tool side path | include |
| Post-start session events | `codex-rs/core/src/thread_manager.rs`, `codex-rs/mcp-server/src/codex_tool_runner.rs` | Startup-only `SessionConfigured`, separate post-start `SessionModelUpdated` event | Prevent breaking startup consumers while still providing live updates | include |
| Remote notification bridge | `codex-rs/app-server/src/bespoke_event_handling.rs` plus protocol v2 notifications | Dedicated event-to-notification mapping for live settings changes | Prevent remote TUI/SDK lag or heuristic polling | include |
| Persisted thread summaries | `codex-rs/state/src/extract.rs`, `codex-rs/state/src/runtime/threads.rs` | Immediate persisted metadata update from non-turn session changes | Prevent `thread/list` / `thread/read` drift after a live change | include |
| TUI manual and skill-driven sync | `codex-rs/tui/src/chatwidget.rs`, `codex-rs/tui/src/app_event.rs` | Shared post-start event as durable truth for both manual and skill-driven changes | Prevent separate UI-only and skill-only update stories | include |
| Startup-only history/help text | `codex-rs/tui/src/history_cell.rs`, docs under `docs/` | Keep startup session info separate from later change notifications | Prevent conflating initial configuration with later live changes | include |
| Provider boundary | `codex-rs/app-server/src/codex_message_processor.rs`, startup/resume config surfaces | Keep provider fixed while live model updates change only model plus reasoning | Prevent scope creep into cross-provider thread migration | include |
| Subagent model controls | existing spawn-agent/request APIs | Preserve existing subagent-specific model selection path | Avoid scope creep into parent-child override semantics | exclude |
<!-- arch_skill:block:call_site_audit:end -->

<!-- arch_skill:block:phase_plan:start -->
# 7) Depth-First Phased Implementation Plan (authoritative)

> Rule: systematic build, foundational first; split Section 7 into the best sequence of coherent self-contained units, optimizing for phases that are fully understood, credibly testable, compliance-complete, and safe to build on later. If two decompositions are both valid, bias toward more phases than fewer. `Work` explains the unit and is explanatory only for modern docs. `Checklist (must all be done)` is the authoritative must-do list inside the phase. `Exit criteria (all required)` names the exhaustive concrete done conditions the audit must validate. Resolve adjacent-surface dispositions and compatibility posture before writing the checklist. Before a phase is valid, run an obligation sweep and move every required promise from architecture, call-site audit, migration notes, delete lists, verification commitments, docs/comments propagation, approved bridges, and required helper follow-through into `Checklist` or `Exit criteria`. The authoritative checklist must name the actual chosen work, not unresolved branches or "if needed" placeholders. Refactors, consolidations, and shared-path extractions must preserve existing behavior with credible evidence proportional to the risk. For agent-backed systems, prefer prompt, grounding, and native-capability changes before new harnesses or scripts. No fallbacks/runtime shims - the system must work correctly or fail loudly (delete superseded paths). If a bridge is explicitly approved, timebox it and include removal work; otherwise plan either clean cutover or preservation work directly. Prefer programmatic checks per phase; defer manual/UI verification to finalization. Avoid negative-value tests and heuristic gates (deletion checks, visual constants, doc-driven gates, keyword or absence gates, repo-shape policing). Also: document new patterns/gotchas in code comments at the canonical boundary (high leverage, not comment spam).

## Phase 1 — Add the canonical live model-update owner path

* Goal:
  * Land one shared runtime path for post-start model/reasoning mutation that both manual and skill-driven flows can use.
* Work:
  * Establish the core `SessionModelUpdated` contract and route model/effort overrides through one apply-and-emit helper in `codex-rs/core`.
* Checklist (must all be done):
  * Add `SessionModelUpdated` and its source enum to `codex-rs/protocol/src/protocol.rs` with old/new model and reasoning values plus the active-turn carry-forward flag.
  * Add a shared helper in `codex-rs/core/src/codex.rs` that wraps `Session::update_settings()`, emits `SessionModelUpdated` on success, and returns loud errors on invalid updates.
  * Keep omitted-effort semantics strict: preserve the current reasoning setting only when it remains valid for the requested model; otherwise reject loudly and require an explicit compatible effort or clear, with no auto-normalization fallback.
  * Route `Op::OverrideTurnContext` through that helper when `model` or `effort` actually changed, while leaving unrelated override fields on their existing behavior path.
  * Keep the confirmed active-turn semantic explicit in the event payload and core behavior: the in-flight request keeps its original model and reasoning settings; new defaults apply to the next request / turn.
  * Add or update high-leverage boundary comments/doc comments at the canonical helper and new event type so later readers do not confuse startup configuration with post-start model updates.
* Verification (required proof):
  * Focused core tests proving successful model/effort updates emit the new event and rejected updates still fail loudly.
  * Extend existing core model-switching coverage to assert next-turn semantics after the new evented path is introduced.
* Docs/comments (propagation; only if needed):
  * Add doc comments on the new event payload and the canonical core helper explaining the startup-vs-post-start boundary.
* Exit criteria (all required):
  * There is one canonical core path for post-start model/effort updates.
  * Manual and future skill-driven model updates share the same success event.
  * Invalid or locked changes fail loudly without introducing a shadow state path.
* Rollback:
  * Remove the new event/helper wiring and fall back to the pre-existing manual override flow.

## Phase 2 — Expose the root-thread tool contract

* Goal:
  * Make the parent agent able to read valid model choices and request the live model update through bounded built-in tools.
* Work:
  * Add `list_available_models` and `update_session_model` to the built-in tool registry so skills can instruct the model to read valid choices first and mutate the root session second.
* Checklist (must all be done):
  * Add the `list_available_models` tool schema in `codex-rs/tools/src/` and define its output around the existing catalog truth: visible model slug, display name, default reasoning effort, and supported reasoning efforts.
  * Add the `update_session_model` tool schema in `codex-rs/tools/src/` with only `model` and `reasoning_effort` inputs.
  * Register both tools and their handler kinds in `codex-rs/tools/src/tool_registry_plan.rs` and `tool_registry_plan_types.rs`.
  * Implement the handlers in `codex-rs/core/src/tools/handlers/` and wire them through `codex-rs/core/src/tools/spec.rs` and handler registration.
  * Keep `list_available_models` read-only and backed by the same runtime catalog truth as existing `model/list`.
  * Enforce the root-thread boundary in the handler, following the same pattern family as `request_user_input`.
  * Reject any attempt to carry `model_provider` or other out-of-scope settings through the tool contract.
  * Return structured success output containing the applied model, applied reasoning effort, and whether the current in-flight turn keeps its previous model and reasoning settings.
* Verification (required proof):
  * Tool-spec tests for the new schemas and registry wiring.
  * Handler tests for truthful catalog output, root-thread-only enforcement, successful updates, and invalid-argument failures.
* Docs/comments (propagation; only if needed):
  * Keep the tool descriptions aligned with `/model` and `model/list` semantics so the model sees one coherent contract family.
* Exit criteria (all required):
  * A skill can instruct the model to read valid model/effort choices from runtime truth before making a change.
  * A skill can call `update_session_model` on the root thread.
  * The tool cannot mutate provider or unrelated session settings.
  * Subagents cannot use the tool to mutate the parent session.
* Rollback:
  * Remove the tool specs and handlers while leaving the Phase 1 core path available to manual flows.

## Phase 3 — Make persistence and thread read models truthful immediately

* Goal:
  * Ensure live model updates become durable thread truth immediately, not only after a later turn emits a new `TurnContextItem`.
* Work:
  * Persist and extract `SessionModelUpdated` so thread list/read/resume/fork surfaces see the new model and reasoning state right away.
* Checklist (must all be done):
  * Persist `SessionModelUpdated` as a normal rollout event without introducing a new parallel rollout item type.
  * Extend `codex-rs/state/src/extract.rs` to apply `SessionModelUpdated` to `ThreadMetadata.model` and `ThreadMetadata.reasoning_effort`.
  * Ensure `codex-rs/state/src/runtime/threads.rs` continues to upsert the updated metadata cleanly into the `threads` table.
  * Verify that `thread/list`, `thread/read`, `thread/resume`, and `thread/fork` read paths surface the updated model/reasoning values from persisted metadata immediately after a live update.
  * Add negative coverage that provider remains unchanged across this persisted read-model path.
* Verification (required proof):
  * State extraction/runtime tests covering the new event.
  * App-server resume/read/fork tests demonstrating immediate truth after a live model update.
* Docs/comments (propagation; only if needed):
  * Add a brief comment near extraction if needed to explain why post-start model updates bypass the old `TurnContextItem`-only assumption.
* Exit criteria (all required):
  * Persisted thread metadata advances on `SessionModelUpdated`, not only on a later turn.
  * Read/list/resume/fork surfaces stay truthful immediately after the live update.
  * No new storage table or shadow metadata path is introduced.
* Rollback:
  * Remove event-based metadata extraction and revert to turn-context-only persistence.

## Phase 4 — Add the stable app-server and SDK control surfaces

* Goal:
  * Give non-agent clients the same capability and make remote consumers observe the same live update contract.
* Work:
  * Add stable `thread/model/set` and `thread/model/updated` surfaces and keep generated schemas and SDKs aligned.
* Checklist (must all be done):
  * Add `thread/model/set` request/response types in `codex-rs/app-server-protocol/src/protocol/v2.rs` and register the method in `common.rs`.
  * Add `thread/model/updated` notification types in the same protocol crate and register them in `common.rs`.
  * Implement server-side handling in `codex-rs/app-server/src/codex_message_processor.rs` that routes to the same Phase 1 core helper.
  * Bridge `SessionModelUpdated` to `thread/model/updated` in `codex-rs/app-server/src/bespoke_event_handling.rs` and `outgoing_message.rs`.
  * Keep `thread/model/set` on the same strict omitted-effort contract as the tool and manual flows: preserve when valid, otherwise reject loudly and require explicit caller intent.
  * Regenerate app-server protocol schema artifacts and keep SDK generated types aligned with the new stable method and notification names.
  * Preserve existing `turn/start` override behavior unchanged while adding the new dedicated surface.
* Verification (required proof):
  * `cargo test -p codex-app-server-protocol`
  * App-server request/response tests for `thread/model/set`
  * Notification serialization/integration tests for `thread/model/updated`
* Docs/comments (propagation; only if needed):
  * Update any protocol docs/comments co-located with the new method/notification types so they clearly describe live post-start model mutation.
* Exit criteria (all required):
  * Non-agent clients can perform the same live model update without synthesizing a user turn.
  * Remote consumers can observe `thread/model/updated` as the canonical post-start live update signal.
  * Generated schemas and SDK surfaces match the shipped protocol contract.
* Rollback:
  * Remove `thread/model/set` / `thread/model/updated` and keep the embedded local feature only.

## Phase 5 — Update TUI and MCP consumers to the new live model contract

* Goal:
  * Make all visible and MCP-facing consumers render the same truthful live model update story.
* Work:
  * Consume the new event/notification in embedded TUI, remote TUI, and MCP bridges, with clear but compact user-visible output.
* Checklist (must all be done):
  * Update embedded TUI `chatwidget`, `app_event`, `history_cell`, and status helpers to consume `SessionModelUpdated`, refresh collaboration state, and render a non-contradictory skill-driven change line.
  * Update remote TUI session-state handling in `app_server_session`, `app`, and any relevant app-server adapters to consume `thread/model/updated`.
  * Update MCP bridging and `codex_tool_runner` to treat `SessionModelUpdated` as the expected post-start model event instead of flagging startup-only `SessionConfigured`.
  * Ensure `/model`, reroute messaging, and the new skill-driven update surfaces do not produce contradictory state or duplicated messaging.
  * Add or update snapshots and focused behavior tests for any user-visible rendering change.
* Verification (required proof):
  * TUI snapshot tests and focused behavior tests for embedded and remote update flows.
  * MCP outgoing-message tests covering the new event handling.
* Docs/comments (propagation; only if needed):
  * Add or refine small comments only where the startup-only versus post-start event distinction would otherwise be non-obvious.
* Exit criteria (all required):
  * Embedded and remote TUI both show the updated model/reasoning truth promptly after the change.
  * MCP consumers no longer treat the post-start update as an unexpected startup event.
  * No stale or contradictory model messaging remains in touched UI surfaces.
* Rollback:
  * Remove the new consumer paths while preserving the protocol and core behavior for debugging.

## Phase 6 — Reality-sync docs and final end-to-end proof

* Goal:
  * Leave the fork coherent, documented, and demonstrably correct using the repo’s existing test framework wherever possible.
* Work:
  * Update live docs/help and add the final end-to-end coverage for a skill-driven model change followed by a new turn.
* Checklist (must all be done):
  * Update `codex-rs/app-server/README.md`, `docs/config.md`, `docs/skills.md`, and any touched help text that would otherwise imply model changes are startup-only or user-only.
  * Add end-to-end coverage for `update_session_model` followed by a subsequent turn using the new model/reasoning settings.
  * Add negative tests proving `model_provider` is not part of the new live mutation contract.
  * Verify no touched doc, comment, or instruction surface still advertises the wrong post-start model-update story.
* Verification (required proof):
  * Existing integration tests extended to cover the full sequence.
  * Generated-schema checks and any repo-standard protocol artifact regeneration checks required by the touched surfaces.
* Docs/comments (propagation; only if needed):
  * This phase owns the doc reality-sync work; if a touched doc/help surface would remain stale, the phase is not complete.
* Exit criteria (all required):
  * Docs and tests match the shipped `update_session_model` / `thread/model/set` / `thread/model/updated` behavior.
  * The feature is provable end to end with existing repo test infrastructure rather than relying on narrative explanation.
  * No touched live truth surface still implies a different ownership or migration story.
* Rollback:
  * Revert the docs and final end-to-end coverage alongside the feature if the feature is backed out.
<!-- arch_skill:block:phase_plan:end -->

# 8) Verification Strategy (common-sense; non-blocking)

## 8.1 Core verification

- Extend existing core model-switching tests instead of building a new harness.
- Add targeted coverage for `list_available_models`, `update_session_model`, `SessionModelUpdated`, and validation failures on the shared core path, including incompatible model changes where effort was omitted.

## 8.2 Protocol and persistence verification

- Add app-server integration/serialization tests for `thread/model/set` and `thread/model/updated`.
- Extend persistence coverage so thread metadata and resume/read/fork surfaces reflect `SessionModelUpdated` immediately.

## 8.3 UI verification

- Add or update TUI snapshots for session header/history/status changes.
- Manually validate idle-thread and active-turn flows during finalization after the programmatic checks pass.

## 8.4 Verification bias

- Prefer existing tests, snapshot infrastructure, and behavior-level assertions.
- Do not add config-file mutation tests for live session changes.

# 9) Rollout / Ops / Telemetry

## 9.1 Rollout shape

- This is a local Codex fork feature, so the main rollout concern is internal consistency across client/protocol surfaces rather than staged deployment.

## 9.2 Operational considerations

- Any new notification or RPC should be additive where possible so older clients fail obviously rather than silently misrendering state.
- Invalid requests should produce explicit errors instead of hidden fallback behavior.

## 9.3 Telemetry / diagnostics

- Preserve or extend existing history/event signals so it is diagnosable when a model or reasoning change came from a skill/tool rather than a manual control surface.

<!-- arch_skill:block:consistency_pass:start -->
## Consistency Pass
- Reviewers: explorer 1, explorer 2, self-integrator
- Scope checked:
  - frontmatter, TL;DR, Sections 0 through 10, helper blocks, and cross-section agreement for scope, compatibility posture, owner path, execution order, verification, and rollout
- Findings summary:
  - The target architecture still contained an implicit model/effort normalization fallback that contradicted the plan's fail-loud stance.
  - The active-turn carry-forward contract was under-specified in a few places as model-only instead of model-plus-reasoning.
  - The Decision Log still mentioned the provisional `SessionSettingsChanged` name without explicitly recording that the final chosen event is `SessionModelUpdated`.
- Integrated repairs:
  - Made omitted-effort behavior explicit and fail-loud: preserve the current reasoning setting only when it remains valid; otherwise reject and require explicit caller intent.
  - Tightened tool, RPC, event, and phase-plan wording so the in-flight carry-forward contract consistently covers both model and reasoning settings.
  - Synced the Decision Log to the final event naming and recorded the consistency-pass resolution.
- Remaining inconsistencies:
  - none
- Unresolved decisions:
  - none
- Unauthorized scope cuts:
  - none
- Decision-complete:
  - yes
- Decision: proceed to implement? yes
<!-- arch_skill:block:consistency_pass:end -->

# 10) Decision Log (append-only)

## 2026-04-16

- Draft created from initial ask.
- Default direction is to reuse `Op::OverrideTurnContext` / `Session::update_settings()` as the single live settings mutation path.
- Default compatibility posture is additive exposure of live model/effort mutation to skills and clients, not a new config/profile layer.
- North Star confirmed and doc promoted to `active`.
- Confirmed active-turn semantic: skill-issued changes during an active turn update session defaults for the next model request / subsequent turn rather than retargeting the already-issued in-flight request.
- Deep-dive pass 1 selected a new persisted post-start `SessionSettingsChanged` event and explicitly rejected reusing startup-only `SessionConfiguredEvent` for live settings mutation.
- Deep-dive pass 1 selected one shared architecture: root-thread-only built-in tool plus dedicated v2 thread-scoped RPC, both landing on the same core apply-and-emit helper over `SessionSettingsUpdate`.
- Deep-dive pass 2 superseded the provisional `SessionSettingsChanged` name with the final `SessionModelUpdated` event and narrowed the design to the repo's existing model contract family: `update_session_model`, `thread/model/set`, `thread/model/updated`, and `SessionModelUpdated`.
- Deep-dive pass 2 explicitly kept `model_provider` out of scope for live mutation; provider changes remain a thread start/resume/fork concern rather than part of this minimal fork.
- Phase-plan pass split execution into six phases: core owner path, tool exposure, persisted read model, stable app-server/SDK surfaces, TUI/MCP consumers, and final docs/end-to-end proof.
- Consistency pass resolved the last plan-shaping ambiguity: when a caller changes model without explicitly providing effort, Codex preserves the current reasoning setting only if the pair remains valid; otherwise the mutation fails loudly and requires an explicit compatible effort or explicit clear. No auto-normalization fallback is approved.
- Consistency pass clarified that the active-turn carry-forward contract always covers both model and reasoning settings, not model alone.
- Clarification pass made the invocation boundary explicit: skills are instruction text only, while the runtime exposes model-visible built-in tools that the model sees and calls. For this workflow, that means `list_available_models` for reads and `update_session_model` for writes on the model side, with `model/list` and `thread/model/set` remaining the client-facing RPC surfaces.
- 2026-04-17 follow-on: the branch's final skill-facing tool family is additive rather than replacing prior contracts: `get_current_session_model` for explicit current-state reads, `list_available_models` for catalog reads, and `update_session_model` for writes, while external clients still use `model/list` and `thread/model/set`.
