//! Suppressed addresses.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::wire::{blank_as_none, null_as_default, vocabulary};
use crate::{DateRange, FilterText, GatewayError, MessageStreamId};

vocabulary! {
    /// Why an address is suppressed.
    SuppressionReason {
        HardBounce => "HardBounce",
        SpamComplaint => "SpamComplaint",
        ManualSuppression => "ManualSuppression",
    }
}

vocabulary! {
    /// How an address came to be suppressed.
    SuppressionOrigin { Recipient => "Recipient", Customer => "Customer", Admin => "Admin" }
}

/// Which suppressions to list.
#[derive(Clone, Debug)]
pub struct SuppressionQuery {
    /// Stream to read.
    pub message_stream: MessageStreamId,
    /// Restricts results to one reason.
    pub reason: Option<SuppressionReason>,
    /// Restricts results to one origin.
    pub origin: Option<SuppressionOrigin>,
    /// Restricts results by full or partial email address.
    pub email_address: Option<FilterText>,
    /// Restricts results to a date range.
    pub dates: DateRange,
}

/// Suppressions on a stream.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct SuppressionList {
    /// Stream these suppressions apply to.
    #[serde(skip_deserializing)]
    pub message_stream: String,
    /// Addresses that will not receive mail on this stream.
    #[serde(default, deserialize_with = "null_as_default")]
    pub suppressions: Vec<Suppression>,
}

/// A suppressed address.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct Suppression {
    /// The suppressed address.
    pub email_address: String,
    /// `HardBounce`, `SpamComplaint`, or `ManualSuppression`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub suppression_reason: Option<String>,
    /// `Recipient`, `Customer`, or `Admin`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub origin: Option<String>,
    /// When the suppression was created.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub created_at: Option<String>,
}

/// Reads suppressions.
#[async_trait]
pub trait ReadsSuppressions: Send + Sync {
    /// Lists suppressions on a stream.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn list_suppressions(&self, query: &SuppressionQuery) -> Result<SuppressionList, GatewayError>;
}
