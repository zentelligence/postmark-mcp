//! The composite port, and the failures every port reports. Ports are small
//! capability traits, one per area of the Postmark API, each defined beside its
//! domain types.

use thiserror::Error;

use crate::{
    ReadsBounces, ReadsMessages, ReadsServer, ReadsStats, ReadsSuppressions, ReadsTemplates, ReadsWebhooks, SendsEmail,
};

/// Every capability the MCP server needs from Postmark.
pub trait PostmarkGateway:
    ReadsBounces
    + ReadsMessages
    + ReadsServer
    + ReadsStats
    + ReadsSuppressions
    + ReadsTemplates
    + ReadsWebhooks
    + SendsEmail
{
}

impl<T> PostmarkGateway for T where
    T: ReadsBounces
        + ReadsMessages
        + ReadsServer
        + ReadsStats
        + ReadsSuppressions
        + ReadsTemplates
        + ReadsWebhooks
        + SendsEmail
{
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
    /// Whether a failed send certainly did not go out. Anything else leaves the
    /// outcome unknown, such as a timeout after Postmark accepted the message, and
    /// a resend then risks sending twice.
    #[must_use]
    pub fn send_outcome_is_known(&self) -> bool {
        matches!(
            self,
            Self::Unauthorised | Self::RateLimited | Self::Rejected { .. } | Self::InvalidRequest { .. }
        )
    }

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
