//! The one place domain failures become the structured error result a tool
//! caller sees: a stable `code`, a human `message`, and a `remediation` hint.

use std::sync::Arc;

use postmark_mcp_core::GatewayError;
use rmcp::ErrorData;
use rmcp::handler::server::tool::{IntoCallToolResult, schema_for_output};
use rmcp::handler::server::wrapper::Json;
use rmcp::model::{CallToolResponse, JsonObject};
use rmcp::schemars;
use serde::Serialize;
use serde_json::{Value, json};

/// Structured error returned as a tool result with `isError` set.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ToolFailure {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) remediation: String,
}

impl IntoCallToolResult for ToolFailure {
    fn into_call_tool_result(self) -> Result<CallToolResponse, ErrorData> {
        Json(self).into_call_tool_result()
    }
}

impl From<GatewayError> for ToolFailure {
    fn from(error: GatewayError) -> Self {
        Self {
            code: error.code().to_owned(),
            message: error.to_string(),
            remediation: error.remediation().to_owned(),
        }
    }
}

/// Output schema for a tool returning `T` on success and [`ToolFailure`]
/// otherwise.
///
/// Both shapes are merged into one flat object, with `required` dropped, rather
/// than expressed as `oneOf`: a result of either shape must validate, and
/// several MCP hosts reject a tool whose output schema has no top-level
/// `properties`.
pub(crate) fn fallible_output_schema_for<T: schemars::JsonSchema + std::any::Any>() -> Arc<JsonObject> {
    let mut properties = JsonObject::new();
    let mut definitions = JsonObject::new();
    for schema in [schema_for_output::<T>(), schema_for_output::<ToolFailure>()] {
        if let Some(fields) = schema.get("properties").and_then(Value::as_object) {
            properties.extend(fields.clone());
        }
        if let Some(fields) = schema.get("$defs").and_then(Value::as_object) {
            definitions.extend(fields.clone());
        }
    }

    let mut merged = JsonObject::new();
    merged.insert(
        "$schema".to_owned(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    if !definitions.is_empty() {
        merged.insert("$defs".to_owned(), Value::Object(definitions));
    }
    merged.insert("type".to_owned(), json!("object"));
    merged.insert("properties".to_owned(), Value::Object(properties));
    Arc::new(merged)
}
