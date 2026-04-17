# Runtime Contract And Examples

This example skill is review-first. It shows the intended in-skill workflow for checking the current root session model and reasoning effort, listing valid choices, and changing the current root session without inventing a second source of truth.

Skills are not executable host-side functions. The runtime exposes model-visible built-in tools, the model sees those tools in its tool list for the turn, and this skill tells the model when to call them.

## Job to be done

Let a user ask, inside one Codex session:

- what models and reasoning efforts are valid right now
- what the current session is using
- to switch the current root session to a specific or derived valid pair

## Anti-case

Do not use this skill to manage spawned agents, change providers, or edit config defaults.

## Runtime prerequisites

This skill assumes the host exposes two runtime surfaces to the model:

1. A built-in current-state read tool, `get_current_session_model`, that returns:
   - current model
   - current reasoning effort
2. A built-in catalog-read tool, `list_available_models`, backed by the live model catalog (`model/list` or the same underlying truth), that returns, for each model:
   - model slug
   - display name
   - default reasoning effort
   - supported reasoning efforts
3. A built-in root-thread mutation tool:
   - `update_session_model`

If either surface is missing, the correct behavior is to fail loud and say what is unavailable.

## Decision rules

- Catalog is the authority for valid models and supported reasoning efforts.
- `get_current_session_model` is the explicit authority for what the root session is using right now.
- The skill should never invent or normalize an effort silently.
- If the user requests `model + reasoning_effort`, validate the exact pair before mutating.
- If the user requests only `model`, call `update_session_model` with only `model`.
- If the runtime rejects that model-only change because the carried-over effort is invalid, report the supported efforts for that model and ask for explicit user intent or honor a semantic ask like "highest supported".
- If the user asks for a semantic target:
  - "highest thinking" means choose the highest supported reasoning effort from the live catalog for the chosen model
  - "lowest thinking" means choose the lowest supported reasoning effort from the live catalog for the chosen model
  - "cheapest" or "faster" should only be honored if the live catalog or surrounding runtime exposes enough information to rank choices honestly; otherwise the skill should not guess
- After success, always report the current-turn versus next-turn effect explicitly.

## Example interactions

### Example 1: read current settings

User ask:

`What model and thinking level is this session using right now?`

Expected behavior:

1. Call `get_current_session_model`.
2. Return the current model and current reasoning effort directly.

### Example 2: list valid choices

User ask:

`What models and thinking levels can this session use right now?`

Expected behavior:

1. Call `list_available_models`.
2. Return a concise list such as:
   - `gpt-5.4` — default `medium`; supports `low`, `medium`, `high`
   - `gpt-5.2-codex` — default `medium`; supports `minimal`, `low`, `medium`

### Example 3: exact change

User ask:

`Switch this session to gpt-5.4 high.`

Expected behavior:

1. Call `list_available_models`.
2. Confirm `gpt-5.4 + high` is valid.
3. Call `update_session_model` with:
   - `model: "gpt-5.4"`
   - `reasoning_effort: "high"`
4. Report the applied pair and whether the active in-flight turn keeps the old settings.

### Example 4: model-only change that fails on carried effort

User ask:

`Switch this session to gpt-5.2-codex.`

Expected behavior:

1. Call `list_available_models`.
2. Call `update_session_model` with only the new model.
3. If the runtime rejects because the current carried-over effort is unsupported, reply with the exact valid efforts for `gpt-5.2-codex` and do not silently pick one.

### Example 5: derived explicit choice

User ask:

`Move this session to gpt-5.2-codex with the highest supported thinking level.`

Expected behavior:

1. Call `list_available_models`.
2. Pick the highest supported reasoning effort for `gpt-5.2-codex` from the catalog.
3. State the explicit chosen pair.
4. Call `update_session_model` with that explicit pair.
