//! The one place domain failures become the structured error result a tool
//! caller sees: a stable `code`, a human `message`, and a `remediation` hint.

use postmark_mcp_core::{GatewayError, MailAddress, ValueError};
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

/// A failed send. Unlike other failures, this may hide mail that went out, so a
/// caller must not be told to retry unless it certainly did not.
pub(crate) struct SendFailure(pub(crate) GatewayError);

impl From<SendFailure> for ToolFailure {
    fn from(SendFailure(error): SendFailure) -> Self {
        if error.outcome_is_known() {
            return error.into();
        }
        Self {
            code: "send/outcome-unknown".to_owned(),
            message: format!("the email may or may not have been sent: {error}"),
            remediation: "Do not send it again until search_outbound_messages shows it did not go out. Allow a few minutes first, as it may still appear.".to_owned(),
        }
    }
}

/// A failed create, which may hide a change that was made, so a caller must
/// check before trying again rather than risk a duplicate.
pub(crate) struct ChangeFailure {
    pub(crate) error: GatewayError,
    /// The tool that shows whether the change was made.
    pub(crate) check_with: &'static str,
}

impl From<ChangeFailure> for ToolFailure {
    fn from(ChangeFailure { error, check_with }: ChangeFailure) -> Self {
        if error.outcome_is_known() {
            return error.into();
        }
        Self {
            code: "change/outcome-unknown".to_owned(),
            message: format!("the change may or may not have been made: {error}"),
            remediation: format!("Check with {check_with} whether it was made before trying again."),
        }
    }
}

impl ToolFailure {
    /// A webhook URL is outside the operator's allowlist. The list itself is not
    /// revealed.
    pub(crate) fn webhook_url_not_allowed() -> Self {
        Self {
            code: "policy/webhook-url-not-allowed".to_owned(),
            message: "this URL is not an allowed webhook destination, so nothing was created".to_owned(),
            remediation: "Use an allowed URL, or ask the operator to add it to --webhook-url-allowlist.".to_owned(),
        }
    }

    /// Names the batch message a failure is about, counting from 0.
    pub(crate) fn for_message(mut self, index: usize) -> Self {
        self.message = format!("message {index}: {}", self.message);
        self
    }

    /// A recipient is outside the operator's allowlist. The list itself is not
    /// revealed, since a caller could use it to find permitted addresses.
    pub(crate) fn recipient_not_allowed(address: &MailAddress) -> Self {
        Self {
            code: "policy/recipient-not-allowed".to_owned(),
            message: format!("{} is not an allowed recipient, so nothing was sent", address.bare()),
            remediation: "Remove this recipient, or ask the operator to add it to POSTMARK_RECIPIENT_ALLOWLIST."
                .to_owned(),
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
