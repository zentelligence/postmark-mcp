//! Ports through which the server reaches Postmark, and the failures they
//! report. Ports are small capability traits, one per area of the Postmark API,
//! so a tool depends only on what it uses and tests fake only what they need.

use async_trait::async_trait;
use thiserror::Error;

/// The Postmark server a token belongs to.
///
/// Deliberately has no field for API tokens: Postmark returns them in this
/// response, and a type that cannot hold them cannot leak them to a tool result.
/// Optional fields are `None` when Postmark reports them unset or omits them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServerSummary {
    /// Postmark's numeric server identifier.
    pub id: i64,
    /// Display name of the server.
    pub name: String,
    /// Colour label of the server in Postmark's interface.
    pub color: Option<String>,
    /// Whether the SMTP API is activated for this server.
    pub smtp_api_activated: Option<bool>,
    /// Address that receives inbound mail for this server.
    pub inbound_address: Option<String>,
    /// Custom domain for inbound mail.
    pub inbound_domain: Option<String>,
    /// Whether open tracking is on by default.
    pub track_opens: Option<bool>,
    /// Default link tracking mode, such as `HtmlAndText`.
    pub track_links: Option<String>,
    /// Whether only the first open of a message triggers the open webhook.
    pub post_first_open_only: Option<bool>,
    /// Legacy server-level webhook for bounces, without any credentials.
    pub bounce_hook_url: Option<String>,
    /// Legacy server-level webhook for opens, without any credentials.
    pub open_hook_url: Option<String>,
    /// Legacy server-level webhook for deliveries, without any credentials.
    pub delivery_hook_url: Option<String>,
    /// Legacy server-level webhook for clicks, without any credentials.
    pub click_hook_url: Option<String>,
    /// Legacy server-level webhook for inbound mail, without any credentials.
    pub inbound_hook_url: Option<String>,
}

/// Reads the Postmark server the configured token belongs to.
#[async_trait]
pub trait ReadsServer: Send + Sync {
    /// Fetches the current server, which also proves the token is accepted.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request is rejected, or Postmark
    /// cannot be reached or understood.
    async fn current_server(&self) -> Result<ServerSummary, GatewayError>;
}

/// A failure reaching or being served by Postmark.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GatewayError {
    /// Postmark did not accept the server token.
    #[error("Postmark rejected the server token")]
    Unauthorised,
    /// Postmark refused the request because of its rate limit.
    #[error("Postmark's rate limit was exceeded")]
    RateLimited,
    /// Postmark refused the request as invalid.
    #[error("Postmark rejected the request (HTTP {status}, ErrorCode {error_code:?}): {message}")]
    Rejected {
        /// HTTP status of the response.
        status: u16,
        /// Postmark's numeric error code, when supplied.
        error_code: Option<i64>,
        /// Postmark's description of the problem.
        message: String,
    },
    /// Postmark reported a failure on its side.
    #[error("Postmark is unavailable (HTTP {status})")]
    Unavailable {
        /// HTTP status of the response.
        status: u16,
    },
    /// The request did not complete in time.
    #[error("the Postmark request timed out")]
    Timeout,
    /// No response was received.
    #[error("could not reach the Postmark API: {detail}")]
    Unreachable {
        /// Description of the connection failure.
        detail: String,
    },
    /// The API responded with a redirect, which is never followed.
    #[error("the Postmark API responded with a redirect (HTTP {status}), which was refused")]
    RedirectRefused {
        /// HTTP status of the response.
        status: u16,
    },
    /// The response was larger than the configured limit.
    #[error("the Postmark response exceeded the {limit_bytes} byte limit")]
    ResponseTooLarge {
        /// Configured limit in bytes.
        limit_bytes: usize,
    },
    /// A successful response could not be understood.
    #[error("the Postmark response could not be understood: {detail}")]
    InvalidResponse {
        /// Description of the decoding failure.
        detail: String,
    },
    /// The request was never sent, because of a defect in this server.
    #[error("the Postmark request could not be built: {detail}")]
    InvalidRequest {
        /// Description of the failure.
        detail: String,
    },
}

impl GatewayError {
    /// Stable, machine-readable code for this failure. Part of the public MCP
    /// contract: never change an existing value.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorised => "postmark/unauthorised",
            Self::RateLimited => "postmark/rate-limited",
            Self::Rejected { .. } => "postmark/rejected",
            Self::Unavailable { .. } => "postmark/unavailable",
            Self::Timeout => "postmark/timeout",
            Self::Unreachable { .. } => "postmark/unreachable",
            Self::RedirectRefused { .. } => "postmark/redirect-refused",
            Self::ResponseTooLarge { .. } => "postmark/response-too-large",
            Self::InvalidResponse { .. } => "postmark/invalid-response",
            Self::InvalidRequest { .. } => "server/invalid-request",
        }
    }

    /// Actionable next step for the operator or calling agent.
    #[must_use]
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::Unauthorised => {
                "Check POSTMARK_SERVER_TOKEN is a Server Token, not an Account Token. Find it at https://account.postmarkapp.com under your server's API Tokens tab."
            }
            Self::RateLimited => "Wait before retrying, and reduce the request rate.",
            Self::Rejected { .. } => "Correct the request using Postmark's message, and retry.",
            Self::Unavailable { .. } | Self::Timeout => {
                "Retry shortly, and check https://status.postmarkapp.com if the failure persists."
            }
            Self::Unreachable { .. } => "Check network access to the Postmark API, and retry.",
            Self::RedirectRefused { .. } => {
                "Check POSTMARK_API_BASE_URL. Redirects are refused so the server token cannot reach another host."
            }
            Self::ResponseTooLarge { .. } => "Narrow the request, for example with a smaller page size.",
            Self::InvalidResponse { .. } | Self::InvalidRequest { .. } => {
                "Retry the operation, and report a defect if it fails again."
            }
        }
    }
}

#[cfg(test)]
#[path = "gateway_test.rs"]
mod tests;
