//! Validated value types for security-significant or protocol-significant
//! strings. Each is only constructible through `TryFrom`, so holding one proves
//! the value was checked at the boundary it entered through.

use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

/// Upper bound on [`AgentLabel`], which is sent on every API request.
const MAX_AGENT_LABEL_LEN: usize = 128;

/// A value failed validation.
///
/// No variant carries the rejected input: a malformed token is still likely to
/// be mostly secret, and these messages reach logs and operators.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ValueError {
    /// The value was empty or only whitespace.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The value cannot be carried in an HTTP header.
    #[error("{field} must contain only visible ASCII characters")]
    NotHeaderSafe {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The sender is not an address, or carries control characters that could
    /// inject additional mail headers.
    #[error("sender must be an email address, optionally as `Name <address>`, with no control characters")]
    MalformedSender,
    /// The message stream identifier has characters Postmark does not use.
    #[error("message stream ID must contain only ASCII letters, digits, hyphens, and underscores")]
    MalformedMessageStream,
    /// The value exceeds its permitted length.
    #[error("{field} must not exceed {limit} characters")]
    TooLong {
        /// Name of the value that failed validation.
        field: &'static str,
        /// Maximum permitted length.
        limit: usize,
    },
}

fn require_header_safe(field: &'static str, value: &str) -> Result<(), ValueError> {
    if value.trim().is_empty() {
        return Err(ValueError::Empty { field });
    }
    if value.bytes().all(|byte| byte.is_ascii_graphic() || byte == b' ') {
        Ok(())
    } else {
        Err(ValueError::NotHeaderSafe { field })
    }
}

/// A Postmark server API token.
///
/// The wrapped secret is redacted from `Debug` output and zeroed on drop. Read
/// it with [`ServerToken::expose`] only at the point it is placed in a request.
#[derive(Clone, Debug)]
pub struct ServerToken(SecretString);

impl ServerToken {
    /// Returns the secret token for placing in an HTTP header.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl TryFrom<String> for ServerToken {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const FIELD: &str = "server token";
        require_header_safe(FIELD, &value)?;
        if value.contains(' ') {
            return Err(ValueError::NotHeaderSafe { field: FIELD });
        }
        Ok(Self(SecretString::from(value)))
    }
}

/// A sender address, as `address@example.com` or `Name <address@example.com>`.
///
/// This proves the value is shaped like a sender and is safe to place in a
/// message. Postmark remains the authority on whether the sender is verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SenderAddress(String);

impl TryFrom<String> for SenderAddress {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(ValueError::Empty { field: "sender" });
        }
        let has_mailbox_and_domain = value
            .rsplit_once('@')
            .is_some_and(|(mailbox, domain)| !mailbox.trim().is_empty() && !domain.trim().is_empty());
        if has_mailbox_and_domain && !value.chars().any(char::is_control) {
            Ok(Self(value))
        } else {
            Err(ValueError::MalformedSender)
        }
    }
}

impl AsRef<str> for SenderAddress {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SenderAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A Postmark message stream identifier, such as `outbound`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageStreamId(String);

impl TryFrom<String> for MessageStreamId {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(ValueError::Empty {
                field: "message stream ID",
            });
        }
        if value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            Ok(Self(value))
        } else {
            Err(ValueError::MalformedMessageStream)
        }
    }
}

impl AsRef<str> for MessageStreamId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MessageStreamId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// An operator-chosen label identifying this instance in Postmark request logs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLabel(String);

impl TryFrom<String> for AgentLabel {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const FIELD: &str = "agent label";
        require_header_safe(FIELD, &value)?;
        if value.len() > MAX_AGENT_LABEL_LEN {
            return Err(ValueError::TooLong {
                field: FIELD,
                limit: MAX_AGENT_LABEL_LEN,
            });
        }
        Ok(Self(value))
    }
}

impl AsRef<str> for AgentLabel {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
#[path = "values_test.rs"]
mod tests;
