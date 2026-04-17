---
name: session-model-control-example
description: "Check the current session model and reasoning effort, list valid models and reasoning-effort choices, explain compatibility, and change the root session model on demand. Use when a user asks what the session is using right now, what models or thinking levels are available right now, wants to switch the current session to a specific model or effort, or wants a faster, cheaper, or stronger compatible setting. Not for subagent model selection, provider switching, or config edits."
---

# Session Model Control Example

Use this skill when the user wants to inspect or change the current root session's working model or reasoning effort.

This skill does not execute host code by itself. It is instruction text for the model. It only works when the runtime exposes model-visible built-in tools that the model can see in its tool list and call for this turn.

Canonical user asks:

- "What model and thinking level is this session using right now?"
- "What models and thinking levels can this session use right now?"
- "Switch this session to `gpt-5.4` with `high` reasoning."
- "Move this session to a cheaper model and tell me which thinking levels it supports."

## When not to use

- The user wants to change a spawned agent or subagent rather than the root session.
- The user wants to switch `model_provider` or edit global defaults in config files.
- The runtime does not expose all three of:
  - the built-in current-state tool `get_current_session_model`
  - the built-in catalog-read tool `list_available_models`
  - the built-in write tool `update_session_model`
- The session's model or reasoning settings are role-locked and cannot be changed.

## Non-negotiables

- Read current settings and valid models from the live runtime surfaces. Do not guess from memory.
- Treat `get_current_session_model`, `list_available_models`, and `update_session_model` as the only host-call surfaces this skill is allowed to use for the workflow.
- Treat the root session as the only mutation target.
- Never change `model_provider`.
- Do not invent a reasoning effort when the user did not ask for one.
- If the user changes only the model, call the mutation tool with only `model` and let the runtime preserve-or-reject the current effort per its own validation rules.
- If the user asks for a semantic target like "highest thinking supported" or "cheapest valid option", derive the choice from the live catalog and state the explicit chosen model and reasoning effort.
- Fail loud when the requested pair is invalid or when the runtime prerequisites are missing.

## First move

1. Classify the request as `current`, `list`, `change`, or `choose-from-catalog`.
2. If the user asked what the session is using right now, call `get_current_session_model`.
3. Call `list_available_models` before making any claim about valid models or reasoning efforts.
4. If the request includes a change, validate the requested pair against the `list_available_models` result before calling `update_session_model`.

## Workflow

1. If the user asked for the current active session settings, call the built-in tool `get_current_session_model` and report:
   - current model
   - current reasoning effort
2. Call the built-in tool `list_available_models` when the user asks for valid choices or when a change request needs validation.
3. Summarize the relevant models with:
   - model slug
   - display name if available
   - default reasoning effort
   - supported reasoning efforts
4. If the user asked for a specific model and effort, validate that exact pair against the `list_available_models` output and then call the built-in tool `update_session_model`.
5. If the user asked for a model only, call `update_session_model` with only `model`.
6. If the runtime rejects the change because the carried-over reasoning effort is invalid for the requested model, report the exact failure and list the valid reasoning efforts for that model.
7. If the user asked for a semantic choice like "highest thinking" or "cheapest", pick from the catalog, say what you chose, and then call `update_session_model` with the explicit chosen pair.
8. After any successful mutation, report:
   - applied model
   - applied reasoning effort
   - whether the current in-flight turn keeps its old settings
   - what the next turn will use

## Output expectations

- `current`: the exact current model and reasoning effort for the root session.
- `list`: concise catalog view of valid models and supported reasoning efforts for this session.
- `change`: the exact applied model and reasoning effort, plus the active-turn versus next-turn effect.
- `failure`: the exact invalid request, why it failed, and the valid reasoning efforts for the target model.

## Reference map

- `references/runtime-contract-and-examples.md` - runtime prerequisites, decision rules, and example interactions
