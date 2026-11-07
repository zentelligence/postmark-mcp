//! The one place domain failures become the structured error result a tool
//! caller sees: a stable `code`, a human `message`, and a `remediation` hint.

use postmark_mcp_core::{GatewayError, ValueError};
use rmcp::ErrorData;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::handler::server::wrapper::Json;
use rmcp::model::CallToolResponse;
use rmcp::schemars;
use serde::Serialize;

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

impl From<ValueError> for ToolFailure {
    fn from(error: ValueError) -> Self {
        Self {
            code: "invalid/argument".to_owned(),
            message: error.to_string(),
            remediation: "Correct the argument, and retry.".to_owned(),
        }
    }
}
