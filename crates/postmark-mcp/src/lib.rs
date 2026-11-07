//! Composition root and MCP adapter for the Postmark MCP server.
//!
//! Tool handlers translate MCP requests to domain calls and domain results back
//! to MCP. No Postmark behaviour lives here, and no MCP type crosses into the
//! lower crates.

mod config;
mod schema;
mod server;
mod tool_error;
mod tool_log;
mod tools;

pub use config::{Config, ConfigError, Environment, Setting, Settings, Sources, Toolsets};
pub use server::{PostmarkMcpServer, ServerDefaults, ServerPolicy};
pub use tool_log::ToolLog;
