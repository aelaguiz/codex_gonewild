use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::McpProcess;
use app_test_support::to_response;
use app_test_support::write_chatgpt_auth;
use app_test_support::write_mock_responses_config_toml;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SessionModelUpdateSource;
use codex_app_server_protocol::ThreadModelSetParams;
use codex_app_server_protocol::ThreadModelSetResponse;
use codex_app_server_protocol::ThreadModelUpdatedNotification;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput as V2UserInput;
use codex_config::types::AuthCredentialsStoreMode;
use codex_features::Feature;
use codex_protocol::ThreadId;
use codex_protocol::openai_models::ReasoningEffort;
use codex_state::StateRuntime;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const INITIAL_MODEL: &str = "gpt-5.2-codex";
const UPDATED_MODEL: &str = "gpt-5.1";
const UPDATED_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::Low;
const DEFAULT_PROVIDER: &str = "mock_provider";

#[tokio::test]
async fn thread_model_set_updates_persisted_state_emits_notification_and_affects_next_turn()
-> Result<()> {
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Done"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;

    let codex_home = TempDir::new()?;
    write_config_toml(
        codex_home.path(),
        &server.uri(),
        /*requires_openai_auth*/ None,
    )?;
    let state_db = init_state_db(codex_home.path(), DEFAULT_PROVIDER).await?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some(INITIAL_MODEL.to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse {
        thread,
        model_provider,
        ..
    } = to_response::<ThreadStartResponse>(start_resp)?;
    assert_eq!(model_provider, DEFAULT_PROVIDER);

    let thread_uuid = ThreadId::from_string(&thread.id)?;
    let set_id = mcp
        .send_thread_model_set_request(ThreadModelSetParams {
            thread_id: thread.id.clone(),
            model: Some(UPDATED_MODEL.to_string()),
            reasoning_effort: Some(Some(UPDATED_REASONING_EFFORT)),
        })
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let set_response = to_response::<ThreadModelSetResponse>(set_resp)?;
    assert_eq!(
        set_response,
        ThreadModelSetResponse {
            thread_id: thread.id.clone(),
            model: UPDATED_MODEL.to_string(),
            reasoning_effort: Some(UPDATED_REASONING_EFFORT),
            current_turn_keeps_previous_model_and_reasoning: false,
        }
    );

    let notification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/model/updated"),
    )
    .await??;
    let notification: ThreadModelUpdatedNotification =
        serde_json::from_value(notification.params.expect("notification params"))?;
    assert_eq!(
        notification,
        ThreadModelUpdatedNotification {
            thread_id: thread.id.clone(),
            previous_model: INITIAL_MODEL.to_string(),
            model: UPDATED_MODEL.to_string(),
            previous_reasoning_effort: None,
            reasoning_effort: Some(UPDATED_REASONING_EFFORT),
            source: SessionModelUpdateSource::AppServer,
            current_turn_keeps_previous_model_and_reasoning: false,
        }
    );

    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: "use the updated model".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let request = response_mock.single_request();
    let payload = request.body_json();
    assert_eq!(payload["model"].as_str(), Some(UPDATED_MODEL));
    assert_eq!(payload["reasoning"]["effort"].as_str(), Some("low"),);

    let metadata = state_db
        .get_thread(thread_uuid)
        .await?
        .expect("thread metadata should be persisted");
    assert_eq!(metadata.model, Some(UPDATED_MODEL.to_string()));
    assert_eq!(metadata.reasoning_effort, Some(UPDATED_REASONING_EFFORT));
    assert_eq!(metadata.model_provider, DEFAULT_PROVIDER);

    Ok(())
}

#[tokio::test]
async fn thread_model_set_works_with_chatgpt_authenticated_provider() -> Result<()> {
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "Done"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;

    let backend_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/backend-api/wham/config/requirements"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(json!({ "contents": "" })),
        )
        .mount(&backend_server)
        .await;

    let codex_home = TempDir::new()?;
    let chatgpt_base_url = format!("{}/backend-api", backend_server.uri());
    write_chatgpt_config_toml(codex_home.path(), &server.uri(), &chatgpt_base_url)?;
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .plan_type("business")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123")
            .account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some(INITIAL_MODEL.to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let set_id = mcp
        .send_thread_model_set_request(ThreadModelSetParams {
            thread_id: thread.id.clone(),
            model: Some(UPDATED_MODEL.to_string()),
            reasoning_effort: Some(Some(UPDATED_REASONING_EFFORT)),
        })
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let _: ThreadModelSetResponse = to_response::<ThreadModelSetResponse>(set_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/model/updated"),
    )
    .await??;

    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: "prove chatgpt auth still works".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let request = response_mock.single_request();
    assert_eq!(request.body_json()["model"].as_str(), Some(UPDATED_MODEL));
    assert_eq!(
        request.header("authorization").as_deref(),
        Some("Bearer chatgpt-token")
    );

    Ok(())
}

async fn init_state_db(codex_home: &Path, default_provider: &str) -> Result<Arc<StateRuntime>> {
    let state_db = StateRuntime::init(codex_home.to_path_buf(), default_provider.into()).await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;
    Ok(state_db)
}

fn write_config_toml(
    codex_home: &Path,
    server_uri: &str,
    requires_openai_auth: Option<bool>,
) -> std::io::Result<()> {
    write_mock_responses_config_toml(
        codex_home,
        server_uri,
        &BTreeMap::from([(Feature::Sqlite, true)]),
        /*auto_compact_limit*/ 200_000,
        requires_openai_auth,
        DEFAULT_PROVIDER,
        "compact",
    )
}

fn write_chatgpt_config_toml(
    codex_home: &Path,
    server_uri: &str,
    chatgpt_base_url: &str,
) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "{INITIAL_MODEL}"
approval_policy = "never"
sandbox_mode = "read-only"
chatgpt_base_url = "{chatgpt_base_url}"

model_provider = "{DEFAULT_PROVIDER}"

[model_providers.{DEFAULT_PROVIDER}]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}
