//! Tool catalogue. Each module holds one area of the Postmark API as its own
//! `#[tool_router]` block; `PostmarkMcpServer::new` merges them.
//!
//! Argument types here are the raw shape a caller sends. Each converts to a
//! validated domain query with `TryFrom`, so bad input is refused with a stable
//! code before any request is made.

mod bounces;
mod diagnosis;
mod messages;
mod sending;
mod server;
mod stats;
mod suppressions;
mod templates;
mod webhooks;

use postmark_mcp_core::{FilterText, MessageStreamId, ValueError};

/// Tool arguments, with the stream to use when they name none.
struct WithDefaultStream<'a, T> {
    args: T,
    default_stream: &'a MessageStreamId,
}

/// Validates an optional free-text filter.
fn filter(field: &'static str, value: Option<String>) -> Result<Option<FilterText>, ValueError> {
    value.map(|text| FilterText::try_from((field, text))).transpose()
}

/// Validates a requested stream, falling back to the configured default.
fn stream_or_default(requested: Option<String>, default: &MessageStreamId) -> Result<MessageStreamId, ValueError> {
    requested.map_or_else(|| Ok(default.clone()), MessageStreamId::try_from)
}
