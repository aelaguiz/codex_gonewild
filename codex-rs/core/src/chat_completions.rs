use std::time::Duration;

use crate::ModelProviderInfo;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::default_client::CodexHttpClient;
use crate::error::CodexErr;
use crate::error::ConnectionFailedError;
use crate::error::ResponseStreamFailed;
use crate::error::Result;
use crate::error::RetryLimitReachedError;
use crate::error::UnexpectedResponseError;
use crate::model_family::ModelFamily;
use crate::tools::spec::create_tools_json_for_chat_completions_api;
use crate::util::backoff;
use bytes::Bytes;
use codex_otel::otel_event_manager::OtelEventManager;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use futures::TryStreamExt;
use reqwest::StatusCode;
use serde_json::json;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

/// Implementation for the classic Chat Completions API.
pub(crate) async fn stream_chat_completions(
    prompt: &Prompt,
    model_family: &ModelFamily,
    client: &CodexHttpClient,
    provider: &ModelProviderInfo,
    otel_event_manager: &OtelEventManager,
    session_source: &SessionSource,
) -> Result<ResponseStream> {
    if prompt.output_schema.is_some() {
        return Err(CodexErr::UnsupportedOperation(
            "output_schema is not supported for Chat Completions API".to_string(),
        ));
    }

    // Build messages array
    let mut messages = Vec::<serde_json::Value>::new();

    let full_instructions = prompt.get_full_instructions(model_family);
    messages.push(json!({"role": "system", "content": full_instructions}));

    let input = prompt.get_formatted_input();

    // Pre-scan: map Reasoning blocks to the adjacent assistant anchor after the last user.
    // - If the last emitted message is a user message, drop all reasoning.
    // - Otherwise, for each Reasoning item after the last user message, attach it
    //   to the immediate previous assistant message (stop turns) or the immediate
    //   next assistant anchor (tool-call turns: function/local shell call, or assistant message).
    let mut reasoning_by_anchor_index: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();

    // Determine the last role that would be emitted to Chat Completions.
    let mut last_emitted_role: Option<&str> = None;
    for item in &input {
        match item {
            ResponseItem::Message { role, .. } => last_emitted_role = Some(role.as_str()),
            ResponseItem::FunctionCall { .. } | ResponseItem::LocalShellCall { .. } => {
                last_emitted_role = Some("assistant")
            }
            ResponseItem::FunctionCallOutput { .. } => last_emitted_role = Some("tool"),
            ResponseItem::Reasoning { .. } | ResponseItem::Other => {}
            ResponseItem::CustomToolCall { .. } => {}
            ResponseItem::CustomToolCallOutput { .. } => {}
            ResponseItem::WebSearchCall { .. } => {}
            ResponseItem::GhostSnapshot { .. } => {}
            ResponseItem::CompactionSummary { .. } => {}
        }
    }

    // Find the last user message index in the input.
    let mut last_user_index: Option<usize> = None;
    for (idx, item) in input.iter().enumerate() {
        if let ResponseItem::Message { role, .. } = item
            && role == "user"
        {
            last_user_index = Some(idx);
        }
    }

    // Attach reasoning only if the conversation does not end with a user message.
    if !matches!(last_emitted_role, Some("user")) {
        for (idx, item) in input.iter().enumerate() {
            // Only consider reasoning that appears after the last user message.
            if let Some(u_idx) = last_user_index
                && idx <= u_idx
            {
                continue;
            }

            if let ResponseItem::Reasoning {
                content: Some(items),
                ..
            } = item
            {
                let mut text = String::new();
                for entry in items {
                    match entry {
                        ReasoningItemContent::ReasoningText { text: segment }
                        | ReasoningItemContent::Text { text: segment } => text.push_str(segment),
                    }
                }
                if text.trim().is_empty() {
                    continue;
                }

                // Prefer immediate previous assistant message (stop turns)
                let mut attached = false;
                if idx > 0
                    && let ResponseItem::Message { role, .. } = &input[idx - 1]
                    && role == "assistant"
                {
                    reasoning_by_anchor_index
                        .entry(idx - 1)
                        .and_modify(|v| v.push_str(&text))
                        .or_insert(text.clone());
                    attached = true;
                }

                // Otherwise, attach to immediate next assistant anchor (tool-calls or assistant message)
                if !attached && idx + 1 < input.len() {
                    match &input[idx + 1] {
                        ResponseItem::FunctionCall { .. } | ResponseItem::LocalShellCall { .. } => {
                            reasoning_by_anchor_index
                                .entry(idx + 1)
                                .and_modify(|v| v.push_str(&text))
                                .or_insert(text.clone());
                        }
                        ResponseItem::Message { role, .. } if role == "assistant" => {
                            reasoning_by_anchor_index
                                .entry(idx + 1)
                                .and_modify(|v| v.push_str(&text))
                                .or_insert(text.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Track last assistant text we emitted to avoid duplicate assistant messages
    // in the outbound Chat Completions payload (can happen if a final
    // aggregated assistant message was recorded alongside an earlier partial).
    let mut last_assistant_text: Option<String> = None;

    // Buffer for merging consecutive assistant items (text content + tool calls)
    // into a single assistant message. This is required by Anthropic's Chat Completions
    // API which expects all tool_calls from one model turn to be in ONE assistant message.
    let mut pending_assistant_content: Option<String> = None;
    let mut pending_tool_calls: Vec<serde_json::Value> = Vec::new();
    let mut pending_reasoning: Option<String> = None;

    // Helper closure to flush buffered assistant items as a single message
    let flush_pending_assistant = |messages: &mut Vec<serde_json::Value>,
                                   content: &mut Option<String>,
                                   tool_calls: &mut Vec<serde_json::Value>,
                                   reasoning: &mut Option<String>,
                                   last_text: &mut Option<String>| {
        if content.is_none() && tool_calls.is_empty() {
            return;
        }

        let content_val = content.take();

        // Skip exact-duplicate assistant messages
        if let Some(text) = &content_val {
            if let Some(prev) = last_text.as_ref() {
                if prev == text && tool_calls.is_empty() {
                    return;
                }
            }
            *last_text = Some(text.clone());
        }

        let mut msg = if tool_calls.is_empty() {
            // Text-only assistant message
            json!({
                "role": "assistant",
                "content": content_val.unwrap_or_default()
            })
        } else {
            // Assistant message with tool calls (and optional content)
            let tool_calls_arr: Vec<serde_json::Value> = tool_calls.drain(..).collect();
            tracing::debug!(
                "Flushing merged assistant message with {} tool_calls",
                tool_calls_arr.len()
            );
            json!({
                "role": "assistant",
                "content": content_val,
                "tool_calls": tool_calls_arr
            })
        };

        if let Some(r) = reasoning.take() {
            if let Some(obj) = msg.as_object_mut() {
                obj.insert("reasoning".to_string(), json!(r));
            }
        }

        messages.push(msg);
    };

    for (idx, item) in input.iter().enumerate() {
        match item {
            ResponseItem::Message { role, content, .. } => {
                // Build content either as a plain string (typical for assistant text)
                // or as an array of content items when images are present (user/tool multimodal).
                let mut text = String::new();
                let mut items: Vec<serde_json::Value> = Vec::new();
                let mut saw_image = false;

                for c in content {
                    match c {
                        ContentItem::InputText { text: t }
                        | ContentItem::OutputText { text: t } => {
                            text.push_str(t);
                            items.push(json!({"type":"text","text": t}));
                        }
                        ContentItem::InputImage { image_url } => {
                            saw_image = true;
                            items.push(json!({"type":"image_url","image_url": {"url": image_url}}));
                        }
                    }
                }

                if role == "assistant" {
                    // Buffer assistant content to merge with subsequent tool calls
                    if let Some(existing) = pending_assistant_content.take() {
                        pending_assistant_content = Some(existing + &text);
                    } else {
                        pending_assistant_content = Some(text.clone());
                    }
                    // Collect reasoning for this item
                    if let Some(reasoning) = reasoning_by_anchor_index.get(&idx) {
                        if let Some(existing) = pending_reasoning.take() {
                            pending_reasoning = Some(existing + reasoning);
                        } else {
                            pending_reasoning = Some(reasoning.clone());
                        }
                    }
                } else {
                    // Non-assistant message: flush any pending assistant items first
                    flush_pending_assistant(
                        &mut messages,
                        &mut pending_assistant_content,
                        &mut pending_tool_calls,
                        &mut pending_reasoning,
                        &mut last_assistant_text,
                    );

                    // For user messages, if an image is present, send an array of content items.
                    let content_value = if saw_image {
                        json!(items)
                    } else {
                        json!(text)
                    };

                    messages.push(json!({"role": role, "content": content_value}));
                }
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                thought_signature,
                ..
            } => {
                tracing::debug!(
                    "Buffering FunctionCall: name={} call_id={} has_thought_sig={}",
                    name,
                    call_id,
                    thought_signature.is_some()
                );
                // Buffer tool call to merge with other consecutive assistant items
                // Include thought_signature for Gemini thinking models
                let mut tool_call_json = json!({
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
                    }
                });
                if let Some(sig) = thought_signature {
                    tool_call_json["extra_content"] = json!({
                        "google": {
                            "thought_signature": sig
                        }
                    });
                }
                pending_tool_calls.push(tool_call_json);
                // Collect reasoning for this item
                if let Some(reasoning) = reasoning_by_anchor_index.get(&idx) {
                    if let Some(existing) = pending_reasoning.take() {
                        pending_reasoning = Some(existing + reasoning);
                    } else {
                        pending_reasoning = Some(reasoning.clone());
                    }
                }
            }
            ResponseItem::LocalShellCall {
                id,
                call_id: _,
                status,
                action,
            } => {
                // Buffer local shell call as a standard function call.
                // Non-OpenAI providers (Gemini, Anthropic) reject type: "local_shell_call".
                // Convert to type: "function" with shell details in arguments.
                let shell_args = json!({
                    "status": status,
                    "action": action,
                });
                pending_tool_calls.push(json!({
                    "id": id.clone().unwrap_or_else(|| "".to_string()),
                    "type": "function",
                    "function": {
                        "name": "shell",
                        "arguments": shell_args.to_string(),
                    }
                }));
                // Collect reasoning for this item
                if let Some(reasoning) = reasoning_by_anchor_index.get(&idx) {
                    if let Some(existing) = pending_reasoning.take() {
                        pending_reasoning = Some(existing + reasoning);
                    } else {
                        pending_reasoning = Some(reasoning.clone());
                    }
                }
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                // Tool output: flush any pending assistant items first
                flush_pending_assistant(
                    &mut messages,
                    &mut pending_assistant_content,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    &mut last_assistant_text,
                );

                tracing::debug!(
                    "Serializing FunctionCallOutput: tool_call_id={}",
                    call_id
                );
                // Prefer structured content items when available (e.g., images)
                // otherwise fall back to the legacy plain-string content.
                let content_value = if let Some(items) = &output.content_items {
                    let mapped: Vec<serde_json::Value> = items
                        .iter()
                        .map(|it| match it {
                            FunctionCallOutputContentItem::InputText { text } => {
                                json!({"type":"text","text": text})
                            }
                            FunctionCallOutputContentItem::InputImage { image_url } => {
                                json!({"type":"image_url","image_url": {"url": image_url}})
                            }
                        })
                        .collect();
                    json!(mapped)
                } else {
                    json!(output.content)
                };

                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content_value,
                }));
            }
            ResponseItem::CustomToolCall {
                id,
                call_id: _,
                name,
                input,
                status: _,
            } => {
                // Buffer custom tool call as a standard function call.
                // Non-OpenAI providers (Gemini, Anthropic) reject type: "custom".
                // Convert to type: "function" with input as arguments.
                let args_str = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                pending_tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args_str,
                    }
                }));
            }
            ResponseItem::CustomToolCallOutput { call_id, output } => {
                // Tool output: flush any pending assistant items first
                flush_pending_assistant(
                    &mut messages,
                    &mut pending_assistant_content,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    &mut last_assistant_text,
                );

                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output,
                }));
            }
            ResponseItem::GhostSnapshot { .. } => {
                // Ghost snapshots annotate history but are not sent to the model.
                continue;
            }
            ResponseItem::Reasoning { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::Other
            | ResponseItem::CompactionSummary { .. } => {
                // Omit these items from the conversation history.
                continue;
            }
        }
    }

    // Flush any remaining buffered assistant items
    flush_pending_assistant(
        &mut messages,
        &mut pending_assistant_content,
        &mut pending_tool_calls,
        &mut pending_reasoning,
        &mut last_assistant_text,
    );

    let tools_json = create_tools_json_for_chat_completions_api(&prompt.tools)?;

    // Use high max_tokens for thinking models like Gemini 3 Pro Preview.
    // Thinking tokens are counted against max_tokens, so low values result in
    // no actual response. Model limits:
    //   - Gemini 3 Pro: 64,000 max output (thinking on by default)
    //   - GPT-5.1: 128,000 max output (reasoning auto-routed)
    //   - Claude Opus 4.5: 64,000 max output (extended thinking opt-in)
    // Request usage data in streaming responses (supported by Gemini, OpenAI, etc.)
    let payload = json!({
        "model": model_family.slug,
        "messages": messages,
        "stream": true,
        "tools": tools_json,
        "max_tokens": if model_family.slug.starts_with("gemini") || model_family.slug.contains("claude-opus-4") { 64000 } else if model_family.slug.contains("claude-3-5") { 8192 } else { 4096 },
        "stream_options": {"include_usage": true},
    });

    debug!(
        "POST to {}: {}",
        provider.get_full_url(&None),
        payload.to_string()
    );

    let mut attempt = 0;
    let max_retries = provider.request_max_retries();
    loop {
        attempt += 1;

        let mut req_builder = provider.create_request_builder(client, &None).await?;

        // Include subagent header only for subagent sessions.
        if let SessionSource::SubAgent(sub) = session_source.clone() {
            let subagent = if let SubAgentSource::Other(label) = sub {
                label
            } else {
                serde_json::to_value(&sub)
                    .ok()
                    .and_then(|v| v.as_str().map(std::string::ToString::to_string))
                    .unwrap_or_else(|| "other".to_string())
            };
            req_builder = req_builder.header("x-openai-subagent", subagent);
        }

        let res = otel_event_manager
            .log_request(attempt, || {
                req_builder
                    .header(reqwest::header::ACCEPT, "text/event-stream")
                    .json(&payload)
                    .send()
            })
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => {
                let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
                let stream = resp.bytes_stream().map_err(|e| {
                    CodexErr::ResponseStreamFailed(ResponseStreamFailed {
                        source: e,
                        request_id: None,
                    })
                });
                tokio::spawn(process_chat_sse(
                    stream,
                    tx_event,
                    provider.stream_idle_timeout(),
                    otel_event_manager.clone(),
                ));
                return Ok(ResponseStream { rx_event });
            }
            Ok(res) => {
                let status = res.status();
                if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) {
                    let body = (res.text().await).unwrap_or_default();
                    return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                        status,
                        body,
                        request_id: None,
                    }));
                }

                if attempt > max_retries {
                    return Err(CodexErr::RetryLimit(RetryLimitReachedError {
                        status,
                        request_id: None,
                    }));
                }

                let retry_after_secs = res
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());

                let delay = retry_after_secs
                    .map(|s| Duration::from_millis(s * 1_000))
                    .unwrap_or_else(|| backoff(attempt));
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                if attempt > max_retries {
                    return Err(CodexErr::ConnectionFailed(ConnectionFailedError {
                        source: e,
                    }));
                }
                let delay = backoff(attempt);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn append_assistant_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    assistant_item: &mut Option<ResponseItem>,
    text: String,
) {
    if assistant_item.is_none() {
        let item = ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![],
        };
        *assistant_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }

    if let Some(ResponseItem::Message { content, .. }) = assistant_item {
        content.push(ContentItem::OutputText { text: text.clone() });
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputTextDelta(text.clone())))
            .await;
    }
}

async fn append_reasoning_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    reasoning_item: &mut Option<ResponseItem>,
    text: String,
) {
    if reasoning_item.is_none() {
        let item = ResponseItem::Reasoning {
            id: String::new(),
            summary: Vec::new(),
            content: Some(vec![]),
            encrypted_content: None,
        };
        *reasoning_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }

    if let Some(ResponseItem::Reasoning {
        content: Some(content),
        ..
    }) = reasoning_item
    {
        let content_index = content.len() as i64;
        content.push(ReasoningItemContent::ReasoningText { text: text.clone() });

        let _ = tx_event
            .send(Ok(ResponseEvent::ReasoningContentDelta {
                delta: text.clone(),
                content_index,
            }))
            .await;
    }
}
/// Lightweight SSE processor for the Chat Completions streaming format. The
/// output is mapped onto Codex's internal [`ResponseEvent`] so that the rest
/// of the pipeline can stay agnostic of the underlying wire format.
async fn process_chat_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    idle_timeout: Duration,
    otel_event_manager: OtelEventManager,
) where
    S: Stream<Item = Result<Bytes>> + Unpin,
{
    let mut stream = stream.eventsource();

    // State to accumulate function calls across streaming chunks.
    // OpenAI may split the `arguments` string over multiple `delta` events
    // until the chunk whose `finish_reason` is `tool_calls` is emitted. We
    // keep collecting the pieces here and forward `ResponseItem::FunctionCall`
    // items once the calls are complete.
    //
    // IMPORTANT: When multiple tools are called in parallel, each has a distinct
    // `index` in the streaming chunks. We must track each tool call separately
    // by its index to avoid concatenating arguments across different calls.
    #[derive(Default, Clone)]
    struct FunctionCallState {
        name: Option<String>,
        arguments: String,
        call_id: Option<String>,
        /// Gemini thought_signature - must be echoed back for thinking models.
        thought_signature: Option<String>,
    }

    let mut fn_call_states: std::collections::HashMap<i64, FunctionCallState> =
        std::collections::HashMap::new();
    let mut assistant_item: Option<ResponseItem> = None;
    let mut reasoning_item: Option<ResponseItem> = None;
    let mut accumulated_usage: Option<TokenUsage> = None;

    loop {
        let start = std::time::Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        let duration = start.elapsed();
        otel_event_manager.log_sse_event(&response, duration);

        let sse = match response {
            Ok(Some(Ok(ev))) => ev,
            Ok(Some(Err(e))) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream(e.to_string(), None)))
                    .await;
                return;
            }
            Ok(None) => {
                // Stream closed gracefully – emit Completed with dummy id.
                let _ = tx_event
                    .send(Ok(ResponseEvent::Completed {
                        response_id: String::new(),
                        token_usage: accumulated_usage,
                    }))
                    .await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream(
                        "idle timeout waiting for SSE".into(),
                        None,
                    )))
                    .await;
                return;
            }
        };

        // OpenAI Chat streaming sends a literal string "[DONE]" when finished.
        if sse.data.trim() == "[DONE]" {
            // Emit any finalized items before closing so downstream consumers receive
            // terminal events for both assistant content and raw reasoning.
            if let Some(item) = assistant_item {
                let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
            }

            if let Some(item) = reasoning_item {
                let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
            }

            let _ = tx_event
                .send(Ok(ResponseEvent::Completed {
                    response_id: String::new(),
                    token_usage: accumulated_usage,
                }))
                .await;
            return;
        }

        // Parse JSON chunk
        let chunk: serde_json::Value = match serde_json::from_str(&sse.data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        trace!("chat_completions received SSE chunk: {chunk:?}");

        // Extract usage data if present (when stream_options.include_usage is set)
        // Gemini/OpenAI format: {"usage": {"prompt_tokens": N, "completion_tokens": N, "total_tokens": N}}
        if let Some(usage) = chunk.get("usage") {
            let input_tokens = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output_tokens = usage
                .get("completion_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let total_tokens = usage
                .get("total_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            accumulated_usage = Some(TokenUsage {
                input_tokens,
                cached_input_tokens: 0,
                output_tokens,
                reasoning_output_tokens: 0,
                total_tokens,
            });
        }

        let choice_opt = chunk.get("choices").and_then(|c| c.get(0));

        if let Some(choice) = choice_opt {
            // Handle assistant content tokens as streaming deltas.
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
                && !content.is_empty()
            {
                append_assistant_text(&tx_event, &mut assistant_item, content.to_string()).await;
            }

            // Forward any reasoning/thinking deltas if present.
            // Some providers stream `reasoning` as a plain string while others
            // nest the text under an object (e.g. `{ "reasoning": { "text": "…" } }`).
            if let Some(reasoning_val) = choice.get("delta").and_then(|d| d.get("reasoning")) {
                let mut maybe_text = reasoning_val
                    .as_str()
                    .map(str::to_string)
                    .filter(|s| !s.is_empty());

                if maybe_text.is_none() && reasoning_val.is_object() {
                    if let Some(s) = reasoning_val
                        .get("text")
                        .and_then(|t| t.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        maybe_text = Some(s.to_string());
                    } else if let Some(s) = reasoning_val
                        .get("content")
                        .and_then(|t| t.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        maybe_text = Some(s.to_string());
                    }
                }

                if let Some(reasoning) = maybe_text {
                    // Accumulate so we can emit a terminal Reasoning item at the end.
                    append_reasoning_text(&tx_event, &mut reasoning_item, reasoning).await;
                }
            }

            // Some providers only include reasoning on the final message object.
            if let Some(message_reasoning) = choice.get("message").and_then(|m| m.get("reasoning"))
            {
                // Accept either a plain string or an object with { text | content }
                if let Some(s) = message_reasoning.as_str() {
                    if !s.is_empty() {
                        append_reasoning_text(&tx_event, &mut reasoning_item, s.to_string()).await;
                    }
                } else if let Some(obj) = message_reasoning.as_object()
                    && let Some(s) = obj
                        .get("text")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.get("content").and_then(|v| v.as_str()))
                    && !s.is_empty()
                {
                    append_reasoning_text(&tx_event, &mut reasoning_item, s.to_string()).await;
                }
            }

            // Handle streaming function / tool calls.
            // Each tool call chunk includes an `index` to identify which tool call
            // the chunk belongs to. We must track state per-index to handle parallel
            // tool calls correctly.
            if let Some(tool_calls) = choice
                .get("delta")
                .and_then(|d| d.get("tool_calls"))
                .and_then(|tc| tc.as_array())
            {
                for tool_call in tool_calls {
                    // === DIAGNOSTIC LOGGING: Capture raw index before resolution ===
                    let raw_index = tool_call.get("index");
                    let index = raw_index
                        .and_then(|v| v.as_i64())
                        .unwrap_or_else(|| {
                            // Missing index in parallel tool calls causes argument concatenation bugs.
                            // This should not happen with compliant providers.
                            tracing::warn!(
                                raw_index_value = ?raw_index,
                                chunk = ?tool_call,
                                existing_keys = ?fn_call_states.keys().collect::<Vec<_>>(),
                                "DIAG: Tool call chunk missing 'index' field, defaulting to 0"
                            );
                            0
                        });

                    // Detect if we're about to concatenate two complete JSON objects.
                    // This can cause bugs like {"foo":1}{"bar":2} which is invalid JSON.
                    // If detected, find a new index slot to avoid corruption.
                    let final_index = {
                        let args_fragment = tool_call
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str());

                        // Check if the existing state at this index has complete JSON
                        let existing_state = fn_call_states.get(&index);
                        let needs_new_slot = if let Some(state) = existing_state {
                            if !state.arguments.is_empty() && state.arguments.ends_with('}') {
                                // Check if new fragment starts a new JSON object
                                if let Some(frag) = args_fragment {
                                    frag.starts_with('{')
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if needs_new_slot {
                            // Find the next available index slot
                            let max_existing = fn_call_states.keys().copied().max().unwrap_or(-1);
                            let new_index = max_existing + 1;
                            tracing::warn!(
                                "Detected JSON concatenation at index={}. Provider may be reusing indices \
                                 for parallel tool calls to same function. Reassigning to index={}.",
                                index,
                                new_index
                            );
                            new_index
                        } else {
                            index
                        }
                    };

                    // Get or create state for the final index.
                    let fn_call_state = fn_call_states.entry(final_index).or_default();

                    // Extract call_id if present.
                    if let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) {
                        tracing::debug!("Streaming: extracted call_id={} for index={}", id, index);
                        fn_call_state.call_id.get_or_insert_with(|| id.to_string());
                    }

                    // Extract function details if present.
                    if let Some(function) = tool_call.get("function") {
                        if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                            fn_call_state.name.get_or_insert_with(|| name.to_string());
                        }

                        if let Some(args_fragment) =
                            function.get("arguments").and_then(|a| a.as_str())
                        {
                            // === DIAGNOSTIC LOGGING: Check for intra-chunk concatenation ===
                            // If the fragment itself contains "}{", the provider sent two JSON
                            // objects in a single chunk - our boundary detection won't catch this.
                            if args_fragment.contains("}{") {
                                tracing::error!(
                                    index = final_index,
                                    fragment = args_fragment,
                                    current_args = fn_call_state.arguments.as_str(),
                                    "DIAG CRITICAL: Intra-chunk concatenation detected! Fragment contains '}}{{'"
                                );
                            }

                            // === DIAGNOSTIC LOGGING: Log state before appending ===
                            let current_len = fn_call_state.arguments.len();
                            let current_tail: &str = if current_len > 30 {
                                &fn_call_state.arguments[current_len - 30..]
                            } else {
                                &fn_call_state.arguments
                            };

                            // Check if we're about to create a boundary issue
                            if !fn_call_state.arguments.is_empty()
                                && fn_call_state.arguments.trim_end().ends_with('}')
                            {
                                tracing::warn!(
                                    index = final_index,
                                    current_tail = current_tail,
                                    fragment_start = &args_fragment[..args_fragment.len().min(50)],
                                    fragment_len = args_fragment.len(),
                                    "DIAG: Appending to state that ends with '}}'. Potential concatenation!"
                                );
                            }

                            fn_call_state.arguments.push_str(args_fragment);
                        }
                    }

                    // Extract Gemini thought_signature from extra_content.google.thought_signature
                    // This must be echoed back for thinking models.
                    if let Some(thought_sig) = tool_call
                        .get("extra_content")
                        .and_then(|ec| ec.get("google"))
                        .and_then(|g| g.get("thought_signature"))
                        .and_then(|ts| ts.as_str())
                    {
                        tracing::debug!(
                            "Streaming: extracted thought_signature for index={}",
                            index
                        );
                        fn_call_state
                            .thought_signature
                            .get_or_insert_with(|| thought_sig.to_string());
                    }
                }
            }

            // Emit end-of-turn when finish_reason signals completion.
            if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str())
                && !finish_reason.is_empty()
            {
                match finish_reason {
                    "tool_calls" if !fn_call_states.is_empty() => {
                        // First, flush the terminal raw reasoning so UIs can finalize
                        // the reasoning stream before any exec/tool events begin.
                        if let Some(item) = reasoning_item.take() {
                            let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                        }

                        // Emit all accumulated FunctionCall response items in index order.
                        let mut indices: Vec<_> = fn_call_states.keys().copied().collect();
                        indices.sort();
                        for index in indices {
                            if let Some(fn_call_state) = fn_call_states.get(&index) {
                                let emitted_call_id = fn_call_state
                                    .call_id
                                    .clone()
                                    .unwrap_or_else(String::new);
                                let emitted_name = fn_call_state
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| "".to_string());

                                // === DIAGNOSTIC LOGGING: Check for concatenation at emission ===
                                let args = &fn_call_state.arguments;
                                if args.contains("}{") {
                                    tracing::error!(
                                        index,
                                        name = emitted_name.as_str(),
                                        call_id = emitted_call_id.as_str(),
                                        args_len = args.len(),
                                        args_preview = &args[..args.len().min(200)],
                                        "DIAG CRITICAL: About to emit FunctionCall with concatenated JSON (contains '}}{{')!"
                                    );
                                }

                                tracing::debug!(
                                    "Emitting FunctionCall: name={} call_id={} (was_some={})",
                                    emitted_name,
                                    emitted_call_id,
                                    fn_call_state.call_id.is_some()
                                );
                                let item = ResponseItem::FunctionCall {
                                    id: None,
                                    name: emitted_name,
                                    arguments: fn_call_state.arguments.clone(),
                                    call_id: emitted_call_id,
                                    thought_signature: fn_call_state.thought_signature.clone(),
                                };

                                let _ =
                                    tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                            }
                        }
                    }
                    "stop" => {
                        // Some providers (e.g., Gemini) use "stop" even when emitting tool calls.
                        // Check if we have accumulated function call state and emit those first.
                        if !fn_call_states.is_empty() {
                            tracing::debug!(
                                "finish_reason=stop but have {} accumulated tool calls, emitting them",
                                fn_call_states.len()
                            );
                            // First, flush the terminal raw reasoning so UIs can finalize
                            // the reasoning stream before any exec/tool events begin.
                            if let Some(item) = reasoning_item.take() {
                                let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                            }

                            // Emit all accumulated FunctionCall response items in index order.
                            let mut indices: Vec<_> = fn_call_states.keys().copied().collect();
                            indices.sort();
                            for index in indices {
                                if let Some(fn_call_state) = fn_call_states.get(&index) {
                                    let emitted_call_id = fn_call_state
                                        .call_id
                                        .clone()
                                        .unwrap_or_else(String::new);
                                    let emitted_name = fn_call_state
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| "".to_string());

                                    // === DIAGNOSTIC LOGGING: Check for concatenation at emission ===
                                    let args = &fn_call_state.arguments;
                                    if args.contains("}{") {
                                        tracing::error!(
                                            index,
                                            name = emitted_name.as_str(),
                                            call_id = emitted_call_id.as_str(),
                                            args_len = args.len(),
                                            args_preview = &args[..args.len().min(200)],
                                            "DIAG CRITICAL: About to emit FunctionCall (stop) with concatenated JSON (contains '}}{{')!"
                                        );
                                    }

                                    tracing::debug!(
                                        "Emitting FunctionCall (stop): name={} call_id={} (was_some={})",
                                        emitted_name,
                                        emitted_call_id,
                                        fn_call_state.call_id.is_some()
                                    );
                                    let item = ResponseItem::FunctionCall {
                                        id: None,
                                        name: emitted_name,
                                        arguments: fn_call_state.arguments.clone(),
                                        call_id: emitted_call_id,
                                        thought_signature: fn_call_state.thought_signature.clone(),
                                    };

                                    let _ =
                                        tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                                }
                            }
                        } else {
                            // Regular turn without tool-call. Emit the final assistant message
                            // as a single OutputItemDone so non-delta consumers see the result.
                            if let Some(item) = assistant_item.take() {
                                let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                            }
                            // Also emit a terminal Reasoning item so UIs can finalize raw reasoning.
                            if let Some(item) = reasoning_item.take() {
                                let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                            }
                        }
                    }
                    _ => {}
                }

                // Emit Completed regardless of reason so the agent can advance.
                let _ = tx_event
                    .send(Ok(ResponseEvent::Completed {
                        response_id: String::new(),
                        token_usage: accumulated_usage,
                    }))
                    .await;

                // Prepare for potential next turn (should not happen in same stream).
                // fn_call_state = FunctionCallState::default();

                return; // End processing for this SSE stream.
            }
        }
    }
}

/// Optional client-side aggregation helper
///
/// Stream adapter that merges the incremental `OutputItemDone` chunks coming from
/// [`process_chat_sse`] into a *running* assistant message, **suppressing the
/// per-token deltas**.  The stream stays silent while the model is thinking
/// and only emits two events per turn:
///
///   1. `ResponseEvent::OutputItemDone` with the *complete* assistant message
///      (fully concatenated).
///   2. The original `ResponseEvent::Completed` right after it.
///
/// This mirrors the behaviour the TypeScript CLI exposes to its higher layers.
///
/// The adapter is intentionally *lossless*: callers who do **not** opt in via
/// [`AggregateStreamExt::aggregate()`] keep receiving the original unmodified
/// events.
#[derive(Copy, Clone, Eq, PartialEq)]
enum AggregateMode {
    AggregatedOnly,
    Streaming,
}
pub(crate) struct AggregatedChatStream<S> {
    inner: S,
    cumulative: String,
    cumulative_reasoning: String,
    pending: std::collections::VecDeque<ResponseEvent>,
    mode: AggregateMode,
}

impl<S> Stream for AggregatedChatStream<S>
where
    S: Stream<Item = Result<ResponseEvent>> + Unpin,
{
    type Item = Result<ResponseEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // First, flush any buffered events from the previous call.
        if let Some(ev) = this.pending.pop_front() {
            return Poll::Ready(Some(Ok(ev)));
        }

        loop {
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(item)))) => {
                    // If this is an incremental assistant message chunk, accumulate but
                    // do NOT emit yet. Forward any other item (e.g. FunctionCall) right
                    // away so downstream consumers see it.

                    let is_assistant_message = matches!(
                        &item,
                        codex_protocol::models::ResponseItem::Message { role, .. } if role == "assistant"
                    );

                    if is_assistant_message {
                        match this.mode {
                            AggregateMode::AggregatedOnly => {
                                // Only use the final assistant message if we have not
                                // seen any deltas; otherwise, deltas already built the
                                // cumulative text and this would duplicate it.
                                if this.cumulative.is_empty()
                                    && let codex_protocol::models::ResponseItem::Message {
                                        content,
                                        ..
                                    } = &item
                                    && let Some(text) = content.iter().find_map(|c| match c {
                                        codex_protocol::models::ContentItem::OutputText {
                                            text,
                                        } => Some(text),
                                        _ => None,
                                    })
                                {
                                    this.cumulative.push_str(text);
                                }
                                // Swallow assistant message here; emit on Completed.
                                continue;
                            }
                            AggregateMode::Streaming => {
                                // In streaming mode, if we have not seen any deltas, forward
                                // the final assistant message directly. If deltas were seen,
                                // suppress the final message to avoid duplication.
                                if this.cumulative.is_empty() {
                                    return Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(
                                        item,
                                    ))));
                                } else {
                                    continue;
                                }
                            }
                        }
                    }

                    // Not an assistant message – forward immediately.
                    return Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone(item))));
                }
                Poll::Ready(Some(Ok(ResponseEvent::RateLimits(snapshot)))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::RateLimits(snapshot))));
                }
                Poll::Ready(Some(Ok(ResponseEvent::Completed {
                    response_id,
                    token_usage,
                }))) => {
                    // Build any aggregated items in the correct order: Reasoning first, then Message.
                    let mut emitted_any = false;

                    if !this.cumulative_reasoning.is_empty()
                        && matches!(this.mode, AggregateMode::AggregatedOnly)
                    {
                        let aggregated_reasoning =
                            codex_protocol::models::ResponseItem::Reasoning {
                                id: String::new(),
                                summary: Vec::new(),
                                content: Some(vec![
                                    codex_protocol::models::ReasoningItemContent::ReasoningText {
                                        text: std::mem::take(&mut this.cumulative_reasoning),
                                    },
                                ]),
                                encrypted_content: None,
                            };
                        this.pending
                            .push_back(ResponseEvent::OutputItemDone(aggregated_reasoning));
                        emitted_any = true;
                    }

                    // Always emit the final aggregated assistant message when any
                    // content deltas have been observed. In AggregatedOnly mode this
                    // is the sole assistant output; in Streaming mode this finalizes
                    // the streamed deltas into a terminal OutputItemDone so callers
                    // can persist/render the message once per turn.
                    if !this.cumulative.is_empty() {
                        let aggregated_message = codex_protocol::models::ResponseItem::Message {
                            id: None,
                            role: "assistant".to_string(),
                            content: vec![codex_protocol::models::ContentItem::OutputText {
                                text: std::mem::take(&mut this.cumulative),
                            }],
                        };
                        this.pending
                            .push_back(ResponseEvent::OutputItemDone(aggregated_message));
                        emitted_any = true;
                    }

                    // Always emit Completed last when anything was aggregated.
                    if emitted_any {
                        this.pending.push_back(ResponseEvent::Completed {
                            response_id: response_id.clone(),
                            token_usage: token_usage.clone(),
                        });
                        // Return the first pending event now.
                        if let Some(ev) = this.pending.pop_front() {
                            return Poll::Ready(Some(Ok(ev)));
                        }
                    }

                    // Nothing aggregated – forward Completed directly.
                    return Poll::Ready(Some(Ok(ResponseEvent::Completed {
                        response_id,
                        token_usage,
                    })));
                }
                Poll::Ready(Some(Ok(ResponseEvent::Created))) => {
                    // These events are exclusive to the Responses API and
                    // will never appear in a Chat Completions stream.
                    continue;
                }
                Poll::Ready(Some(Ok(ResponseEvent::OutputTextDelta(delta)))) => {
                    // Always accumulate deltas so we can emit a final OutputItemDone at Completed.
                    this.cumulative.push_str(&delta);
                    if matches!(this.mode, AggregateMode::Streaming) {
                        // In streaming mode, also forward the delta immediately.
                        return Poll::Ready(Some(Ok(ResponseEvent::OutputTextDelta(delta))));
                    } else {
                        continue;
                    }
                }
                Poll::Ready(Some(Ok(ResponseEvent::ReasoningContentDelta {
                    delta,
                    content_index,
                }))) => {
                    // Always accumulate reasoning deltas so we can emit a final Reasoning item at Completed.
                    this.cumulative_reasoning.push_str(&delta);
                    if matches!(this.mode, AggregateMode::Streaming) {
                        // In streaming mode, also forward the delta immediately.
                        return Poll::Ready(Some(Ok(ResponseEvent::ReasoningContentDelta {
                            delta,
                            content_index,
                        })));
                    } else {
                        continue;
                    }
                }
                Poll::Ready(Some(Ok(ResponseEvent::ReasoningSummaryDelta { .. }))) => {
                    continue;
                }
                Poll::Ready(Some(Ok(ResponseEvent::ReasoningSummaryPartAdded { .. }))) => {
                    continue;
                }
                Poll::Ready(Some(Ok(ResponseEvent::OutputItemAdded(item)))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::OutputItemAdded(item))));
                }
            }
        }
    }
}

/// Extension trait that activates aggregation on any stream of [`ResponseEvent`].
pub(crate) trait AggregateStreamExt: Stream<Item = Result<ResponseEvent>> + Sized {
    /// Returns a new stream that emits **only** the final assistant message
    /// per turn instead of every incremental delta.  The produced
    /// `ResponseEvent` sequence for a typical text turn looks like:
    ///
    /// ```ignore
    ///     OutputItemDone(<full message>)
    ///     Completed
    /// ```
    ///
    /// No other `OutputItemDone` events will be seen by the caller.
    ///
    /// Usage:
    ///
    /// ```ignore
    /// let agg_stream = client.stream(&prompt).await?.aggregate();
    /// while let Some(event) = agg_stream.next().await {
    ///     // event now contains cumulative text
    /// }
    /// ```
    fn aggregate(self) -> AggregatedChatStream<Self> {
        AggregatedChatStream::new(self, AggregateMode::AggregatedOnly)
    }
}

impl<T> AggregateStreamExt for T where T: Stream<Item = Result<ResponseEvent>> + Sized {}

impl<S> AggregatedChatStream<S> {
    fn new(inner: S, mode: AggregateMode) -> Self {
        AggregatedChatStream {
            inner,
            cumulative: String::new(),
            cumulative_reasoning: String::new(),
            pending: std::collections::VecDeque::new(),
            mode,
        }
    }

    pub(crate) fn streaming_mode(inner: S) -> Self {
        Self::new(inner, AggregateMode::Streaming)
    }
}
