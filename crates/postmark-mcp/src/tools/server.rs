//! Tools about the Postmark server the configured token belongs to.

use postmark_mcp_core::ServerSummary;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::server::{PostmarkMcpServer, ServerDefaults};
use crate::tool_error::{ToolFailure, fallible_output_schema_for};

/// Input for a tool that takes no arguments. Rejects unknown fields, so a
/// caller's mistaken argument fails loudly rather than being ignored.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoArguments {}

/// Advertises an optional result field as its plain type, not schemars' default
/// `["string", "null"]`. Several MCP hosts mishandle array-form `type`, and an
/// unset field here is omitted rather than null.
fn plain<T: schemars::JsonSchema>(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    generator.subschema_for::<T>()
}

/// Result of `get_server_info`. API tokens are deliberately absent, and unset
/// fields are omitted.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ServerInfoResult {
    /// Postmark's numeric server identifier.
    server_id: i64,
    /// Display name of the server.
    name: String,
    /// Colour label of the server in Postmark's interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    color: Option<String>,
    /// Whether the SMTP API is activated for this server.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<bool>")]
    smtp_api_activated: Option<bool>,
    /// Address that receives inbound mail for this server.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    inbound_address: Option<String>,
    /// Custom domain for inbound mail.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    inbound_domain: Option<String>,
    /// Whether open tracking is on by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<bool>")]
    track_opens: Option<bool>,
    /// Default link tracking mode, such as `HtmlAndText`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    track_links: Option<String>,
    /// Whether only the first open of a message triggers the open webhook.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<bool>")]
    post_first_open_only: Option<bool>,
    /// Legacy server-level webhook for bounces, with any credentials removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    bounce_hook_url: Option<String>,
    /// Legacy server-level webhook for opens, with any credentials removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    open_hook_url: Option<String>,
    /// Legacy server-level webhook for deliveries, with any credentials removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    delivery_hook_url: Option<String>,
    /// Legacy server-level webhook for clicks, with any credentials removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    click_hook_url: Option<String>,
    /// Legacy server-level webhook for inbound mail, with any credentials removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "plain::<String>")]
    inbound_hook_url: Option<String>,
    /// Sender used when a tool call does not supply one.
    default_sender: String,
    /// Message stream used when a tool call does not supply one.
    default_message_stream: String,
}

impl From<(ServerSummary, &ServerDefaults)> for ServerInfoResult {
    fn from((server, defaults): (ServerSummary, &ServerDefaults)) -> Self {
        Self {
            server_id: server.id,
            name: server.name,
            color: server.color,
            smtp_api_activated: server.smtp_api_activated,
            inbound_address: server.inbound_address,
            inbound_domain: server.inbound_domain,
            track_opens: server.track_opens,
            track_links: server.track_links,
            post_first_open_only: server.post_first_open_only,
            bounce_hook_url: server.bounce_hook_url,
            open_hook_url: server.open_hook_url,
            delivery_hook_url: server.delivery_hook_url,
            click_hook_url: server.click_hook_url,
            inbound_hook_url: server.inbound_hook_url,
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
