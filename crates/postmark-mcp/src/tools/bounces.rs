//! Tools for bounces.

use postmark_mcp_core::{
    BounceDump, BounceId, BounceList, BounceSearch, BounceType, DateRange, Page, PageRequest, ValueError,
};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use super::{WithDefaultStream, filter, stream_or_default};
use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::ToolFailure;

/// Page size when a caller does not choose one.
const DEFAULT_COUNT: u32 = 50;

/// Arguments of `search_bounces`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchBouncesArgs {
    /// Restricts results to this kind of bounce.
    r#type: Option<BounceType>,
    /// Restricts results to deactivated addresses, or to active ones.
    inactive: Option<bool>,
    /// Restricts results by full or partial email address.
    email_filter: Option<String>,
    /// Restricts results to this tag.
    tag: Option<String>,
    /// Restricts results to this original message.
    message_id: Option<String>,
    /// Stream to search. Defaults to the configured default stream.
    message_stream: Option<String>,
    /// Start date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    from_date: Option<String>,
    /// End date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    to_date: Option<String>,
    /// Number of bounces to return, from 1 to 500. Defaults to 50.
    count: Option<u32>,
    /// Number of bounces to skip. count plus offset cannot exceed 10,000.
    offset: Option<u32>,
}

impl TryFrom<WithDefaultStream<'_, SearchBouncesArgs>> for BounceSearch {
    type Error = ValueError;

    fn try_from(
        WithDefaultStream { args, default_stream }: WithDefaultStream<'_, SearchBouncesArgs>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            page: Page::try_from(PageRequest {
                count: args.count,
                offset: args.offset,
                default_count: DEFAULT_COUNT,
            })?,
            message_stream: stream_or_default(args.message_stream, default_stream)?,
            bounce_type: args.r#type,
            inactive: args.inactive,
            email_filter: filter("email_filter", args.email_filter)?,
            tag: filter("tag", args.tag)?,
            message_id: filter("message_id", args.message_id)?,
            dates: DateRange::try_from((args.from_date, args.to_date))?,
        })
    }
}

/// Arguments of `get_bounce_dump`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BounceDumpArgs {
    /// The bounce's ID, from `search_bounces`.
    bounce_id: u64,
}

impl TryFrom<BounceDumpArgs> for BounceId {
    type Error = ValueError;

    fn try_from(args: BounceDumpArgs) -> Result<Self, Self::Error> {
        Self::try_from(args.bounce_id)
    }
}

#[tool_router(router = bounce_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "search_bounces",
        description = "Search the bounce log by type, address, tag, message, stream, or date range. Returns each bounce's type, description, time, and whether the address can be reactivated. Postmark keeps bounce records for 45 days by default.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<SearchBouncesArgs>(),
        output_schema = fallible_output_schema_for::<BounceList>()
    )]
    async fn search_bounces(
        &self,
        Parameters(args): Parameters<SearchBouncesArgs>,
    ) -> Result<Json<BounceList>, ToolFailure> {
        let search = BounceSearch::try_from(WithDefaultStream {
            args,
            default_stream: &self.defaults.message_stream,
        })?;
        Ok(Json(self.gateway.search_bounces(&search).await?))
    }

    #[tool(
        name = "get_bounce_dump",
        description = "Retrieve the raw source of a bounce, showing how the remote server rejected the message. This can quote the original message, including its body, so request it only when the bounce details are not enough. Postmark keeps it for 30 days, after which available is false.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<BounceDumpArgs>(),
        output_schema = fallible_output_schema_for::<BounceDump>()
    )]
    async fn get_bounce_dump(
        &self,
        Parameters(args): Parameters<BounceDumpArgs>,
    ) -> Result<Json<BounceDump>, ToolFailure> {
        let id = BounceId::try_from(args)?;
        Ok(Json(self.gateway.bounce_dump(id).await?))
    }
}
