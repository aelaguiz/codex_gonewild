# Chat Completions API Investigation for Non-OpenAI Providers

## Overview

This document captures investigation work done to support non-OpenAI providers (Gemini, Anthropic) through codex-rs's Chat Completions API pathway. The goal was to enable `WireApi::Chat` models to work correctly with the existing codex-rs infrastructure.

## Current Architecture

### API Routing (wire_api.rs)

```rust
pub enum WireApi {
    Responses,  // OpenAI Responses API (default for OpenAI models)
    Chat,       // OpenAI Chat Completions API (used for non-OpenAI providers)
}
```

Models route based on provider:
- **OpenAI models** (gpt-*, o1-*, o3-*): Use `WireApi::Responses`
- **Anthropic models** (claude-*): Use `WireApi::Chat` → routes to `api.anthropic.com/v1/chat/completions`
- **Gemini models** (gemini-*): Use `WireApi::Chat` → routes to `generativelanguage.googleapis.com/v1beta/openai/chat/completions`

### Chat Completions Implementation (chat_completions.rs)

The `exec_chat_completions_turn` function handles streaming SSE responses from the Chat Completions endpoint. Key payload structure:

```rust
let payload = json!({
    "model": model_family.slug,
    "messages": messages,
    "stream": true,
    "tools": tools_json,
    "max_tokens": 64000,
    "stream_options": {"include_usage": true},
});
```

## Issues Discovered & Fixes Applied

### Issue 1: JSON Arguments Parsing

**Problem**: Tool call arguments were being parsed incorrectly when they arrived as pre-parsed JSON objects vs JSON strings.

**Symptom**: Tool calls would fail with JSON parsing errors.

**Fix**: Added logic to handle both cases:
```rust
// Arguments can arrive as string OR as already-parsed object
let arguments_value = func.get("arguments").unwrap_or(&serde_json::Value::Null);
let arguments_str = match arguments_value {
    serde_json::Value::String(s) => s.clone(),
    other => serde_json::to_string(other).unwrap_or_default(),
};
```

### Issue 2: Anthropic tool_call_id Mismatch

**Problem**: Anthropic's Chat Completions API requires tool responses to have `tool_call_id` matching exactly with the assistant's `tool_calls[].id`.

**Symptom**: Error message: "tool_call_ids did not have response messages"

**Root Cause**: The streaming parser was generating its own sequential IDs (`chatcmpl-tool-0`, `chatcmpl-tool-1`) instead of using the actual IDs from the API response.

**Fix**: Extract and use the actual `id` field from tool call chunks:
```rust
if let Some(id) = tool_call_obj.get("id").and_then(|v| v.as_str()) {
    current_tool_call_id = Some(id.to_string());
}
// Later use current_tool_call_id.unwrap_or_else(|| format!("chatcmpl-tool-{idx}"))
```

### Issue 3: Anthropic thought_signature Support

**Problem**: Anthropic returns `thought_signature` field for extended thinking, which wasn't being captured.

**Fix**: Added parsing for thought_signature in streaming chunks (though this may need further work for proper integration).

### Issue 4: Gemini Empty Responses (0 output tokens)

**Problem**: Gemini 3 Pro Preview returned empty responses when used with large system prompts (~33K+ tokens).

**Symptom**: `output_tokens: 0` in usage data, no actual response content.

**Root Cause**: Gemini 3 Pro Preview is a "thinking" model. It uses output tokens for internal reasoning. With `max_tokens: 16384` and a large system prompt, the thinking budget consumed all available tokens, leaving nothing for the actual response.

**Fix**: Increased `max_tokens` from 16384 to 64000 (Gemini's max is 64K).

**Important Note**: Gemini does NOT yet support the `reasoning_effort` parameter to control thinking token allocation. This is a known constraint from Google.

## Model Specifications

### Gemini 3 Pro Preview
- Input: 1,000,000 tokens
- Output: 64,000 tokens (includes thinking tokens)
- Thinking: ON by default (cannot be disabled via API)
- `reasoning_effort` parameter: NOT YET SUPPORTED
- Endpoint: `generativelanguage.googleapis.com/v1beta/openai/chat/completions`

### Claude Models (via Chat Completions)
- Endpoint: `api.anthropic.com/v1/chat/completions`
- Supports extended thinking with `thought_signature`
- Strict about tool_call_id matching

### GPT-5.1 (for reference)
- Output: 128,000 tokens
- Supports `reasoning_effort`: low (1K), medium (8K), high (24K)

## Message Structure Requirements

### Anthropic Chat Completions API

Tested message structure requirements:

| Scenario | Works? |
|----------|--------|
| Single tool call with matching response | ✓ |
| Tool call with mismatched response ID | ✗ |
| Separate assistant messages (content, then tool_calls) | ✗ |
| Combined assistant message (content + tool_calls together) | ✓ |
| Multiple tool calls in ONE assistant message | ✓ |
| Multiple tool calls in SEPARATE assistant messages | ✗ |
| Tool response before its call (wrong order) | ✗ |

**Key insight**: Anthropic requires:
1. Tool calls and content must be in the SAME assistant message (not separate messages)
2. Tool responses must follow their corresponding tool call
3. Tool call IDs must match exactly

## Current State

### What's Working
- Gemini 3 Pro Preview with large system prompts (after max_tokens fix)
- Basic tool calling for Gemini
- JSON argument parsing for both string and object formats

### What's Broken
- **Opus (Claude) is now broken** - needs investigation
- Possibly message structure issues similar to what was discovered in Anthropic testing
- May be fundamental disconnect in how codex-rs builds conversation history

## Suspected Root Causes for Ongoing Issues

1. **Message History Construction**: codex-rs may be constructing message history in a way that violates provider-specific requirements (e.g., splitting assistant messages)

2. **Tool Call ID Tracking**: The ID tracking fix may not be complete or may have introduced regressions

3. **Provider-Specific Normalization**: Each provider (Anthropic, Gemini) has different requirements for message structure that aren't being handled uniformly

## Files Modified

- `/Users/aelaguiz/workspace/codex/codex-rs/core/src/chat_completions.rs`
  - max_tokens: 16384 → 64000
  - JSON arguments parsing fix
  - Tool call ID extraction from stream

- `/Users/aelaguiz/workspace/codex/codex-rs/core/src/client.rs`
  - Various call site updates (some reverted)

## Test Scripts Created

- `/Users/aelaguiz/workspace/pokerskill_agents/scripts/test_anthropic_message_structure.py` - Tests Anthropic's message structure requirements directly against the API

- `/tmp/test_gemini_tools.py` - Tests Gemini's OpenAI-compatible API with tools

## Next Steps to Investigate

1. **Examine message history construction** in codex-rs to see how it builds the messages array for Chat Completions

2. **Add logging** to see exactly what messages are being sent to each provider

3. **Compare with working implementations** - look at how other tools (e.g., LiteLLM) handle provider normalization

4. **Test each provider in isolation** with minimal reproduction cases to identify the exact failure modes

## References

- Gemini API docs: https://ai.google.dev/gemini-api/docs/openai
- Anthropic Chat Completions: https://docs.anthropic.com/en/api/openai-sdk
- codex-rs source: `/Users/aelaguiz/workspace/codex/codex-rs/`
