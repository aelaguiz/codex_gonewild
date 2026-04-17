# Skills

For information about skills, refer to [this documentation](https://developers.openai.com/codex/skills).

## Runtime Boundary

Skills are instruction text, not host-executable code. When a skill needs to
interact with Codex runtime features, it does so by telling the model to call
model-visible built-in tools that Codex exposes in the current session.

For example, the skill-driven session model switching work in this repo uses:

- `list_available_models` to read the live catalog of valid models and
  supported reasoning efforts for the current session.
- `update_session_model` to update the root thread's live session `model`
  and/or `reasoning_effort`.

That is distinct from app-server RPC surfaces like `model/list` or
`thread/model/set`, which are for external clients rather than skills.
