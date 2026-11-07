//! The delivery diagnosis tool. The diagnosis itself lives in core.

use postmark_mcp_core::{
    DateRange, DeliveryDiagnosis, DiagnosisQuery, RecipientAddress, ResourceId, ValueError, diagnose_delivery,
};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use super::{WithDefaultStream, stream_or_default};
use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::ToolFailure;

/// Arguments of `diagnose_delivery`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiagnoseArgs {
    /// The recipient address to investigate.
    recipient: String,
    /// A specific message to investigate, instead of the most recent one found. Cannot be combined with dates.
    message_id: Option<String>,
    /// Stream to investigate. Defaults to the configured default stream. A given message's own stream takes precedence.
    message_stream: Option<String>,
    /// Start date, inclusive, as `YYYY-MM-DD` in Eastern Time. All retained history is searched if both dates are omitted.
    from_date: Option<String>,
    /// End date, inclusive, as `YYYY-MM-DD` in Eastern Time.
    to_date: Option<String>,
}

impl TryFrom<WithDefaultStream<'_, DiagnoseArgs>> for DiagnosisQuery {
    type Error = ValueError;

    fn try_from(
        WithDefaultStream { args, default_stream }: WithDefaultStream<'_, DiagnoseArgs>,
    ) -> Result<Self, Self::Error> {
        if args.message_id.is_some() && (args.from_date.is_some() || args.to_date.is_some()) {
            return Err(ValueError::MessageIdWithDates);
        }
        Ok(Self {
            recipient: RecipientAddress::try_from(args.recipient)?,
            message_id: args
                .message_id
                .map(|id| ResourceId::try_from(("message_id", id)))
                .transpose()?,
            message_stream: stream_or_default(args.message_stream, default_stream)?,
            dates: DateRange::try_from((args.from_date, args.to_date))?,
        })
    }
}

#[tool_router(router = diagnosis_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "diagnose_delivery",
        description = "Diagnose why an email may not have reached a recipient. Checks the most recent message and its events, then suppressions and bounce history for this exact address, and gives a verdict and a recommendation. It covers one message, so give message_id to ask about an older one. Anything that could not be established is listed in incomplete, so treat absent data as unknown when that list is not empty. Use this first when a recipient reports a missing or bounced email.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<DiagnoseArgs>(),
        output_schema = fallible_output_schema_for::<DeliveryDiagnosis>()
    )]
    async fn diagnose_delivery(
        &self,
        Parameters(args): Parameters<DiagnoseArgs>,
    ) -> Result<Json<DeliveryDiagnosis>, ToolFailure> {
        let query = DiagnosisQuery::try_from(WithDefaultStream {
            args,
            default_stream: &self.defaults.message_stream,
        })?;
        Ok(Json(diagnose_delivery(self.gateway.as_ref(), &query).await?))
    }
}
