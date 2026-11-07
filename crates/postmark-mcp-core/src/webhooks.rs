//! Webhooks configured through the Webhooks API.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::skip_serializing_none;

use crate::wire::{blank_as_none, null_as_default, sanitised_url};
use crate::{GatewayError, MessageStreamId};

/// Webhooks on the server.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct WebhookList {
    /// The configured webhooks.
    #[serde(default, deserialize_with = "null_as_default")]
    pub webhooks: Vec<Webhook>,
}

/// A webhook.
///
/// `HttpAuth` and `HttpHeaders` are deliberately not modelled: Postmark returns
/// the password and header values in full, and they must not reach a tool result.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct Webhook {
    /// Numeric webhook identifier.
    #[serde(rename(serialize = "id", deserialize = "ID"))]
    pub id: i64,
    /// Destination URL, with credentials, query, and fragment removed.
    #[serde(default, deserialize_with = "sanitised_url")]
    pub url: Option<String>,
    /// Stream the webhook applies to.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message_stream: Option<String>,
    /// `verified` or `unverified`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub status: Option<String>,
    /// Events the webhook is enabled for.
    #[serde(default, deserialize_with = "enabled_triggers")]
    pub triggers: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Trigger {
    #[serde(rename = "Enabled", default)]
    enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct Triggers {
    open: Trigger,
    click: Trigger,
    delivery: Trigger,
    bounce: Trigger,
    spam_complaint: Trigger,
    subscription_change: Trigger,
}

fn enabled_triggers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let triggers = Option::<Triggers>::deserialize(deserializer)?.unwrap_or_default();
    Ok([
        ("Open", triggers.open),
        ("Click", triggers.click),
        ("Delivery", triggers.delivery),
        ("Bounce", triggers.bounce),
        ("SpamComplaint", triggers.spam_complaint),
        ("SubscriptionChange", triggers.subscription_change),
    ]
    .into_iter()
    .filter(|(_, trigger)| trigger.enabled)
    .map(|(name, _)| name.to_owned())
    .collect())
}

/// Reads webhooks.
#[async_trait]
pub trait ReadsWebhooks: Send + Sync {
    /// Lists webhooks, on one stream or across all of them.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn list_webhooks(&self, message_stream: Option<&MessageStreamId>) -> Result<WebhookList, GatewayError>;
}
