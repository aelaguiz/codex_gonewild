use crate::codex::SessionSettingsUpdate;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::openai_models::ModelPreset;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::protocol::SessionModelUpdateSource;
use codex_protocol::protocol::SessionSource;
use codex_tools::GET_CURRENT_SESSION_MODEL_TOOL_NAME;
use codex_tools::LIST_AVAILABLE_MODELS_TOOL_NAME;
use codex_tools::UPDATE_SESSION_MODEL_TOOL_NAME;
use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize)]
struct GetCurrentSessionModelArgs {}

#[derive(Serialize)]
struct GetCurrentSessionModelResult {
    model: String,
    reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Deserialize)]
struct ListAvailableModelsArgs {
    include_hidden: Option<bool>,
}

#[derive(Serialize)]
struct ListAvailableModelsResult {
    current_model: String,
    reasoning_effort: Option<ReasoningEffort>,
    models: Vec<AvailableModel>,
}

#[derive(Serialize)]
struct AvailableModel {
    model: String,
    display_name: String,
    description: String,
    hidden: bool,
    default_reasoning_effort: ReasoningEffort,
    supported_reasoning_efforts: Vec<AvailableReasoningEffort>,
    is_default: bool,
}

#[derive(Serialize)]
struct AvailableReasoningEffort {
    reasoning_effort: ReasoningEffort,
    description: String,
}

#[derive(Deserialize)]
struct UpdateSessionModelArgs {
    model: Option<String>,
    reasoning_effort: Option<Option<ReasoningEffort>>,
}

#[derive(Serialize)]
struct UpdateSessionModelResult {
    previous_model: String,
    model: String,
    previous_reasoning_effort: Option<ReasoningEffort>,
    reasoning_effort: Option<ReasoningEffort>,
    current_turn_keeps_previous_model_and_reasoning: bool,
}

pub struct ListAvailableModelsHandler;

pub struct GetCurrentSessionModelHandler;

impl ToolHandler for GetCurrentSessionModelHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "{GET_CURRENT_SESSION_MODEL_TOOL_NAME} handler received unsupported payload"
                )));
            }
        };

        reject_subagent_thread(&turn.session_source, GET_CURRENT_SESSION_MODEL_TOOL_NAME)?;

        let _args: GetCurrentSessionModelArgs = parse_arguments(&arguments)?;
        let collaboration_mode = session.collaboration_mode().await;
        let result = GetCurrentSessionModelResult {
            model: collaboration_mode.model().to_string(),
            reasoning_effort: collaboration_mode.reasoning_effort(),
        };
        serialize_tool_result(result, GET_CURRENT_SESSION_MODEL_TOOL_NAME)
    }
}

impl ToolHandler for ListAvailableModelsHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "{LIST_AVAILABLE_MODELS_TOOL_NAME} handler received unsupported payload"
                )));
            }
        };

        reject_subagent_thread(&turn.session_source, LIST_AVAILABLE_MODELS_TOOL_NAME)?;

        let args: ListAvailableModelsArgs = parse_arguments(&arguments)?;
        let include_hidden = args.include_hidden.unwrap_or(false);
        let models = session
            .services
            .models_manager
            .list_models(RefreshStrategy::OnlineIfUncached)
            .await
            .into_iter()
            .filter(|model| include_hidden || model.show_in_picker)
            .map(available_model_from_preset)
            .collect();
        let collaboration_mode = session.collaboration_mode().await;
        let result = ListAvailableModelsResult {
            current_model: collaboration_mode.model().to_string(),
            reasoning_effort: collaboration_mode.reasoning_effort(),
            models,
        };
        serialize_tool_result(result, LIST_AVAILABLE_MODELS_TOOL_NAME)
    }
}

pub struct UpdateSessionModelHandler;

impl ToolHandler for UpdateSessionModelHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "{UPDATE_SESSION_MODEL_TOOL_NAME} handler received unsupported payload"
                )));
            }
        };

        reject_subagent_thread(&turn.session_source, UPDATE_SESSION_MODEL_TOOL_NAME)?;

        let args: UpdateSessionModelArgs = parse_arguments(&arguments)?;
        if args.model.is_none() && args.reasoning_effort.is_none() {
            return Err(FunctionCallError::RespondToModel(
                "update_session_model requires at least one of `model` or `reasoning_effort`"
                    .to_string(),
            ));
        }

        let collaboration_mode = session.collaboration_mode().await.with_updates(
            args.model,
            args.reasoning_effort,
            /*developer_instructions*/ None,
        );
        let outcome = session
            .apply_settings_update_and_emit_session_model_event(
                turn.sub_id.clone(),
                SessionSettingsUpdate {
                    collaboration_mode: Some(collaboration_mode),
                    ..Default::default()
                },
                SessionModelUpdateSource::Tool,
            )
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
        let event = outcome.event;
        let result = UpdateSessionModelResult {
            previous_model: event.previous_model,
            model: event.model,
            previous_reasoning_effort: event.previous_reasoning_effort,
            reasoning_effort: event.reasoning_effort,
            current_turn_keeps_previous_model_and_reasoning: event
                .current_turn_keeps_previous_model_and_reasoning,
        };
        serialize_tool_result(result, UPDATE_SESSION_MODEL_TOOL_NAME)
    }
}

fn reject_subagent_thread(
    session_source: &SessionSource,
    tool_name: &str,
) -> Result<(), FunctionCallError> {
    if matches!(session_source, SessionSource::SubAgent(_)) {
        Err(FunctionCallError::RespondToModel(format!(
            "{tool_name} can only be used by the root thread"
        )))
    } else {
        Ok(())
    }
}

fn available_model_from_preset(preset: ModelPreset) -> AvailableModel {
    AvailableModel {
        model: preset.model,
        display_name: preset.display_name,
        description: preset.description,
        hidden: !preset.show_in_picker,
        default_reasoning_effort: preset.default_reasoning_effort,
        supported_reasoning_efforts: preset
            .supported_reasoning_efforts
            .into_iter()
            .map(available_reasoning_effort_from_preset)
            .collect(),
        is_default: preset.is_default,
    }
}

fn available_reasoning_effort_from_preset(
    preset: ReasoningEffortPreset,
) -> AvailableReasoningEffort {
    AvailableReasoningEffort {
        reasoning_effort: preset.effort,
        description: preset.description,
    }
}

fn serialize_tool_result<T>(
    result: T,
    tool_name: &str,
) -> Result<FunctionToolOutput, FunctionCallError>
where
    T: Serialize,
{
    let text = serde_json::to_string(&result).map_err(|err| {
        FunctionCallError::Fatal(format!("failed to serialize {tool_name} response: {err}"))
    })?;
    Ok(FunctionToolOutput::from_text(text, Some(true)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::make_session_and_context;
    use crate::tools::context::ToolInvocation;
    use crate::turn_diff_tracker::TurnDiffTracker;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::SubAgentSource;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn get_current_session_model_returns_live_settings() {
        let (session, turn) = make_session_and_context().await;
        let collaboration_mode = session.collaboration_mode().await;
        let expected_model = collaboration_mode.model().to_string();
        let expected_reasoning_effort = serde_json::to_value(collaboration_mode.reasoning_effort())
            .expect("reasoning effort should serialize");

        let result = GetCurrentSessionModelHandler
            .handle(ToolInvocation {
                session: Arc::new(session),
                turn: Arc::new(turn),
                tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
                call_id: "call-1".to_string(),
                tool_name: codex_tools::ToolName::plain(GET_CURRENT_SESSION_MODEL_TOOL_NAME),
                payload: ToolPayload::Function {
                    arguments: json!({}).to_string(),
                },
            })
            .await
            .expect("current session model read should succeed");

        let value: serde_json::Value =
            serde_json::from_str(&result.into_text()).expect("json response");
        assert_eq!(value["model"], json!(expected_model));
        assert_eq!(value["reasoning_effort"], expected_reasoning_effort);
    }

    #[tokio::test]
    async fn get_current_session_model_rejects_subagent_threads() {
        let (session, mut turn) = make_session_and_context().await;
        turn.session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ThreadId::new(),
            depth: 1,
            agent_path: None,
            agent_nickname: None,
            agent_role: None,
        });

        let result = GetCurrentSessionModelHandler
            .handle(ToolInvocation {
                session: Arc::new(session),
                turn: Arc::new(turn),
                tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
                call_id: "call-1".to_string(),
                tool_name: codex_tools::ToolName::plain(GET_CURRENT_SESSION_MODEL_TOOL_NAME),
                payload: ToolPayload::Function {
                    arguments: json!({}).to_string(),
                },
            })
            .await;

        let Err(err) = result else {
            panic!("sub-agent get_current_session_model should fail");
        };
        assert_eq!(
            err,
            FunctionCallError::RespondToModel(
                "get_current_session_model can only be used by the root thread".to_string(),
            )
        );
    }

    #[tokio::test]
    async fn list_available_models_rejects_subagent_threads() {
        let (session, mut turn) = make_session_and_context().await;
        turn.session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ThreadId::new(),
            depth: 1,
            agent_path: None,
            agent_nickname: None,
            agent_role: None,
        });

        let result = ListAvailableModelsHandler
            .handle(ToolInvocation {
                session: Arc::new(session),
                turn: Arc::new(turn),
                tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
                call_id: "call-1".to_string(),
                tool_name: codex_tools::ToolName::plain(LIST_AVAILABLE_MODELS_TOOL_NAME),
                payload: ToolPayload::Function {
                    arguments: json!({}).to_string(),
                },
            })
            .await;

        let Err(err) = result else {
            panic!("sub-agent list_available_models should fail");
        };
        assert_eq!(
            err,
            FunctionCallError::RespondToModel(
                "list_available_models can only be used by the root thread".to_string(),
            )
        );
    }

    #[tokio::test]
    async fn update_session_model_requires_an_argument() {
        let (session, turn) = make_session_and_context().await;

        let result = UpdateSessionModelHandler
            .handle(ToolInvocation {
                session: Arc::new(session),
                turn: Arc::new(turn),
                tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
                call_id: "call-1".to_string(),
                tool_name: codex_tools::ToolName::plain(UPDATE_SESSION_MODEL_TOOL_NAME),
                payload: ToolPayload::Function {
                    arguments: json!({}).to_string(),
                },
            })
            .await;

        let Err(err) = result else {
            panic!("missing arguments should fail");
        };
        assert_eq!(
            err,
            FunctionCallError::RespondToModel(
                "update_session_model requires at least one of `model` or `reasoning_effort`"
                    .to_string(),
            )
        );
    }

    #[tokio::test]
    async fn update_session_model_returns_applied_reasoning_effort() {
        let (session, turn) = make_session_and_context().await;

        let result = UpdateSessionModelHandler
            .handle(ToolInvocation {
                session: Arc::new(session),
                turn: Arc::new(turn),
                tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
                call_id: "call-1".to_string(),
                tool_name: codex_tools::ToolName::plain(UPDATE_SESSION_MODEL_TOOL_NAME),
                payload: ToolPayload::Function {
                    arguments: json!({
                        "reasoning_effort": "high"
                    })
                    .to_string(),
                },
            })
            .await
            .expect("reasoning update should succeed");

        let value: serde_json::Value =
            serde_json::from_str(&result.into_text()).expect("json response");
        assert_eq!(value["reasoning_effort"], json!("high"));
        assert_eq!(
            value["current_turn_keeps_previous_model_and_reasoning"],
            json!(false)
        );
    }
}
