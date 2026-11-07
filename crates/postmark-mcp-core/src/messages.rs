//! Outbound messages.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::wire::{JsonObject, blank_as_none, null_as_default, object_or_none, safe_event_details, vocabulary};
use crate::{DateRange, FilterText, GatewayError, MessageStreamId, Page, ResourceId};

vocabulary! {
    /// Status of an outbound message. `sent` and `processed` are interchangeable.
    MessageStatus { Queued => "queued", Sent => "sent", Processed => "processed" }
}

/// Which outbound messages to find.
#[derive(Clone, Debug)]
pub struct MessageSearch {
    /// The page of results to return.
    pub page: Page,
    /// Stream to search.
    pub message_stream: MessageStreamId,
    /// Restricts results to one recipient.
    pub recipient: Option<FilterText>,
    /// Restricts results to one sender.
    pub from_email: Option<FilterText>,
    /// Restricts results to one tag.
    pub tag: Option<FilterText>,
    /// Restricts results by subject.
    pub subject: Option<FilterText>,
    /// Restricts results to one status.
    pub status: Option<MessageStatus>,
    /// Restricts results to a date range.
    pub dates: DateRange,
}

/// A page of outbound messages.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct MessageList {
    /// Number of matching messages, across all pages.
    pub total_count: i64,
    /// Messages on this page.
    #[serde(default, deserialize_with = "null_as_default")]
    pub messages: Vec<MessageSummary>,
}

/// An outbound message in search results.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct MessageSummary {
    /// Postmark's message identifier.
    #[serde(rename(serialize = "message_id", deserialize = "MessageID"))]
    pub message_id: String,
    /// Stream the message was sent on.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message_stream: Option<String>,
    /// Addresses the message was sent to.
    #[serde(default, deserialize_with = "null_as_default")]
    pub recipients: Vec<String>,
    /// When the message was sent.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub received_at: Option<String>,
    /// Sender of the message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub from: Option<String>,
    /// Subject line.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub subject: Option<String>,
    /// `Sent`, `Processed`, or `Queued`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub status: Option<String>,
    /// Tag on the message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub tag: Option<String>,
    /// Custom metadata on the message.
    #[serde(default, deserialize_with = "object_or_none")]
    pub metadata: Option<JsonObject>,
}

/// An outbound message and its event timeline. Message bodies are deliberately
/// not modelled, so they cannot reach a tool result.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct MessageDetails {
    /// Postmark's message identifier.
    #[serde(rename(serialize = "message_id", deserialize = "MessageID"))]
    pub message_id: String,
    /// Stream the message was sent on.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message_stream: Option<String>,
    /// Addresses the message was sent to.
    #[serde(default, deserialize_with = "null_as_default")]
    pub recipients: Vec<String>,
    /// When the message was sent.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub received_at: Option<String>,
    /// Sender of the message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub from: Option<String>,
    /// Subject line.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub subject: Option<String>,
    /// `Sent`, `Processed`, or `Queued`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub status: Option<String>,
    /// Tag on the message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub tag: Option<String>,
    /// Whether open tracking was enabled.
    pub track_opens: Option<bool>,
    /// Link tracking mode used.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub track_links: Option<String>,
    /// Whether the message was sandboxed.
    pub sandboxed: Option<bool>,
    /// Custom metadata on the message.
    #[serde(default, deserialize_with = "object_or_none")]
    pub metadata: Option<JsonObject>,
    /// What happened to the message, in the order Postmark reports it, which is
    /// not always chronological. Compare `received_at` to order events.
    #[serde(
        rename(serialize = "events", deserialize = "MessageEvents"),
        default,
        deserialize_with = "null_as_default"
    )]
    pub events: Vec<MessageEvent>,
}

/// One event in a message's timeline.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct MessageEvent {
    /// Address the event relates to.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub recipient: Option<String>,
    /// Kind of event, such as `Delivered`, `Opened`, or `Bounced`.
    pub r#type: String,
    /// When the event happened.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub received_at: Option<String>,
    /// Details of the event, which vary by kind. Links are never included.
    #[serde(default, deserialize_with = "safe_event_details")]
    pub details: Option<JsonObject>,
}

/// Reads outbound messages.
#[async_trait]
pub trait ReadsMessages: Send + Sync {
    /// Searches outbound messages.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn search_outbound_messages(&self, search: &MessageSearch) -> Result<MessageList, GatewayError>;

    /// Fetches one outbound message and its events.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn message_details(&self, message_id: &ResourceId) -> Result<MessageDetails, GatewayError>;
}
