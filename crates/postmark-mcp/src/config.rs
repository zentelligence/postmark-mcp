//! Runtime configuration, read once at startup.
//!
//! Every setting has a command-line flag, because every MCP host can pass
//! arguments to the server it launches, while not every host can set environment
//! variables. Each setting also falls back to an environment variable, named as
//! in the official `postmark-mcp` server where one exists there, so existing
//! host entries carry across. A flag always wins over its variable.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use clap::Args;
use postmark_mcp_client::{ApiBaseUrl, BaseUrlError, POSTMARK_API_ORIGIN};
use postmark_mcp_core::{
    AgentLabel, MessageStreamId, RecipientAllowlist, SenderAddress, ServerToken, ValueError, WebhookUrlAllowlist,
};
use thiserror::Error;

/// One configurable setting: its flag, and the variable it falls back to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Setting {
    /// The command-line flag.
    pub flag: &'static str,
    /// The environment variable.
    pub variable: &'static str,
}

impl fmt::Display for Setting {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} (or {})", self.flag, self.variable)
    }
}

const fn setting(flag: &'static str, variable: &'static str) -> Setting {
    Setting { flag, variable }
}

const SERVER_TOKEN: Setting = setting("--server-token", "POSTMARK_SERVER_TOKEN");
const DEFAULT_SENDER: Setting = setting("--default-sender", "DEFAULT_SENDER_EMAIL");
const DEFAULT_MESSAGE_STREAM: Setting = setting("--default-message-stream", "DEFAULT_MESSAGE_STREAM");
const AGENT_LABEL: Setting = setting("--agent-label", "AGENT_LABEL");
const API_BASE_URL: Setting = setting("--api-base-url", "POSTMARK_API_BASE_URL");
const SKIP_VERIFY: Setting = setting("--skip-verify", "POSTMARK_SKIP_VERIFY");
const TOOLSETS: Setting = setting("--toolsets", "POSTMARK_TOOLSETS");
const RECIPIENT_ALLOWLIST: Setting = setting("--recipient-allowlist", "POSTMARK_RECIPIENT_ALLOWLIST");
const WEBHOOK_URL_ALLOWLIST: Setting = setting("--webhook-url-allowlist", "WEBHOOK_URL_ALLOWLIST");
const LOG_FILE: Setting = setting("--log-file", "LOG_FILE");
const LOG_EMAIL_FULL: Setting = setting("--log-email-full", "LOG_EMAIL_FULL");

/// Settings given as command-line flags. Any left unset fall back to the
/// environment.
#[derive(Args, Clone, Default)]
pub struct Settings {
    /// Postmark server token. Visible to other users of this machine through the
    /// process list, so prefer --server-token-file where that matters.
    #[arg(long, global = true, conflicts_with = "server_token_file")]
    pub server_token: Option<String>,
    /// File holding the Postmark server token, which keeps it out of the process
    /// list and the host's configuration.
    #[arg(long, global = true, value_name = "PATH")]
    pub server_token_file: Option<PathBuf>,
    /// Sender used when a tool call does not supply one. Must be verified in Postmark.
    #[arg(long, global = true, value_name = "ADDRESS")]
    pub default_sender: Option<String>,
    /// Message stream used when a tool call does not supply one, typically `outbound`.
    #[arg(long, global = true, value_name = "STREAM")]
    pub default_message_stream: Option<String>,
    /// Label sent to Postmark as `X-Agent-Label` on every request.
    #[arg(long, global = true, value_name = "LABEL")]
    pub agent_label: Option<String>,
    /// Postmark API origin. Only for testing against a local stand-in.
    #[arg(long, global = true, value_name = "URL")]
    pub api_base_url: Option<String>,
    /// Skip checking the token against Postmark at startup.
    #[arg(long, global = true)]
    pub skip_verify: bool,
    /// Tool groups to offer, from `read`, `send`, and `manage`, comma-separated.
    /// Defaults to all three.
    #[arg(long, global = true, value_name = "GROUPS")]
    pub toolsets: Option<String>,
    /// Addresses (`name@example.com`) and domains (`@example.com`) that mail may
    /// be sent to, comma-separated. For staging, internal-only, and demonstration
    /// deployments.
    #[arg(long, global = true, value_name = "ENTRIES")]
    pub recipient_allowlist: Option<String>,
    /// HTTPS URL prefixes that `create_webhook` may register, comma-separated.
    #[arg(long, global = true, value_name = "URLS")]
    pub webhook_url_allowlist: Option<String>,
    /// File each tool call is appended to, as well as stderr.
    #[arg(long, global = true, value_name = "PATH")]
    pub log_file: Option<PathBuf>,
    /// Log addresses in full rather than masked.
    #[arg(long, global = true)]
    pub log_email_full: bool,
}

/// Written by hand so the token never appears in debug output.
impl fmt::Debug for Settings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Settings")
            .field("server_token", &self.server_token.as_ref().map(|_| "[redacted]"))
            .field("server_token_file", &self.server_token_file)
            .field("default_sender", &self.default_sender)
            .field("default_message_stream", &self.default_message_stream)
            .field("toolsets", &self.toolsets)
            .finish_non_exhaustive()
    }
}

/// A snapshot of environment variables. Passing this in, rather than reading
/// the process environment, keeps configuration deterministic under test.
#[derive(Clone, Debug, Default)]
pub struct Environment(BTreeMap<String, String>);

impl FromIterator<(String, String)> for Environment {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Which groups of tools the server offers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Toolsets {
    /// Tools that only read.
    pub read: bool,
    /// Tools that send email.
    pub send: bool,
    /// Tools that change templates, suppressions, bounces, or webhooks.
    pub manage: bool,
}

impl Default for Toolsets {
    fn default() -> Self {
        Self {
            read: true,
            send: true,
            manage: true,
        }
    }
}

impl TryFrom<&str> for Toolsets {
    type Error = ConfigError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut chosen = Self {
            read: false,
            send: false,
            manage: false,
        };
        for group in value.split(',').map(str::trim) {
            match group {
                "read" => chosen.read = true,
                "send" => chosen.send = true,
                "manage" => chosen.manage = true,
                _ => return Err(ConfigError::UnknownToolset { setting: TOOLSETS }),
            }
        }
        Ok(chosen)
    }
}

/// Configuration is missing or invalid.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A required setting is unset or empty.
    #[error("{setting} is not set. {remediation}")]
    Missing {
        /// The setting.
        setting: Setting,
        /// How to obtain a value.
        remediation: &'static str,
    },
    /// A setting failed validation.
    #[error("{setting} is invalid: {source}")]
    Invalid {
        /// The setting.
        setting: Setting,
        /// The validation failure.
        source: ValueError,
    },
    /// The API base URL was rejected.
    #[error("{API_BASE_URL} is invalid: {0}")]
    BaseUrl(#[from] BaseUrlError),
    /// A boolean variable has an unrecognised value.
    #[error("{setting} must be `true` or `false`")]
    InvalidFlag {
        /// The setting.
        setting: Setting,
    },
    /// A flag was given an empty value, which would otherwise hide its variable.
    #[error("{setting} was given an empty value. Give a value, or leave the flag out")]
    EmptyFlag {
        /// The setting.
        setting: Setting,
    },
    /// A setting from an earlier version is still set.
    #[error("POSTMARK_READ_ONLY is no longer supported. Use --toolsets read (or POSTMARK_TOOLSETS=read) instead")]
    RetiredReadOnly,
    /// A tool group is not recognised.
    #[error("{setting} must list one or more of read, send, and manage, comma-separated")]
    UnknownToolset {
        /// The setting.
        setting: Setting,
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
    /// Which groups of tools to offer.
    pub toolsets: Toolsets,
    /// Restricts where mail may be sent.
    pub recipient_allowlist: Option<RecipientAllowlist>,
    /// Restricts where webhooks may be registered.
    pub webhook_url_allowlist: Option<WebhookUrlAllowlist>,
    /// File tool calls are appended to, as well as stderr.
    pub log_file: Option<PathBuf>,
    /// Whether to log addresses in full rather than masked.
    pub log_email_full: bool,
}

/// Where configuration comes from: flags first, then the environment.
#[derive(Clone, Copy, Debug)]
pub struct Sources<'a> {
    /// Settings given as flags. A token file must already have been read into
    /// `server_token`, so this stays free of I/O.
    pub settings: &'a Settings,
    /// Environment variables.
    pub environment: &'a Environment,
}

impl Sources<'_> {
    /// A flag's value, or else its variable's. An empty value counts as unset,
    /// because hosts commonly template an `env` block and leave entries blank.
    ///
    /// An empty flag is refused: it would otherwise hide its variable, which for
    /// an allowlist means silently lifting the restriction.
    fn value<'v>(&'v self, setting: Setting, flag: Option<&'v str>) -> Result<Option<&'v str>, ConfigError> {
        if flag == Some("") {
            return Err(ConfigError::EmptyFlag { setting });
        }
        Ok(flag
            .or_else(|| self.environment.0.get(setting.variable).map(String::as_str))
            .filter(|value| !value.is_empty()))
    }

    fn require<'v>(
        &'v self,
        setting: Setting,
        flag: Option<&'v str>,
        remediation: &'static str,
    ) -> Result<&'v str, ConfigError> {
        self.value(setting, flag)?
            .ok_or(ConfigError::Missing { setting, remediation })
    }

    /// A switch is on if its flag is present, or else its variable is `true`.
    fn switch(&self, setting: Setting, flag: bool) -> Result<bool, ConfigError> {
        if flag {
            return Ok(true);
        }
        match self.value(setting, None)? {
            None | Some("false") => Ok(false),
            Some("true") => Ok(true),
            Some(_) => Err(ConfigError::InvalidFlag { setting }),
        }
    }
}

fn invalid(setting: Setting) -> impl Fn(ValueError) -> ConfigError {
    move |source| ConfigError::Invalid { setting, source }
}

impl TryFrom<Sources<'_>> for Config {
    type Error = ConfigError;

    fn try_from(sources: Sources<'_>) -> Result<Self, Self::Error> {
        let settings = sources.settings;
        // Ignoring it would silently grant every tool to a deployment that asked
        // for read-only access.
        if sources.environment.0.contains_key("POSTMARK_READ_ONLY") {
            return Err(ConfigError::RetiredReadOnly);
        }
        let server_token = sources.require(
            SERVER_TOKEN,
            settings.server_token.as_deref(),
            "Give it with --server-token or --server-token-file. Find it at https://account.postmarkapp.com under your server's API Tokens tab.",
        )?;
        let default_sender = sources.require(
            DEFAULT_SENDER,
            settings.default_sender.as_deref(),
            "Use an address with a verified Sender Signature in Postmark.",
        )?;
        let default_message_stream = sources.require(
            DEFAULT_MESSAGE_STREAM,
            settings.default_message_stream.as_deref(),
            "Use your outbound message stream ID, typically `outbound`.",
        )?;
        let toolsets = match sources
            .environment
            .0
            .get(TOOLSETS.variable)
            .filter(|_| settings.toolsets.is_none())
        {
            // An empty flag is a mistake to report, not a request for the default.
            None => settings
                .toolsets
                .as_deref()
                .map_or(Ok(Toolsets::default()), Toolsets::try_from)?,
            Some(variable) if variable.is_empty() => Toolsets::default(),
            Some(variable) => Toolsets::try_from(variable.as_str())?,
        };

        Ok(Self {
            server_token: server_token.to_owned().try_into().map_err(invalid(SERVER_TOKEN))?,
            default_sender: default_sender.to_owned().try_into().map_err(invalid(DEFAULT_SENDER))?,
            default_message_stream: default_message_stream
                .to_owned()
                .try_into()
                .map_err(invalid(DEFAULT_MESSAGE_STREAM))?,
            agent_label: sources
                .value(AGENT_LABEL, settings.agent_label.as_deref())?
                .map(|label| AgentLabel::try_from(label.to_owned()))
                .transpose()
                .map_err(invalid(AGENT_LABEL))?,
            api_base_url: sources
                .value(API_BASE_URL, settings.api_base_url.as_deref())?
                .unwrap_or(POSTMARK_API_ORIGIN)
                .try_into()?,
            skip_verify: sources.switch(SKIP_VERIFY, settings.skip_verify)?,
            toolsets,
            recipient_allowlist: sources
                .value(RECIPIENT_ALLOWLIST, settings.recipient_allowlist.as_deref())?
                .map(RecipientAllowlist::try_from)
                .transpose()
                .map_err(invalid(RECIPIENT_ALLOWLIST))?,
            webhook_url_allowlist: sources
                .value(WEBHOOK_URL_ALLOWLIST, settings.webhook_url_allowlist.as_deref())?
                .map(WebhookUrlAllowlist::try_from)
                .transpose()
                .map_err(invalid(WEBHOOK_URL_ALLOWLIST))?,
            log_file: match settings.log_file.as_deref() {
                Some(path) if path.as_os_str().is_empty() => return Err(ConfigError::EmptyFlag { setting: LOG_FILE }),
                Some(path) => Some(path.to_path_buf()),
                None => sources.value(LOG_FILE, None)?.map(PathBuf::from),
            },
            log_email_full: sources.switch(LOG_EMAIL_FULL, settings.log_email_full)?,
        })
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
