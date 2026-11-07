//! Tools for webhooks.

use postmark_mcp_core::{MessageStreamId, WebhookList};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::ToolFailure;

/// Arguments of `list_webhooks`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListWebhooksArgs {
    /// Restricts results to this stream. All streams are covered otherwise.
    message_stream: Option<String>,
}

#[tool_router(router = webhook_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "list_webhooks",
        description = "List webhooks configured through Postmark's Webhooks API, with each one's ID, URL, stream, status, and enabled triggers. Credentials, custom headers, and URL query strings are never included, though a URL path can itself be a secret. See get_server_info for legacy server-level webhooks.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<ListWebhooksArgs>(),
        output_schema = fallible_output_schema_for::<WebhookList>()
    )]
    async fn list_webhooks(
        &self,
        Parameters(args): Parameters<ListWebhooksArgs>,
    ) -> Result<Json<WebhookList>, ToolFailure> {
        let stream = args.message_stream.map(MessageStreamId::try_from).transpose()?;
        Ok(Json(self.gateway.list_webhooks(stream.as_ref()).await?))
    }
}
