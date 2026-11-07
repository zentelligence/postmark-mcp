//! Tools for outbound messages.

use postmark_mcp_core::{
    DateRange, MessageDetails, MessageList, MessageSearch, MessageStatus, Page, PageRequest, ResourceId, ValueError,
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

/// Arguments of `search_outbound_messages`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchMessagesArgs {
    /// Restricts results to this recipient address.
    recipient: Option<String>,
    /// Restricts results to this sender address.
    from_email: Option<String>,
    /// Restricts results to this tag.
    tag: Option<String>,
    /// Restricts results by subject.
    subject: Option<String>,
    /// Restricts results to this status.
    status: Option<MessageStatus>,
    /// Stream to search. Defaults to the configured default stream.
    message_stream: Option<String>,
    /// Start date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    from_date: Option<String>,
    /// End date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    to_date: Option<String>,
    /// Number of messages to return, from 1 to 500. Defaults to 50.
    count: Option<u32>,
    /// Number of messages to skip. count plus offset cannot exceed 10,000.
    offset: Option<u32>,
}

impl TryFrom<WithDefaultStream<'_, SearchMessagesArgs>> for MessageSearch {
    type Error = ValueError;

    fn try_from(
        WithDefaultStream { args, default_stream }: WithDefaultStream<'_, SearchMessagesArgs>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            page: Page::try_from(PageRequest {
                count: args.count,
                offset: args.offset,
                default_count: DEFAULT_COUNT,
            })?,
            message_stream: stream_or_default(args.message_stream, default_stream)?,
            recipient: filter("recipient", args.recipient)?,
            from_email: filter("from_email", args.from_email)?,
            tag: filter("tag", args.tag)?,
            subject: filter("subject", args.subject)?,
            status: args.status,
            dates: DateRange::try_from((args.from_date, args.to_date))?,
        })
    }
}

/// Arguments of `get_message_details`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MessageDetailsArgs {
    /// The message's ID, from `search_outbound_messages`.
    message_id: String,
}

impl TryFrom<MessageDetailsArgs> for ResourceId {
    type Error = ValueError;

    fn try_from(args: MessageDetailsArgs) -> Result<Self, Self::Error> {
        Self::try_from(("message_id", args.message_id))
    }
}

#[tool_router(router = message_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "search_outbound_messages",
        description = "Search outbound message history by recipient, sender, subject, tag, status, stream, or date range. Returns up to 500 messages per call, with total_count for paging. Use get_message_details for a message's event timeline.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<SearchMessagesArgs>(),
        output_schema = fallible_output_schema_for::<MessageList>()
    )]
    async fn search_outbound_messages(
        &self,
        Parameters(args): Parameters<SearchMessagesArgs>,
    ) -> Result<Json<MessageList>, ToolFailure> {
        let search = MessageSearch::try_from(WithDefaultStream {
            args,
            default_stream: &self.defaults.message_stream,
        })?;
        Ok(Json(self.gateway.search_outbound_messages(&search).await?))
    }

    #[tool(
        name = "get_message_details",
        description = "Retrieve one outbound message's delivery details and event timeline (delivered, opened, clicked, bounced, and so on). Message bodies and clicked link URLs are not included. Use search_outbound_messages to find a MessageID.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<MessageDetailsArgs>(),
        output_schema = fallible_output_schema_for::<MessageDetails>()
    )]
    async fn get_message_details(
        &self,
        Parameters(args): Parameters<MessageDetailsArgs>,
    ) -> Result<Json<MessageDetails>, ToolFailure> {
        let id = ResourceId::try_from(args)?;
        Ok(Json(self.gateway.message_details(&id).await?))
    }
}
