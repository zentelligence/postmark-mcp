//! Bounces.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::wire::{blank_as_none, null_as_default, vocabulary};
use crate::{BounceId, DateRange, FilterText, GatewayError, MessageStreamId, Page};

vocabulary! {
    /// Kind of bounce.
    BounceType {
        AddressChange => "AddressChange",
        AutoResponder => "AutoResponder",
        BadEmailAddress => "BadEmailAddress",
        Blocked => "Blocked",
        ChallengeVerification => "ChallengeVerification",
        DmarcPolicy => "DMARCPolicy",
        DnsError => "DnsError",
        HardBounce => "HardBounce",
        InboundError => "InboundError",
        ManuallyDeactivated => "ManuallyDeactivated",
        OpenRelayTest => "OpenRelayTest",
        SmtpApiError => "SMTPApiError",
        SoftBounce => "SoftBounce",
        SpamComplaint => "SpamComplaint",
        SpamNotification => "SpamNotification",
        Subscribe => "Subscribe",
        TemplateRenderingFailed => "TemplateRenderingFailed",
        Transient => "Transient",
        Unconfirmed => "Unconfirmed",
        Unknown => "Unknown",
        Unsubscribe => "Unsubscribe",
        VirusNotification => "VirusNotification",
    }
}

/// What a bounce record says about delivery. Postmark's bounce log holds more
/// than failed deliveries, including out of office replies and notices about mail
/// that did arrive, so a record alone does not mean a message failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BounceOutcome {
    /// The message was not delivered.
    Failed,
    /// Delivery hit a temporary problem.
    Delayed,
    /// The record is not a delivery failure.
    NotAFailure,
    /// The record does not say either way.
    Unclassified,
}

/// Which bounces to find.
#[derive(Clone, Debug)]
pub struct BounceSearch {
    /// The page of results to return.
    pub page: Page,
    /// Stream to search.
    pub message_stream: MessageStreamId,
    /// Restricts results to one kind of bounce.
    pub bounce_type: Option<BounceType>,
    /// Restricts results to deactivated, or active, addresses.
    pub inactive: Option<bool>,
    /// Restricts results by full or partial email address.
    pub email_filter: Option<FilterText>,
    /// Restricts results to one tag.
    pub tag: Option<FilterText>,
    /// Restricts results to one original message.
    pub message_id: Option<FilterText>,
    /// Restricts results to a date range.
    pub dates: DateRange,
}

/// A page of bounces.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct BounceList {
    /// Number of matching bounces, across all pages.
    pub total_count: i64,
    /// Bounces on this page.
    #[serde(default, deserialize_with = "null_as_default")]
    pub bounces: Vec<Bounce>,
}

/// A bounce record. The original message content is deliberately not modelled.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct Bounce {
    /// Identifier for `get_bounce_dump`.
    #[serde(rename(serialize = "bounce_id", deserialize = "ID"))]
    pub id: i64,
    /// Kind of bounce.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub r#type: Option<String>,
    /// Numeric code for the kind of bounce.
    pub type_code: Option<i64>,
    /// Name of the kind of bounce.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub name: Option<String>,
    /// Tag on the original message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub tag: Option<String>,
    /// Identifier of the original message.
    #[serde(rename(serialize = "message_id", deserialize = "MessageID"))]
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message_id: Option<String>,
    /// Stream of the original message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message_stream: Option<String>,
    /// Description of the bounce.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub description: Option<String>,
    /// Details from the receiving server.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub details: Option<String>,
    /// Address that bounced.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub email: Option<String>,
    /// Original sender, if available.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub from: Option<String>,
    /// When the bounce happened.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub bounced_at: Option<String>,
    /// Whether `get_bounce_dump` can retrieve the raw source.
    pub dump_available: Option<bool>,
    /// Whether Postmark deactivated the address because of this bounce.
    pub inactive: Option<bool>,
    /// Whether the address can be reactivated.
    pub can_activate: Option<bool>,
    /// Subject of the original message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub subject: Option<String>,
}

impl Bounce {
    /// Classifies this record from its type, following the descriptions in
    /// Postmark's bounce type table. `SoftBounce` is a failure, not a delay:
    /// Postmark's guidance says it needs action, and nothing says it is retried.
    #[must_use]
    pub fn outcome(&self) -> BounceOutcome {
        match self.r#type.as_deref() {
            Some(
                "HardBounce"
                | "SoftBounce"
                | "BadEmailAddress"
                | "Blocked"
                | "SMTPApiError"
                | "DMARCPolicy"
                | "TemplateRenderingFailed",
            ) => BounceOutcome::Failed,
            Some("Transient" | "DnsError") => BounceOutcome::Delayed,
            Some(
                "AutoResponder"
                | "SpamNotification"
                | "SpamComplaint"
                | "Subscribe"
                | "Unsubscribe"
                | "AddressChange"
                | "OpenRelayTest"
                | "VirusNotification"
                | "ChallengeVerification",
            ) => BounceOutcome::NotAFailure,
            _ => BounceOutcome::Unclassified,
        }
    }
}

/// The raw source of a bounce. This is what the remote server sent back, which
/// can quote the original message, so it is returned only on explicit request.
#[skip_serializing_none]
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct BounceDump {
    /// Identifier of the bounce.
    pub bounce_id: u64,
    /// Whether the source is available. Postmark keeps it for 30 days.
    pub available: bool,
    /// The raw source.
    pub body: Option<String>,
}

/// Reads bounces.
#[async_trait]
pub trait ReadsBounces: Send + Sync {
    /// Searches bounces.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn search_bounces(&self, search: &BounceSearch) -> Result<BounceList, GatewayError>;

    /// Fetches the raw source of a bounce.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn bounce_dump(&self, bounce_id: BounceId) -> Result<BounceDump, GatewayError>;
}
