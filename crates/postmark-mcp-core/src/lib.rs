//! Domain types and ports for the Postmark MCP server.
//!
//! This crate is the centre of the hexagon: every other workspace crate depends
//! on it, and it depends on no transport, HTTP, or MCP implementation.

mod gateway;
mod values;

pub use gateway::{GatewayError, ReadsServer, ServerSummary};
pub use values::{AgentLabel, MessageStreamId, SenderAddress, ServerToken, ValueError};
