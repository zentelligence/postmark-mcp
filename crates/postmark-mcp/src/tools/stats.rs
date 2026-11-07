//! Tools for delivery statistics.

use postmark_mcp_core::{DateRange, DeliveryStat, DeliveryStats, MessageStreamId, StatsQuery, ValueError};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use super::filter;
use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::ToolFailure;

/// Arguments of `get_delivery_stats`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeliveryStatsArgs {
    /// Statistic to report. Defaults to `summary`, which adds headline rates.
    stat: Option<DeliveryStat>,
    /// Restricts results to this tag.
    tag: Option<String>,
    /// Restricts results to this stream. All streams are covered otherwise.
    message_stream: Option<String>,
    /// Start date, inclusive, as `YYYY-MM-DD` in Eastern Time. All time is covered if both dates are omitted.
    from_date: Option<String>,
    /// End date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    to_date: Option<String>,
}

impl TryFrom<DeliveryStatsArgs> for StatsQuery {
    type Error = ValueError;

    fn try_from(args: DeliveryStatsArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            stat: args.stat.unwrap_or_default(),
            tag: filter("tag", args.tag)?,
            message_stream: args.message_stream.map(MessageStreamId::try_from).transpose()?,
            dates: DateRange::try_from((args.from_date, args.to_date))?,
        })
    }
}

#[tool_router(router = stats_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "get_delivery_stats",
        description = "Retrieve outbound email statistics. The default summary reports totals with open, click, bounce, and spam rates. Other stat values give focused breakdowns, such as opens by email client. Filter by tag, stream, or date range.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<DeliveryStatsArgs>(),
        output_schema = fallible_output_schema_for::<DeliveryStats>()
    )]
    async fn get_delivery_stats(
        &self,
        Parameters(args): Parameters<DeliveryStatsArgs>,
    ) -> Result<Json<DeliveryStats>, ToolFailure> {
        let query = StatsQuery::try_from(args)?;
        Ok(Json(self.gateway.delivery_stats(&query).await?))
    }
}
