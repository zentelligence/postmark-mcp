//! The Postmark server a token belongs to.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::GatewayError;
use crate::wire::{blank_as_none, sanitised_url};

/// The Postmark server a token belongs to.
///
/// Deliberately has no field for API tokens: Postmark returns them in this
/// response, and a type that cannot hold them cannot leak them to a tool result.
/// Optional fields are `None` when Postmark reports them unset or omits them.
#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct ServerSummary {
    /// Postmark's numeric server identifier.
    #[serde(rename(serialize = "server_id", deserialize = "ID"))]
    pub id: i64,
    /// Display name of the server.
    pub name: String,
    /// Colour label of the server in Postmark's interface.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub color: Option<String>,
    /// Whether the SMTP API is activated for this server.
    pub smtp_api_activated: Option<bool>,
    /// Address that receives inbound mail for this server.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub inbound_address: Option<String>,
    /// Custom domain for inbound mail.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub inbound_domain: Option<String>,
    /// Whether open tracking is on by default.
    pub track_opens: Option<bool>,
    /// Default link tracking mode, such as `HtmlAndText`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub track_links: Option<String>,
    /// Whether only the first open of a message triggers the open webhook.
    pub post_first_open_only: Option<bool>,
    /// Legacy server-level webhook for bounces, with credentials, query, and fragment removed.
    #[serde(default, deserialize_with = "sanitised_url")]
    pub bounce_hook_url: Option<String>,
    /// Legacy server-level webhook for opens, with credentials, query, and fragment removed.
    #[serde(default, deserialize_with = "sanitised_url")]
    pub open_hook_url: Option<String>,
    /// Legacy server-level webhook for deliveries, with credentials, query, and fragment removed.
    #[serde(default, deserialize_with = "sanitised_url")]
    pub delivery_hook_url: Option<String>,
    /// Legacy server-level webhook for clicks, with credentials, query, and fragment removed.
    #[serde(default, deserialize_with = "sanitised_url")]
    pub click_hook_url: Option<String>,
    /// Legacy server-level webhook for inbound mail, with credentials, query, and fragment removed.
    #[serde(default, deserialize_with = "sanitised_url")]
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
