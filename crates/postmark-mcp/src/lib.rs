//! Composition root and MCP adapter for the Postmark MCP server.
//!
//! Tool handlers translate MCP requests to domain calls and domain results back
//! to MCP. No Postmark behaviour lives here, and no MCP type crosses into the
//! lower crates.

mod config;
mod server;
mod tool_error;
mod tools;

pub use config::{Config, ConfigError, Environment};
pub use server::{PostmarkMcpServer, ServerDefaults};
