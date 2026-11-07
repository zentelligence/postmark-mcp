//! Tools for suppressed addresses.

use postmark_mcp_core::{
    DateRange, SuppressionList, SuppressionOrigin, SuppressionQuery, SuppressionReason, ValueError,
};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use super::{WithDefaultStream, filter, stream_or_default};
use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::ToolFailure;

/// Arguments of `list_suppressions`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSuppressionsArgs {
    /// Stream to read. Defaults to the configured default stream.
    message_stream: Option<String>,
    /// Restricts results to this reason.
    suppression_reason: Option<SuppressionReason>,
    /// Restricts results to this origin.
    origin: Option<SuppressionOrigin>,
    /// Restricts results by full or partial email address.
    email_address: Option<String>,
    /// Start date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    from_date: Option<String>,
    /// End date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    to_date: Option<String>,
}

impl TryFrom<WithDefaultStream<'_, ListSuppressionsArgs>> for SuppressionQuery {
    type Error = ValueError;

    fn try_from(
        WithDefaultStream { args, default_stream }: WithDefaultStream<'_, ListSuppressionsArgs>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            message_stream: stream_or_default(args.message_stream, default_stream)?,
            reason: args.suppression_reason,
            origin: args.origin,
            email_address: filter("email_address", args.email_address)?,
            dates: DateRange::try_from((args.from_date, args.to_date))?,
        })
    }
}

#[tool_router(router = suppression_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "list_suppressions",
        description = "List addresses suppressed on a message stream, which will not receive mail on it. Filter by reason, origin, address, or date range. This list is eventually consistent, so a very recent change may not appear yet.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<ListSuppressionsArgs>(),
        output_schema = fallible_output_schema_for::<SuppressionList>()
    )]
    async fn list_suppressions(
        &self,
        Parameters(args): Parameters<ListSuppressionsArgs>,
    ) -> Result<Json<SuppressionList>, ToolFailure> {
        let query = SuppressionQuery::try_from(WithDefaultStream {
            args,
            default_stream: &self.defaults.message_stream,
        })?;
        Ok(Json(self.gateway.list_suppressions(&query).await?))
    }
}
