---
title: "Codex - Current Session Model Tools and Documentation - Architecture Plan"
date: 2026-04-17
status: active
fallback_policy: forbidden
owners: [aelaguiz]
reviewers: []
doc_type: architectural_change
related:
  - docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md
  - docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16_WORKLOG.md
  - docs/skills.md
  - docs/examples/skills/session-model-control-example/SKILL.md
  - docs/examples/skills/session-model-control-example/references/runtime-contract-and-examples.md
  - codex-rs/app-server/README.md
  - codex-rs/tools/src/session_model_tool.rs
  - codex-rs/core/src/tools/handlers/session_model.rs
---

# TL;DR

Outcome

Add exactly one new first-class skill-visible tool that reports the current root session model and reasoning effort, while keeping the already-landed `list_available_models` and `update_session_model` tools in place, and fully document the branch's session-model tool family and adjacent control surfaces so skills can inspect current state, list valid models and efforts, and change the live session without ambiguity.

Problem

This branch already added skill-visible catalog and mutation surfaces, live session syncing, persistence, and external RPC coverage for session model switching. But the current-state read story is still implicit inside `list_available_models` rather than explicit as its own contract, and the documentation set does not yet fully explain the runtime boundary, the full tool family, the signed-in OpenAI-account path, or how skills are supposed to inspect current state versus enumerate valid choices versus mutate settings.

Approach

Keep the existing live owner path and tool/runtime boundary. Add exactly one narrow built-in read tool for the current root-session model and reasoning effort, keep the already-existing `list_available_models` as the catalog read surface, keep the already-existing `update_session_model` as the write surface, and then do a documentation convergence pass across the plan doc, worklog, evergreen docs, app-server docs, and example skill so they all describe the same shipped behavior in the same terms.

Plan

1. Ground the current implementation and documentation gap precisely, then lock the compatibility stance that `list_available_models` keeps its current-state fields while the new tool becomes the dedicated explicit current-state surface.
2. Add the dedicated current-state read tool on the same root-thread-only boundary as the existing model tools and verify skills can inspect current state, list valid choices, and set the model cleanly.
3. Update the branch's documentation set so the tool/runtime split, current-state tool, catalog tool, write tool, RPC surfaces, event surfaces, persistence behavior, and OpenAI-account support are documented coherently.

Non-negotiables

- No second source of truth for live session model or reasoning state.
- No provider switching, hidden fallbacks, or in-flight request retargeting.
- Skills must use model-visible built-in tools; they do not call Rust internals or app-server RPCs directly.
- Documentation must describe actual shipped behavior, not aspirational behavior.
- The documented story must stay true for both OpenAI-account and API-key authenticated sessions.

<!-- arch_skill:block:implementation_audit:start -->
# Implementation Audit (authoritative)
Date: 2026-04-17
Verdict (code): COMPLETE
Manual QA: n/a (non-blocking)

## Code blockers (why code is not done)
- None.

## Reopened phases (false-complete fixes)
- None.

## Missing items (code gaps; evidence-anchored; no tables)
- None.

## Non-blocking follow-ups (manual QA / screenshots / human verification)
- None.
<!-- arch_skill:block:implementation_audit:end -->

<!-- arch_skill:block:planning_passes:start -->
<!--
arch_skill:planning_passes
deep_dive_pass_1: done 2026-04-17
recommended_flow: research -> deep dive -> phase plan -> implement
note: This block tracks stage order only. It never overrides readiness blockers caused by unresolved decisions.
-->
<!-- arch_skill:block:planning_passes:end -->

# 0) Holistic North Star

## 0.1 The claim (falsifiable)

If Codex exposes three explicit skill-visible built-in tools for this contract family, one to read the current root-session model and reasoning effort, one to list valid runtime-supported model and effort choices, and one to mutate the live root session for future requests, then skills can inspect and control the current session honestly while every documentation and control surface describes the same behavior without inventing a second settings system.

## 0.2 In scope

- Adding exactly one new dedicated model-visible built-in read tool for the current root-session model and reasoning effort. Working assumption: `get_current_session_model`.
- Keeping skill-visible catalog and mutation surfaces explicit and distinct:
  - the already-existing `list_available_models` for valid model and reasoning-effort choices from the live catalog.
  - the already-existing `update_session_model` for root-thread live mutation that applies to future requests, not the already-issued in-flight request.
- Clarifying and preserving the tool/runtime boundary:
  - skills are instruction text only;
  - the model sees built-in tools in its tool list;
  - the model calls those tools;
  - external clients continue to use RPCs such as `model/list` and `thread/model/set`.
- Documenting the full already-landed branch behavior across adjacent surfaces that must stay in sync:
  - tool contracts and examples;
  - app-server RPC and notification surfaces;
  - persistence and resume truth;
  - TUI-visible behavior;
  - signed-in OpenAI-account support, not only API-key flows.

## 0.3 Out of scope

- Model-provider switching or any post-start provider mutation.
- Changing the confirmed active-turn semantic: the current in-flight request keeps its original model and reasoning settings.
- General arbitrary skill-driven mutation of unrelated session settings.
- New profile, config, or collaboration-mode products.
- Documentation that hand-waves over actual implementation boundaries or claims skills invoke host internals directly.

## 0.4 Definition of done (acceptance evidence)

- A skill-visible built-in tool exists for reading the current root-session model and reasoning effort.
- Skills can clearly do all three operations through explicit tool contracts:
  - inspect current model and reasoning;
  - list valid models and supported reasoning efforts;
  - set the live session model and reasoning for future requests.
- The implementation continues to use the existing canonical owner path for live session mutation.
- The branch documentation set fully explains the shipped behavior, including the distinction between model-visible tools and external RPCs.
- The documentation set explicitly covers that the feature works for signed-in OpenAI-account sessions as well as API-key sessions.
- Tests and examples cover or point to current-state reads, catalog reads, live mutation, and the truthfulness of surfaced state.

## 0.5 Key invariants (fix immediately if violated)

- No dual truth surfaces for current session model or reasoning.
- No silent normalization of invalid model and reasoning combinations.
- No documentation drift between code, example skill, app-server docs, and evergreen skills docs.
- No regression to an API-key-only story; signed-in OpenAI-account behavior must remain true and documented.
- No ambiguity about who calls what: skills instruct, tools execute, RPCs remain client-facing.

# 1) Key Design Considerations (what matters most)

## 1.1 Priorities (ranked)

1. Make the current-state read contract explicit for skills.
2. Keep one canonical runtime owner path for live session settings.
3. Document the entire contract family in a way that a cold reader can actually use.
4. Preserve the already-approved active-turn and provider boundaries.

## 1.2 Constraints

- The branch already ships `list_available_models`, `update_session_model`, and the live owner path beneath them.
- `list_available_models` currently returns current-state fields, so adding a dedicated read tool must not create contradictory truth or confusing overlap.
- The branch already has app-server, persistence, and TUI sync behavior that docs must describe accurately rather than reinterpret.
- The user explicitly wants this understandable from inside skills, which means the docs must be clearer about model-visible built-in tools versus RPC surfaces.

## 1.3 Architectural principles (rules we will enforce)

- Separate "read current state", "list valid choices", and "mutate session" as explicit contracts, while keeping them backed by one runtime truth.
- Prefer additive clarification over renaming or reshaping stable branch behavior unless research proves the current contract is misleading.
- Treat documentation as part of the shipped architecture surface for this feature, not as an afterthought.

## 1.4 Known tradeoffs (explicit)

- A dedicated current-state tool overlaps with fields already returned by `list_available_models`; this follow-on should preserve that overlap for compatibility and document the new tool as the primary explicit current-state contract.
- Full branch documentation will take more than one file update because the truth currently spans tools, RPCs, events, tests, and example skill guidance.

# 2) Problem Statement (existing architecture + why change)

## 2.1 What exists today

- Skills can already use branch-added tool surfaces to list valid models and update the live session model and reasoning settings.
- The existing catalog read path already includes current-state fields, but that behavior is not yet formalized as its own first-class skill-facing contract.
- Branch docs already exist, but they are split across a plan doc, a worklog, app-server docs, evergreen skills docs, and an example skill package.

## 2.2 What's broken / missing (concrete)

- There is no explicit dedicated tool whose only job is "tell me the current model and thinking level right now."
- The documentation set does not yet fully explain the three distinct skill tasks:
  - check current state;
  - list valid choices;
  - update the live session.
- The tool-versus-RPC boundary has already caused confusion and needs to be stated bluntly in the permanent docs and example skill.

## 2.3 Constraints implied by the problem

- We need a clearer skill-facing read contract without reopening the already-implemented mutation architecture.
- We need a documentation sweep that is broad enough to cover the whole branch behavior but still anchored in code truth.

<!-- arch_skill:block:research_grounding:start -->
# 3) Research Grounding (external + internal "ground truth")

## 3.1 External anchors (papers, systems, prior art)

- None required. This is a repo-internal tool-contract and documentation convergence change, so the credible move is to reuse the existing Codex tool/runtime and live-session owner paths rather than import outside patterns.

## 3.2 Internal ground truth (code as spec)

- Authoritative behavior anchors (do not reinvent):
  - `codex-rs/tools/src/session_model_tool.rs` — today the branch exposes exactly two model-visible tools, `list_available_models` and `update_session_model`; the list-tool description only promises catalog reads, while the handler contract is broader.
  - `codex-rs/core/src/tools/handlers/session_model.rs` — `ListAvailableModelsResult` already returns `current_model`, `reasoning_effort`, and `models`, while `UpdateSessionModelHandler` routes writes through the canonical session-update helper and enforces the root-thread-only boundary.
  - `codex-rs/core/src/codex.rs` — `apply_settings_update_and_emit_session_model_event()` is the canonical post-start owner path for live session model and reasoning updates.
  - `codex-rs/protocol/src/protocol.rs` — `SessionModelUpdatedEvent` is the persisted post-start event carrying old/new model and reasoning values plus the active-turn carry-forward flag.
  - `codex-rs/state/src/extract.rs` — persisted thread metadata already updates immediately from `EventMsg::SessionModelUpdated`.
- Canonical path / owner to reuse:
  - `codex-rs/tools/src/session_model_tool.rs`, `codex-rs/tools/src/tool_registry_plan.rs`, and `codex-rs/core/src/tools/handlers/session_model.rs` — the new current-state read tool should live as a sibling in the existing session-model tool family, be model-visible through the same registry path, and read live collaboration-mode state directly from the current session without adding a second catalog or settings store.
- Adjacent surfaces tied to the same contract family:
  - `codex-rs/core-skills/src/render.rs`, `codex-rs/core/src/codex.rs`, and `codex-rs/core/src/tools/router.rs` — skills are injected as prompt instructions and the model sees built-in tools through `render_skills_section(...)` plus `router.model_visible_specs()`, so the skill-facing explanation must describe tool calls rather than direct host-function calls.
  - `docs/skills.md` — the evergreen runtime-boundary doc currently mentions only `list_available_models` and `update_session_model`, so it will drift unless the new current-state tool and the tool-versus-RPC split are added there.
  - `docs/examples/skills/session-model-control-example/SKILL.md` and `docs/examples/skills/session-model-control-example/references/runtime-contract-and-examples.md` — the example skill package currently assumes exactly two tools and uses `list_available_models` for both valid-choice discovery and implied current-state reads, so it must be updated to show the three-tool family clearly.
  - `codex-rs/app-server/README.md`, `codex-rs/app-server/src/codex_message_processor.rs`, and `codex-rs/app-server/src/bespoke_event_handling.rs` — external clients already use `model/list`, `thread/model/set`, and `thread/model/updated`; docs must keep these separate from skill-visible tools and continue describing them as client-facing RPCs/notifications rather than skill calls.
  - `codex-rs/tui/src/chatwidget.rs` and `codex-rs/tui/src/chatwidget/tests/app_server.rs` — TUI already consumes `SessionModelUpdated` and renders a user-visible change line, so the full documentation sweep must keep the branch's live-state story aligned with what users already see.
  - `codex-rs/app-server/tests/suite/v2/thread_model_set.rs` — branch proof already covers both persisted live updates and the signed-in ChatGPT/OpenAI-account path, which the docs need to surface explicitly.
- Compatibility posture (separate from `fallback_policy`):
  - Preserve the existing `list_available_models` response shape, including `current_model` and `reasoning_effort`, and add one new dedicated current-state read tool as an additive explicit contract. Repo truth already ships the overlap today, and this follow-on ask is to add one new tool, not to trim or rename the existing list tool.
- Existing patterns to reuse:
  - `codex-rs/core/src/tools/handlers/session_model.rs` — reuse `reject_subagent_thread(...)` so the new tool stays root-thread-only in the same way as the existing session-model tools.
  - `codex-rs/core/src/tools/handlers/session_model.rs` — reuse `serialize_tool_result(...)` so the new tool returns JSON text in the same built-in tool style as the existing handlers.
  - `codex-rs/tools/src/tool_registry_plan.rs` and `codex-rs/core/src/tools/spec.rs` — reuse the existing tool-spec registration and handler-kind wiring rather than inventing another tool exposure path.
- Prompt surfaces / agent contract to reuse:
  - `codex-rs/core-skills/src/render.rs` — `<skills_instructions>` is the prompt surface that tells the model what skills exist and where their `SKILL.md` files live.
  - `codex-rs/core/src/codex.rs` and `codex-rs/core/src/tools/router.rs` — the model already receives the visible built-in tool list for the turn, so the new behavior should remain “skill instructs, model calls tool” rather than adding a separate control mechanism.
- Native model or agent capabilities to lean on:
  - Built-in function-tool calling already handles this workflow. The model can decide when to inspect current state, inspect valid choices, and apply a change once the host exposes the dedicated current-state tool.
- Existing grounding / tool / file exposure:
  - The runtime already exposes the existing session-model tools through the built-in tool registry, and the model already receives the available-skill inventory and tool list in the same session context.
- Duplicate or drifting paths relevant to this change:
  - `list_available_models` currently serves two jobs at once: catalog read plus implicit current-state read. That overlap is real shipped behavior, but it is under-documented and easy to mis-explain.
  - The branch truth is currently spread across `docs/skills.md`, the example skill package, the main architecture plan/worklog, app-server README, runtime tool specs, and tests. Without an explicit documentation sweep, these surfaces will continue to drift.
  - External RPCs (`model/list`, `thread/model/set`, `thread/model/updated`) are adjacent but not skill-call surfaces; earlier confusion already shows this boundary needs to be documented as a first-class contract.
- Capability-first opportunities before new tooling:
  - Add one sibling built-in tool in the existing session-model family instead of building a new RPC, a shadow cache, or a prompt-only heuristic.
  - Keep `list_available_models` as the broad catalog source and document the new tool as the narrow “what is the session using now?” source instead of removing already-shipped overlap.
- Behavior-preservation signals already available:
  - `codex-rs/core/src/tools/handlers/session_model.rs` tests already protect the root-thread-only boundary and `update_session_model` argument validation, and they are the natural place to extend coverage for the new read tool.
  - `codex-rs/app-server/tests/suite/v2/thread_model_set.rs` already proves post-start mutation, persistence, and ChatGPT/OpenAI-account-authenticated operation.
  - `codex-rs/state/src/extract.rs` tests already protect metadata extraction from `SessionModelUpdated`.
  - `codex-rs/tui/src/chatwidget/tests/app_server.rs` already protects the visible live-update rendering path.

## 3.3 Decision gaps that must be resolved before implementation

- None currently. Repo evidence settles the main architecture choice for this follow-on:
  - add exactly one new built-in current-state read tool in the existing session-model tool family;
  - preserve `list_available_models` as-is for compatibility, including its current-state fields;
  - document the three-tool family and the client-RPC boundary explicitly rather than reshaping already-landed branch behavior.
<!-- arch_skill:block:research_grounding:end -->

<!-- arch_skill:block:current_architecture:start -->
# 4) Current Architecture (as-is)

## 4.1 On-disk structure

- Model-visible tool definitions for this contract family live in `codex-rs/tools/src/session_model_tool.rs` and are exported through `codex-rs/tools/src/lib.rs`.
- Model-visible tool registration lives in `codex-rs/tools/src/tool_registry_plan.rs` with handler enum ownership in `codex-rs/tools/src/tool_registry_plan_types.rs`.
- Runtime tool handlers live in `codex-rs/core/src/tools/handlers/session_model.rs`, are re-exported via `codex-rs/core/src/tools/handlers/mod.rs`, and are wired into the runtime tool builder in `codex-rs/core/src/tools/spec.rs`.
- The canonical live session mutation owner path lives in `codex-rs/core/src/codex.rs` through `apply_settings_update_and_emit_session_model_event(...)`.
- Post-start event and persistence truth live in `codex-rs/protocol/src/protocol.rs` and `codex-rs/state/src/extract.rs`.
- User-facing explanation is currently split across `docs/skills.md`, `docs/examples/skills/session-model-control-example/`, `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md`, `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16_WORKLOG.md`, and `codex-rs/app-server/README.md`.

## 4.2 Control paths (runtime)

1. The runtime injects skill inventory into `<skills_instructions>` via `codex-rs/core-skills/src/render.rs`, and the model receives the visible built-in tool list via `router.model_visible_specs()` in `codex-rs/core/src/tools/router.rs`.
2. Today, a skill that wants either the current session model or the valid model catalog calls `list_available_models`.
3. `ListAvailableModelsHandler` in `codex-rs/core/src/tools/handlers/session_model.rs` reads the live catalog from `models_manager.list_models(...)`, reads the current collaboration mode from `session.collaboration_mode().await`, and returns both the current session state and the available models in one result.
4. `UpdateSessionModelHandler` validates that at least one of `model` or `reasoning_effort` is present, rejects subagent threads, constructs the updated collaboration mode, and routes the mutation through `apply_settings_update_and_emit_session_model_event(...)`.
5. On successful mutation, `SessionModelUpdatedEvent` becomes the shared post-start truth that persistence, TUI, app-server notifications, and other consumers already use.
6. External clients do not call these tools. They use app-server RPCs like `model/list` and `thread/model/set`, and they observe `thread/model/updated`.

## 4.3 Object model + key abstractions

- `LIST_AVAILABLE_MODELS_TOOL_NAME` and `UPDATE_SESSION_MODEL_TOOL_NAME` in `codex-rs/tools/src/session_model_tool.rs` define the current tool-family identity.
- `ListAvailableModelsResult` currently mixes two concepts:
  - the current session state via `current_model` and `reasoning_effort`
  - the valid catalog via `models`
- `UpdateSessionModelResult` reports both the before/after values and the carry-forward flag `current_turn_keeps_previous_model_and_reasoning`.
- `reject_subagent_thread(...)` in `codex-rs/core/src/tools/handlers/session_model.rs` is the shared root-thread-only boundary for the current session-model tools.
- `SessionSettingsUpdate` and `SessionModelUpdatedEvent` already own live session mutation and post-start propagation; there is no second mutable settings store.

## 4.4 Observability + failure behavior today

- Unsupported tool payloads and invalid JSON arguments fail through `FunctionCallError::RespondToModel(...)`.
- `list_available_models` and `update_session_model` both reject subagent threads loudly.
- `update_session_model` rejects empty requests and invalid model/effort combinations through the canonical session-update path.
- Live write observability is already strong:
  - `SessionModelUpdatedEvent` is emitted on successful change,
  - `thread/model/updated` is sent for app-server clients,
  - TUI consumes the event and renders the change line,
  - persisted thread metadata updates immediately from the same event.
- Live read observability is weaker in one narrow way: the current session state is available to skills only as fields embedded in the broader `list_available_models` result, and the docs do not describe that as a first-class contract.

## 4.5 UI surfaces (ASCII mockups, if UI work)

- No dedicated UI surface exists for the missing current-state read tool. The user-facing gap is explanatory rather than visual.
- Existing visible behavior already reflects writes cleanly through TUI history and status after `SessionModelUpdatedEvent`.
<!-- arch_skill:block:current_architecture:end -->

<!-- arch_skill:block:target_architecture:start -->
# 5) Target Architecture (to-be)

## 5.1 On-disk structure (future)

- Keep the session-model tool family in the same files:
  - extend `codex-rs/tools/src/session_model_tool.rs` with one new tool constant and schema factory for `get_current_session_model`
  - export it from `codex-rs/tools/src/lib.rs`
  - add one handler kind in `codex-rs/tools/src/tool_registry_plan_types.rs`
  - register the new spec/handler in `codex-rs/tools/src/tool_registry_plan.rs`
  - add the runtime handler in `codex-rs/core/src/tools/handlers/session_model.rs`
  - re-export/register it in `codex-rs/core/src/tools/handlers/mod.rs` and `codex-rs/core/src/tools/spec.rs`
- Keep the documentation convergence in existing homes rather than inventing a second evergreen architecture doc:
  - `docs/skills.md` for the runtime boundary
  - the example skill package for the practical workflow
  - the existing branch plan/worklog for branch-history truth

## 5.2 Control paths (future)

1. When a user asks what the current session is using, the skill instructs the model to call `get_current_session_model`.
2. When a user asks what models or reasoning efforts are valid, the skill instructs the model to call `list_available_models`.
3. When a user asks to change the current session, the skill instructs the model to call `update_session_model`.
4. `get_current_session_model` reads the live root-session collaboration mode directly from `session.collaboration_mode().await`, serializes a narrow result, and reuses the same root-thread-only boundary helper as the other session-model tools.
5. No new event, persistence, TUI, or app-server RPC path is introduced for this follow-on. The existing write path stays the single owner for live mutation, and the new tool is read-only.
6. `list_available_models` keeps returning `current_model` and `reasoning_effort` for compatibility, but docs and examples treat `get_current_session_model` as the explicit preferred surface for “what is this session using now?”

## 5.3 Object model + abstractions (future)

- Add one new tool name and one new narrow result shape:
  - working name: `get_current_session_model`
  - result: current session `model` plus current session `reasoning_effort`
- Do not add `model_provider`, catalog data, or mutation semantics to this tool.
- Keep `list_available_models` unchanged as the broad catalog contract.
- Keep `update_session_model` unchanged as the write contract that applies to subsequent requests and turns.

## 5.4 Invariants and boundaries

- Exactly one new tool is added. No existing tool or RPC is removed or renamed.
- The new tool is root-thread-only, matching the boundary style of the existing session-model tools.
- The new tool is read-only. It does not emit `SessionModelUpdatedEvent`, mutate state, or create a new persisted truth surface.
- The session-model family remains one contract family with distinct jobs:
  - `get_current_session_model` = explicit current-state read
  - `list_available_models` = valid catalog read
  - `update_session_model` = live write
- Compatibility posture is additive, not a cutover:
  - preserve the existing `list_available_models` shape,
  - preserve the existing write and RPC behavior,
  - document the preferred tool choice rather than forcing migration.
- Skills remain instruction text only. Deterministic code exposes the tools; prompt behavior decides when to call each one.
- External clients continue to use `model/list`, `thread/model/set`, and `thread/model/updated`. No new app-server surface is needed for this follow-on.
- Provider mutation remains out of scope.

## 5.5 UI surfaces (ASCII mockups, if UI work)

- No new TUI or app-server UI state is required.
- The user-visible improvement is documentation and skill behavior clarity:
  - current-state questions map to the new read tool
  - valid-choice questions map to the existing catalog tool
  - mutation requests map to the existing write tool
<!-- arch_skill:block:target_architecture:end -->

<!-- arch_skill:block:call_site_audit:start -->
# 6) Call-Site Audit (exhaustive change inventory)

## 6.1 Change map (table)

| Area | File | Symbol / Call site | Current behavior | Required change | Why | New API / contract | Tests impacted |
| ---- | ---- | ------------------ | ---------------- | --------------- | --- | ------------------ | -------------- |
| Tool schema | `codex-rs/tools/src/session_model_tool.rs` | `LIST_AVAILABLE_MODELS_TOOL_NAME`, `UPDATE_SESSION_MODEL_TOOL_NAME`, factory functions | Defines only the existing list and write tools | Add `GET_CURRENT_SESSION_MODEL_TOOL_NAME`, one schema factory, and stable spec tests | This is the canonical model-visible tool-definition file for the session-model family | New built-in function tool `get_current_session_model` | `cargo test -p codex-tools` |
| Tool exports | `codex-rs/tools/src/lib.rs` | `pub use session_model_tool::...` | Exports only list/update tool symbols | Export the new tool constant and factory | Keep one public tool-definition surface for the family | New exported tool identity for runtime registration | `cargo test -p codex-tools` |
| Tool registry types | `codex-rs/tools/src/tool_registry_plan_types.rs` | `ToolHandlerKind` | No handler kind for the new read tool | Add one handler kind for `get_current_session_model` | Required to register the new tool into the model-visible plan | New handler-kind contract only | `cargo test -p codex-tools` |
| Tool registry plan | `codex-rs/tools/src/tool_registry_plan.rs` | session-model tool registration block | Registers only `list_available_models` and `update_session_model` | Register the new tool next to the existing session-model tools | The model can only call tools exposed through this plan | Three-tool family exposed to the model | `cargo test -p codex-core` via tool spec tests |
| Runtime handler | `codex-rs/core/src/tools/handlers/session_model.rs` | `ListAvailableModelsHandler`, `UpdateSessionModelHandler` | Only list and write handlers exist | Add `GetCurrentSessionModelHandler`, narrow result type, and handler tests; preserve current list-tool shape | Keep the new behavior in the same canonical handler module and reuse the root-thread gate | New read-only handler over live collaboration mode | `cargo test -p codex-core` |
| Runtime handler exports | `codex-rs/core/src/tools/handlers/mod.rs` | re-exports | Re-exports only list/update handlers | Re-export the new handler | Required for runtime tool builder wiring | No new user contract beyond handler availability | `cargo test -p codex-core` |
| Runtime tool builder | `codex-rs/core/src/tools/spec.rs` | handler registration switch | Registers only list/update session-model handlers | Register the new handler kind | Without this, the model-visible tool would have no runtime implementation | New runtime wiring only | `cargo test -p codex-core` |
| Tool exposure assertions | `codex-rs/core/src/tools/spec_tests.rs` | expected model-visible tool lists | Expected lists contain only `list_available_models` and `update_session_model` | Insert `get_current_session_model` in the expected tool lists for supported models/configs | Keeps the model-visible tool surface explicitly tested | Three-tool family in visible specs | `cargo test -p codex-core` |
| Evergreen runtime-boundary docs | `docs/skills.md` | runtime boundary section | Documents only list/update tools | Update to describe the three-tool family and the “skills instruct, tools execute, RPCs are client-facing” split | This is the shortest evergreen explanation surface for cold readers | Evergreen contract wording only | Manual doc review |
| Example skill package | `docs/examples/skills/session-model-control-example/SKILL.md` | skill contract and workflow | Assumes only two tools and uses list-tool output for implied current-state reads | Update workflow so `get_current_session_model` handles current-state questions and the list tool handles valid-choice questions | The example skill is the clearest reviewable workflow artifact for this feature | Three-tool workflow for skills | Manual doc review |
| Example skill reference | `docs/examples/skills/session-model-control-example/references/runtime-contract-and-examples.md` | runtime contract and examples | Assumes two runtime surfaces | Update examples and prerequisites to the additive three-tool family | Keeps concrete examples aligned with shipped behavior | Explicit “current / list / set” story | Manual doc review |
| Branch-history docs | `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md` | plan narrative and tool-family description | Describes the branch as a two-tool skill-facing surface | Append or revise the tool-family description to include the follow-on current-state read tool and the preserved compatibility posture | The user asked for the full branch truth to stay documented, not just the latest delta | Branch-history truth, not new runtime behavior | Manual doc review |
| Branch-history docs | `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16_WORKLOG.md` | implementation progress summary | Summarizes the landed branch before the follow-on tool/doc pass | Add a follow-on entry describing the new current-state tool and documentation sweep | Keeps the worklog honest about the branch’s full final state | Branch-history truth, not new runtime behavior | Manual doc review |

## 6.2 Migration notes

- Canonical owner path / shared code path:
  - Keep the session-model tool family centered in `codex-rs/tools/src/session_model_tool.rs` and `codex-rs/core/src/tools/handlers/session_model.rs`.
  - Keep live mutation owned exclusively by `apply_settings_update_and_emit_session_model_event(...)` in `codex-rs/core/src/codex.rs`.
- Deprecated APIs (if any):
  - None.
- Delete list (what must be removed; include superseded shims/parallel paths if any):
  - No code-path deletion is required.
  - Rewrite stale wording that implies the skill-facing family is only `list_available_models` plus `update_session_model`.
- Adjacent surfaces tied to the same contract family:
  - `docs/skills.md`
  - example skill package files under `docs/examples/skills/session-model-control-example/`
  - branch plan/worklog docs
  - existing external-client docs like `codex-rs/app-server/README.md`, which must at least remain truthful even if no wording change is needed
- Compatibility posture / cutover plan:
  - Additive explicit-read-tool change.
  - Preserve `list_available_models` output shape, including `current_model` and `reasoning_effort`.
  - Preserve `update_session_model`, `model/list`, `thread/model/set`, and `thread/model/updated` unchanged.
- Capability-replacing harnesses to delete or justify:
  - None. The existing built-in tool registry and skill prompt surfaces already own the workflow.
- Live docs/comments/instructions to update or delete:
  - Update any skill-facing docs or examples that currently blur “check current state” with “list valid choices” or blur tools with client RPCs.
- Behavior-preservation signals for refactors:
  - Tool-schema tests in `codex-rs/tools/src/session_model_tool.rs`
  - Handler tests in `codex-rs/core/src/tools/handlers/session_model.rs`
  - Tool-exposure assertions in `codex-rs/core/src/tools/spec_tests.rs`
  - Existing app-server/state/TUI tests remain the proof that the write path and surfaced live state were not disturbed

## Pattern Consolidation Sweep (anti-blinders; scoped by plan)

| Area | File / Symbol | Pattern to adopt | Why (drift prevented) | Proposed scope (include/defer/exclude/blocker question) |
| ---- | ------------- | ---------------- | ---------------------- | ------------------------------------- |
| Session-model tools | `codex-rs/tools/src/session_model_tool.rs` and `codex-rs/core/src/tools/handlers/session_model.rs` | Keep all three skill-visible session-model tools in one family module | Prevents a scattered read/list/write contract and keeps root-thread gating consistent | include |
| Model-visible registry | `codex-rs/tools/src/tool_registry_plan.rs`, `codex-rs/tools/src/tool_registry_plan_types.rs`, `codex-rs/core/src/tools/spec.rs`, `codex-rs/core/src/tools/spec_tests.rs` | Register and assert the new tool alongside the existing session-model tools everywhere the visible tool list is owned | Prevents the new tool from existing in schema code but not in the actual runtime tool list | include |
| Evergreen skills docs | `docs/skills.md` | Document the three-tool boundary explicitly | Prevents future readers from repeating the tool-versus-RPC confusion | include |
| Example skill docs | `docs/examples/skills/session-model-control-example/` | Make “current / list / set” the practical skill workflow | Prevents skills from continuing to use the catalog tool as the only documented current-state read path | include |
| Branch-history docs | `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md` and `_WORKLOG.md` | Update the branch narrative to the final three-tool state | Prevents the branch’s own history docs from freezing the older two-tool story | include |
| External app-server docs | `codex-rs/app-server/README.md` | Keep external RPC docs truthful without turning them into skill-tool docs | Prevents boundary drift while respecting audience fit | defer |
| Post-start write path | `codex-rs/core/src/codex.rs`, `codex-rs/protocol/src/protocol.rs`, `codex-rs/state/src/extract.rs`, TUI/app-server consumers | Leave the existing write/event/persistence path untouched | This follow-on is read-tool plus docs work, not a second mutation architecture pass | exclude |
<!-- arch_skill:block:call_site_audit:end -->

<!-- arch_skill:block:phase_plan:start -->
# 7) Depth-First Phased Implementation Plan (authoritative)

> Rule: systematic build, foundational first; split Section 7 into the best sequence of coherent self-contained units, optimizing for phases that are fully understood, credibly testable, compliance-complete, and safe to build on later. If two decompositions are both valid, bias toward more phases than fewer. `Work` explains the unit and is explanatory only for modern docs. `Checklist (must all be done)` is the authoritative must-do list inside the phase. `Exit criteria (all required)` names the exhaustive concrete done conditions the audit must validate. Resolve adjacent-surface dispositions and compatibility posture before writing the checklist. Before a phase is valid, run an obligation sweep and move every required promise from architecture, call-site audit, migration notes, delete lists, verification commitments, docs/comments propagation, approved bridges, and required helper follow-through into `Checklist` or `Exit criteria`. The authoritative checklist must name the actual chosen work, not unresolved branches or `if needed` placeholders. Refactors, consolidations, and shared-path extractions must preserve existing behavior with credible evidence proportional to the risk. For agent-backed systems, prefer prompt, grounding, and native-capability changes before new harnesses or scripts. No fallbacks/runtime shims - the system must work correctly or fail loudly (delete superseded paths). If a bridge is explicitly approved, timebox it and include removal work; otherwise plan either clean cutover or preservation work directly. Prefer programmatic checks per phase; defer manual/UI verification to finalization. Avoid negative-value tests and heuristic gates (deletion checks, visual constants, doc-driven gates, keyword or absence gates, repo-shape policing). Also: document new patterns/gotchas in code comments at the canonical boundary (high leverage, not comment spam).

## Phase 1 — Add the dedicated current-state tool in the canonical family

Status: COMPLETE 2026-04-17

* Goal:
  Land `get_current_session_model` as a read-only sibling inside the existing session-model tool family without disturbing the already-shipped list/write behavior or the post-start mutation path.
* Work:
  Add the new tool schema and the new runtime handler at the canonical session-model boundary so the feature exists as code before it is exposed broadly or documented as available.
* Checklist (must all be done):
  - Add `GET_CURRENT_SESSION_MODEL_TOOL_NAME` and one schema factory in `codex-rs/tools/src/session_model_tool.rs`.
  - Export the new tool constant and schema factory from `codex-rs/tools/src/lib.rs`.
  - Add a narrow result shape and `GetCurrentSessionModelHandler` in `codex-rs/core/src/tools/handlers/session_model.rs` that reads `session.collaboration_mode().await`.
  - Reuse `reject_subagent_thread(...)` so the new tool is root-thread-only.
  - Reuse `serialize_tool_result(...)` so the new tool returns the same JSON-text built-in tool output style as the existing session-model handlers.
  - Add handler-level tests for the new tool’s root-thread success path and subagent rejection path.
  - Add tool-schema tests for the new tool’s stable contract in `codex-rs/tools/src/session_model_tool.rs`.
  - Preserve the existing `list_available_models` and `update_session_model` handler logic unchanged.
* Verification (required proof):
  - `cargo test -p codex-tools`
  - `cargo test -p codex-core`
* Docs/comments (propagation; only if needed):
  - Add a brief boundary comment only if the new handler would otherwise leave the intentional overlap with `list_available_models` hard to understand in code review.
* Exit criteria (all required):
  - `get_current_session_model` exists as a built-in tool definition and runtime handler.
  - The new tool returns only current `model` and current `reasoning_effort`.
  - The new tool rejects subagent threads.
  - Existing list/write behavior remains unchanged by code inspection and passing tests.
* Rollback:
  Revert the new tool-definition and handler changes, leaving the pre-existing two-tool family intact.

## Phase 2 — Expose the tool to the model and lock runtime visibility

Status: COMPLETE 2026-04-17

* Goal:
  Make the new current-state tool actually visible and callable anywhere the runtime already exposes the session-model tool family.
* Work:
  Wire the new tool through the existing registry and visible-spec path so the model sees a three-tool family rather than a schema that exists only on disk.
* Checklist (must all be done):
  - Add one handler kind for `get_current_session_model` in `codex-rs/tools/src/tool_registry_plan_types.rs`.
  - Register the new tool alongside the existing session-model tools in `codex-rs/tools/src/tool_registry_plan.rs`.
  - Re-export the new handler from `codex-rs/core/src/tools/handlers/mod.rs`.
  - Register the new handler kind in `codex-rs/core/src/tools/spec.rs`.
  - Update `codex-rs/core/src/tools/spec_tests.rs` so all relevant expected tool lists include `get_current_session_model` alongside `list_available_models` and `update_session_model`.
  - Confirm this follow-on adds no new app-server RPC, event, persistence, or TUI state path.
* Verification (required proof):
  - `cargo test -p codex-core`
* Docs/comments (propagation; only if needed):
  - None beyond phase-local code truth unless a test name or helper comment needs to reflect the three-tool family explicitly.
* Exit criteria (all required):
  - The model-visible tool list contains `get_current_session_model`, `list_available_models`, and `update_session_model` in the same runtime family.
  - The runtime has one canonical registration path for the three-tool family.
  - No new external-client surface was introduced by this phase.
* Rollback:
  Revert the registry and visible-spec wiring, returning the runtime to the existing two-tool exposure.

## Phase 3 — Update evergreen skill-facing docs and the example workflow

Status: COMPLETE 2026-04-17

* Goal:
  Make the cold-reader and skill-authoring story explicit: current-state read, valid-choice read, and live write are separate operations, and skills use built-in tools rather than Rust internals or client RPCs.
* Work:
  Update the evergreen runtime-boundary doc and the example skill package so the practical workflow matches the shipped three-tool family.
* Checklist (must all be done):
  - Update `docs/skills.md` so it documents the three-tool family and the “skills instruct, tools execute, RPCs are client-facing” split.
  - Update `docs/examples/skills/session-model-control-example/SKILL.md` so current-state asks use `get_current_session_model`, valid-choice asks use `list_available_models`, and mutation asks use `update_session_model`.
  - Update `docs/examples/skills/session-model-control-example/references/runtime-contract-and-examples.md` so runtime prerequisites and examples reflect the additive three-tool family.
  - Ensure the example skill no longer implies that `list_available_models` is the only documented way to discover the current session state.
  - Ensure the evergreen docs continue to say that external clients use `model/list`, `thread/model/set`, and `thread/model/updated`, not the skill-facing tools.
* Verification (required proof):
  - Manual doc review against the implemented tool contracts and runtime boundary
* Docs/comments (propagation; only if needed):
  - These files are the propagation surface for this phase and must be updated directly.
* Exit criteria (all required):
  - A cold reader can distinguish current-state read, catalog read, and live write from the evergreen docs and example skill alone.
  - No touched evergreen skill-facing doc still blurs built-in tools with app-server RPCs.
  - No touched evergreen skill-facing doc still tells a two-tool story.
* Rollback:
  Revert the evergreen docs/example skill changes together so the explanation surface stays internally consistent.

## Phase 4 — Update branch-history docs and complete the final truth sweep

Status: COMPLETE 2026-04-17

* Goal:
  Bring the branch’s existing plan/worklog narrative and adjacent documentation into final sync so no stale “two-tool only” or API-key-only explanation remains.
* Work:
  Update the branch-history docs and run the final cross-doc truth sweep against the implemented code and existing signed-in OpenAI-account coverage.
* Checklist (must all be done):
  - Update `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16.md` so its skill-facing tool-family description reflects the final additive three-tool state and preserved compatibility posture.
  - Update `docs/SKILL_DRIVEN_SESSION_MODEL_AND_EFFORT_SWITCHING_2026-04-16_WORKLOG.md` with a follow-on entry for the current-state tool and the documentation convergence pass.
  - Verify `codex-rs/app-server/README.md` remains truthful for this follow-on; update it only if a cold reader would otherwise infer a wrong boundary story.
  - Make sure the final documentation set explicitly preserves the signed-in OpenAI-account support story somewhere a cold reader reviewing this branch work will see it.
  - Perform a final truth sweep across `docs/skills.md`, the example skill package, the branch plan, the branch worklog, and `codex-rs/app-server/README.md`.
  - Remove or rewrite any touched stale wording that implies only two skill-visible tools exist for this contract family.
* Verification (required proof):
  - Manual diff-based doc review against the implemented code and the existing ChatGPT/OpenAI-account coverage in `codex-rs/app-server/tests/suite/v2/thread_model_set.rs`
* Docs/comments (propagation; only if needed):
  - These files are the propagation surface for this phase and must be updated directly.
* Exit criteria (all required):
  - The full documentation set tells one consistent three-tool story.
  - No touched doc claims or implies that the feature is API-key-only.
  - No touched doc still freezes the older two-tool-only branch narrative.
* Rollback:
  Revert the branch-history and final doc-sweep edits together, keeping the pre-follow-on branch narrative intact if the convergence pass is abandoned.
<!-- arch_skill:block:phase_plan:end -->

# 8) Verification Strategy (common-sense; non-blocking)

## 8.1 Proofs

- Tool-schema and handler tests covering the new current-state read tool.
- Model-visible tool-spec tests proving the new tool is actually exposed alongside the existing list/write tools.
- Existing write-path coverage for list/set behavior, `SessionModelUpdated`, persistence, TUI rendering, and ChatGPT/OpenAI-account-authenticated app-server flow remains the regression proof that this follow-on did not disturb the already-landed mutation path.
- Documentation review against code truth, not only prose consistency.

## 8.2 Artefacts to inspect

- Tool specs and handlers.
- Tool registry and visible-spec assertions.
- App-server README and protocol descriptions.
- Example skill package.
- Existing architecture plan and worklog for the branch.

## 8.3 Explicit non-goals for verification

- No separate product redesign or provider-switch test matrix is needed for this follow-on change.

## 8.4 Executed verification on 2026-04-17

- `cargo test -p codex-tools`
- `cargo test -p codex-core`
- `cargo build -p codex-cli`
- `cargo build -p codex-rmcp-client --bin test_stdio_server`
- `cargo test -p codex-core --test all suite::prompt_caching::prompt_tools_are_consistent_across_requests -- --exact --nocapture`
- `cargo test -p codex-core --test all suite::search_tool::tool_search_indexes_only_enabled_non_app_mcp_tools -- --exact --nocapture`
- `cargo test -p codex-core --test all suite::cli_stream::responses_mode_stream_cli -- --exact --nocapture`
- `cargo test -p codex-core --test all suite::code_mode::code_mode_lists_global_scope_items -- --exact --nocapture`
- `cargo test -p codex-core` after the binary prebuild step

Notes:
- The first raw `cargo test -p codex-core` pass exposed one real expectation miss in `prompt_caching.rs`, which was fixed by adding the new tool names to the expected tool list.
- That first raw pass also showed that many `codex-core` integration tests expect the workspace binaries `codex` and `test_stdio_server` to exist in `target/debug`; prebuilding those binaries made the full `codex-core` crate pass meaningful and green.
- `codex-rs/app-server/README.md` was verified during the truth sweep and did not require edits for this follow-on because no app-server contract changed here.

# 9) Rollout / Ops / Telemetry

## 9.1 Rollout posture

- This is a branch-local convergence change on top of already-landed work, so rollout is primarily about keeping contracts and docs aligned rather than introducing a new staged deployment story.

## 9.2 Operational concerns

- The docs must make the signed-in OpenAI-account support explicit because that was a user requirement and is easy to lose in summary-only documentation.

## 9.3 Telemetry / observability

- No new telemetry is obviously required from the ask alone; confirm during research whether any event/doc examples should be surfaced more clearly instead.

# 10) Decision Log (append-only)

## 10.1 Draft decisions

- 2026-04-17: Scope clarification: this follow-on plan adds exactly one new tool for current-session state reads; `list_available_models` and `update_session_model` already exist and remain.
- 2026-04-17: Draft direction is to add a dedicated current-state read tool for skills instead of relying only on current-state fields embedded in `list_available_models`.
- 2026-04-17: The documentation scope is the full branch behavior, not only the new tool addition.
- 2026-04-17: The tool/runtime boundary must be stated explicitly everywhere relevant: skills instruct, model-visible tools execute, external clients use RPCs.
- 2026-04-17: Deep-dive locked the additive compatibility posture: keep `list_available_models` unchanged, add `get_current_session_model` as the explicit read surface, and leave app-server/state/TUI write-path code untouched.
- 2026-04-17: Deep-dive locked the documentation homes: `docs/skills.md` plus the example skill package are the primary evergreen explanation surfaces, while the existing branch plan/worklog must be updated to reflect the final three-tool state.
- 2026-04-17: Phase-plan split the work into four phases: canonical tool-family code, runtime exposure, evergreen skill-facing docs, and branch-history truth sweep, with no new RPC/event/state path in scope.
- 2026-04-17: Implementation landed `get_current_session_model` as a root-thread-only built-in tool in the existing session-model family, kept `list_available_models` and `update_session_model` intact, and updated the model-visible registry/tests to the final three-tool contract.
- 2026-04-17: The evergreen docs, example skill package, and branch-history docs were updated to the final three-tool story; `codex-rs/app-server/README.md` remained truthful and required no edits.
- 2026-04-17: Full scoped verification required prebuilding `codex` and `test_stdio_server` before rerunning `cargo test -p codex-core`; after that prebuild, the crate-level suite passed.
