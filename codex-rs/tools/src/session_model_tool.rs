use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

pub const LIST_AVAILABLE_MODELS_TOOL_NAME: &str = "list_available_models";
pub const UPDATE_SESSION_MODEL_TOOL_NAME: &str = "update_session_model";

pub fn create_list_available_models_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "include_hidden".to_string(),
        JsonSchema::boolean(Some(
            "When true, include models hidden from the default picker.".to_string(),
        )),
    )]);

    ToolSpec::Function(ResponsesApiTool {
        name: LIST_AVAILABLE_MODELS_TOOL_NAME.to_string(),
        description: "List the current root session's valid models and supported reasoning efforts from the live runtime catalog."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, /*required*/ None, Some(false.into())),
        output_schema: None,
    })
}

pub fn create_update_session_model_tool() -> ToolSpec {
    let reasoning_effort_values = vec![
        json!("none"),
        json!("minimal"),
        json!("low"),
        json!("medium"),
        json!("high"),
        json!("xhigh"),
    ];
    let reasoning_effort = JsonSchema::any_of(
        vec![
            JsonSchema::string_enum(
                reasoning_effort_values,
                Some("Reasoning effort to apply. Use null to clear it.".to_string()),
            ),
            JsonSchema::null(/*description*/ None),
        ],
        Some(
            "Optional reasoning effort override. Omit to preserve the current effort.".to_string(),
        ),
    );
    let properties = BTreeMap::from([
        (
            "model".to_string(),
            JsonSchema::string(Some("Model slug to apply.".to_string())),
        ),
        ("reasoning_effort".to_string(), reasoning_effort),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: UPDATE_SESSION_MODEL_TOOL_NAME.to_string(),
        description: "Update the current root session's model and/or reasoning effort. The current in-flight turn keeps its old settings; the new settings apply to the next request or turn."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, /*required*/ None, Some(false.into())),
        output_schema: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResponsesApiTool;
    use pretty_assertions::assert_eq;

    #[test]
    fn list_available_models_tool_spec_is_stable() {
        assert_eq!(
            create_list_available_models_tool(),
            ToolSpec::Function(ResponsesApiTool {
                name: LIST_AVAILABLE_MODELS_TOOL_NAME.to_string(),
                description: "List the current root session's valid models and supported reasoning efforts from the live runtime catalog."
                    .to_string(),
                strict: false,
                defer_loading: None,
                parameters: JsonSchema::object(
                    BTreeMap::from([(
                        "include_hidden".to_string(),
                        JsonSchema::boolean(Some(
                            "When true, include models hidden from the default picker."
                                .to_string(),
                        )),
                    )]),
                    /*required*/ None,
                    Some(false.into()),
                ),
                output_schema: None,
            })
        );
    }

    #[test]
    fn update_session_model_tool_spec_has_nullable_reasoning_effort() {
        let ToolSpec::Function(ResponsesApiTool { parameters, .. }) =
            create_update_session_model_tool()
        else {
            panic!("expected function tool");
        };

        let properties = parameters
            .properties
            .expect("object tool schema should have properties");
        assert_eq!(
            properties.keys().collect::<Vec<_>>(),
            vec!["model", "reasoning_effort"]
        );
        assert!(
            properties
                .get("reasoning_effort")
                .and_then(|schema| schema.any_of.as_ref())
                .is_some(),
            "reasoning_effort should allow string or null"
        );
    }
}
