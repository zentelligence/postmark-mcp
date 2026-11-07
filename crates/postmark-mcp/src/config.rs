//! Runtime configuration, read once from the environment at startup.
//!
//! MCP hosts launch servers with an `env` block, so the environment is the only
//! configuration source, and secrets never touch a file this server owns.
//! Variable names match the official `postmark-mcp` server, so existing host
//! entries carry across unchanged.

use std::collections::BTreeMap;

use postmark_mcp_client::{ApiBaseUrl, BaseUrlError, POSTMARK_API_ORIGIN};
use postmark_mcp_core::{AgentLabel, MessageStreamId, SenderAddress, ServerToken, ValueError};
use thiserror::Error;

const SERVER_TOKEN: &str = "POSTMARK_SERVER_TOKEN";
const DEFAULT_SENDER: &str = "DEFAULT_SENDER_EMAIL";
const DEFAULT_MESSAGE_STREAM: &str = "DEFAULT_MESSAGE_STREAM";
const AGENT_LABEL: &str = "AGENT_LABEL";
const API_BASE_URL: &str = "POSTMARK_API_BASE_URL";
const SKIP_VERIFY: &str = "POSTMARK_SKIP_VERIFY";

/// A snapshot of environment variables. Passing this in, rather than reading
/// the process environment, keeps configuration deterministic under test.
#[derive(Clone, Debug, Default)]
pub struct Environment(BTreeMap<String, String>);

impl Environment {
    /// An unset variable and an empty one mean the same thing, because hosts
    /// commonly template an `env` block and leave unused entries blank.
    fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str).filter(|value| !value.is_empty())
    }

    fn require(&self, name: &'static str, remediation: &'static str) -> Result<String, ConfigError> {
        self.get(name)
            .map(str::to_owned)
            .ok_or(ConfigError::Missing { name, remediation })
    }
}

impl FromIterator<(String, String)> for Environment {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Configuration is missing or invalid.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A required variable is unset or empty.
    #[error("{name} is not set. {remediation}")]
    Missing {
        /// Name of the variable.
        name: &'static str,
        /// How to obtain a value.
        remediation: &'static str,
    },
    /// A variable failed validation.
    #[error("{name} is invalid: {source}")]
    Invalid {
        /// Name of the variable.
        name: &'static str,
        /// The validation failure.
        source: ValueError,
    },
    /// The API base URL was rejected.
    #[error("{API_BASE_URL} is invalid: {0}")]
    BaseUrl(#[from] BaseUrlError),
    /// A boolean variable has an unrecognised value.
    #[error("{name} must be `true` or `false`")]
    InvalidFlag {
        /// Name of the variable.
        name: &'static str,
    },
}

/// Validated runtime configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// Token authenticating every Postmark request.
    pub server_token: ServerToken,
    /// Sender used when a tool call does not supply one.
    pub default_sender: SenderAddress,
    /// Message stream used when a tool call does not supply one.
    pub default_message_stream: MessageStreamId,
    /// Optional label identifying this instance to Postmark.
    pub agent_label: Option<AgentLabel>,
    /// Origin the server token is sent to.
    pub api_base_url: ApiBaseUrl,
    /// Whether to skip checking the token against Postmark at startup.
    pub skip_verify: bool,
}

impl TryFrom<&Environment> for Config {
    type Error = ConfigError;

    fn try_from(environment: &Environment) -> Result<Self, Self::Error> {
        let invalid = |name| move |source| ConfigError::Invalid { name, source };

        let server_token = environment.require(
            SERVER_TOKEN,
            "Find your Server Token at https://account.postmarkapp.com under your server's API Tokens tab.",
        )?;
        let default_sender = environment.require(
            DEFAULT_SENDER,
            "Use an address with a verified Sender Signature in Postmark.",
        )?;
        let default_message_stream = environment.require(
            DEFAULT_MESSAGE_STREAM,
            "Use your outbound message stream ID, typically `outbound`.",
        )?;

        Ok(Self {
            server_token: server_token.try_into().map_err(invalid(SERVER_TOKEN))?,
            default_sender: default_sender.try_into().map_err(invalid(DEFAULT_SENDER))?,
            default_message_stream: default_message_stream
                .try_into()
                .map_err(invalid(DEFAULT_MESSAGE_STREAM))?,
            agent_label: environment
                .get(AGENT_LABEL)
                .map(|label| AgentLabel::try_from(label.to_owned()))
                .transpose()
                .map_err(invalid(AGENT_LABEL))?,
            api_base_url: environment
                .get(API_BASE_URL)
                .unwrap_or(POSTMARK_API_ORIGIN)
                .try_into()?,
            skip_verify: match environment.get(SKIP_VERIFY) {
                None | Some("false") => false,
                Some("true") => true,
                Some(_) => return Err(ConfigError::InvalidFlag { name: SKIP_VERIFY }),
            },
        })
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
