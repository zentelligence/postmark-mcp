//! The MCP server: shared state, the tool catalogue, and the protocol handler.

use std::sync::Arc;

use postmark_mcp_core::{MessageStreamId, ReadsServer, SenderAddress};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool_handler};

use crate::Config;

const INSTRUCTIONS: &str =
    "Tools for the Postmark email API, scoped to the Postmark server the configured token belongs to.";

/// Values applied when a tool call does not supply its own.
#[derive(Clone, Debug)]
pub struct ServerDefaults {
    /// Sender used when a tool call does not supply one.
    pub sender: SenderAddress,
    /// Message stream used when a tool call does not supply one.
    pub message_stream: MessageStreamId,
}

impl From<&Config> for ServerDefaults {
    fn from(config: &Config) -> Self {
        Self {
            sender: config.default_sender.clone(),
            message_stream: config.default_message_stream.clone(),
        }
    }
}

/// The Postmark MCP server. Cheap to clone, and transport-independent.
#[derive(Clone)]
pub struct PostmarkMcpServer {
    pub(crate) gateway: Arc<dyn ReadsServer>,
    pub(crate) defaults: Arc<ServerDefaults>,
    tools: ToolRouter<Self>,
}

impl PostmarkMcpServer {
    /// Creates a server over the given gateway.
    #[must_use]
    pub fn new(gateway: Arc<dyn ReadsServer>, defaults: ServerDefaults) -> Self {
        Self {
            gateway,
            defaults: Arc::new(defaults),
            tools: Self::server_tool_router(),
        }
    }
}

// The macro generates `async` trait methods whose bodies do not all await, which
// this code cannot change without hand-writing the tool dispatch it provides.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler(router = self.tools)]
impl ServerHandler for PostmarkMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS.to_owned())
    }
}
