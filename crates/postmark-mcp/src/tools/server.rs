//! Tools about the Postmark server the configured token belongs to.

use postmark_mcp_core::ServerSummary;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::schema::fallible_output_schema_for;
use crate::server::{PostmarkMcpServer, ServerDefaults};
use crate::tool_error::ToolFailure;

/// Input for a tool that takes no arguments. Rejects unknown fields, so a
/// caller's mistaken argument fails loudly rather than being ignored.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoArguments {}

/// Result of `get_server_info`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ServerInfoResult {
    #[serde(flatten)]
    server: ServerSummary,
    /// Sender used when a tool call does not supply one.
    default_sender: String,
    /// Message stream used when a tool call does not supply one.
    default_message_stream: String,
}

impl From<(ServerSummary, &ServerDefaults)> for ServerInfoResult {
    fn from((server, defaults): (ServerSummary, &ServerDefaults)) -> Self {
        Self {
            server,
            default_sender: defaults.sender.to_string(),
            default_message_stream: defaults.message_stream.to_string(),
        }
    }
}

#[tool_router(router = server_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "get_server_info",
        description = "Report the configuration of the Postmark server this token belongs to: name, ID, colour, SMTP activation, inbound address, tracking settings, and legacy server-level webhook URLs, plus the default sender and message stream applied when a tool call does not supply its own.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = fallible_output_schema_for::<ServerInfoResult>()
    )]
    async fn get_server_info(
        &self,
        Parameters(NoArguments {}): Parameters<NoArguments>,
    ) -> Result<Json<ServerInfoResult>, ToolFailure> {
        let server = self.gateway.current_server().await?;
        Ok(Json((server, self.defaults.as_ref()).into()))
    }
}
