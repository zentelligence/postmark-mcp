//! Implements the domain ports against the Postmark API. Result types decode
//! straight from responses, so each method only names the route and parameters.

use async_trait::async_trait;
use postmark_mcp_core::{
    BatchItem, BatchOutcome, Bounce, BounceDump, BounceId, BounceList, BounceSearch, ChangesBounces,
    ChangesSuppressions, ChangesTemplates, ChangesWebhooks, Deletion, DeliveryStat, DeliveryStats, GatewayError,
    JsonObject, MessageDetails, MessageList, MessageSearch, MessageStreamId, NewTemplate, NewWebhook, OutboundEmail,
    ReadsBounces, ReadsMessages, ReadsServer, ReadsStats, ReadsSuppressions, ReadsTemplates, ReadsWebhooks, ResourceId,
    SendReceipt, SendsEmail, ServerSummary, StatsQuery, SuppressionChange, SuppressionList, SuppressionQuery,
    SuppressionResponse, SuppressionResults, Template, TemplateChange, TemplateList, TemplateQuery, TemplateSummary,
    TemplateUpdate, TemplateValidation, TemplatedEmail, ValidationResult, Webhook, WebhookId, WebhookList,
};
use serde::{Deserialize, Serialize};

use crate::base_url::QueryParams;
use crate::transport::HardenedTransport;

/// The Postmark API, reached through a [`HardenedTransport`].
#[derive(Clone, Debug)]
pub struct PostmarkApi {
    transport: HardenedTransport,
}

impl From<HardenedTransport> for PostmarkApi {
    fn from(transport: HardenedTransport) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl ReadsServer for PostmarkApi {
    async fn current_server(&self) -> Result<ServerSummary, GatewayError> {
        self.transport.get(&["server"], &QueryParams::default()).await
    }
}

#[async_trait]
impl ReadsTemplates for PostmarkApi {
    async fn list_templates(&self, query: &TemplateQuery) -> Result<TemplateList, GatewayError> {
        let params = QueryParams::default()
            .with("count", query.page.count)
            .with("offset", query.page.offset)
            .with_optional("TemplateType", query.template_type)
            .with_optional("LayoutTemplate", query.layout_template.as_ref());
        self.transport.get(&["templates"], &params).await
    }

    async fn get_template(&self, id_or_alias: &ResourceId) -> Result<Template, GatewayError> {
        self.transport
            .get(&["templates", id_or_alias.as_ref()], &QueryParams::default())
            .await
    }

    async fn validate_template(&self, content: &TemplateValidation) -> Result<ValidationResult, GatewayError> {
        self.transport.post(&["templates", "validate"], content).await
    }
}

#[async_trait]
impl ReadsMessages for PostmarkApi {
    async fn search_outbound_messages(&self, search: &MessageSearch) -> Result<MessageList, GatewayError> {
        let params = QueryParams::default()
            .with("count", search.page.count)
            .with("offset", search.page.offset)
            .with("messagestream", &search.message_stream)
            .with_optional("recipient", search.recipient.as_ref())
            .with_optional("fromemail", search.from_email.as_ref())
            .with_optional("tag", search.tag.as_ref())
            .with_optional("subject", search.subject.as_ref())
            .with_optional("status", search.status)
            .with_optional("fromdate", search.dates.from.as_ref())
            .with_optional("todate", search.dates.to.as_ref());
        self.transport.get(&["messages", "outbound"], &params).await
    }

    async fn message_details(&self, message_id: &ResourceId) -> Result<MessageDetails, GatewayError> {
        self.transport
            .get(
                &["messages", "outbound", message_id.as_ref(), "details"],
                &QueryParams::default(),
            )
            .await
    }
}

/// Response of `GET /bounces/{id}/dump`.
#[derive(Debug, Deserialize)]
struct DumpResponse {
    #[serde(rename = "Body", default)]
    body: Option<String>,
}

#[async_trait]
impl ReadsBounces for PostmarkApi {
    async fn search_bounces(&self, search: &BounceSearch) -> Result<BounceList, GatewayError> {
        let params = QueryParams::default()
            .with("count", search.page.count)
            .with("offset", search.page.offset)
            .with("messagestream", &search.message_stream)
            .with_optional("type", search.bounce_type)
            .with_optional("inactive", search.inactive)
            .with_optional("emailFilter", search.email_filter.as_ref())
            .with_optional("tag", search.tag.as_ref())
            .with_optional("messageID", search.message_id.as_ref())
            .with_optional("fromdate", search.dates.from.as_ref())
            .with_optional("todate", search.dates.to.as_ref());
        self.transport.get(&["bounces"], &params).await
    }

    async fn bounce_dump(&self, bounce_id: BounceId) -> Result<BounceDump, GatewayError> {
        let id = bounce_id.to_string();
        let response: DumpResponse = self
            .transport
            .get(&["bounces", &id, "dump"], &QueryParams::default())
            .await?;
        // Postmark reports an unavailable dump as an empty body.
        let body = response.body.filter(|text| !text.is_empty());
        Ok(BounceDump {
            bounce_id: bounce_id.get(),
            available: body.is_some(),
            body,
        })
    }
}

#[async_trait]
impl ReadsSuppressions for PostmarkApi {
    async fn list_suppressions(&self, query: &SuppressionQuery) -> Result<SuppressionList, GatewayError> {
        let params = QueryParams::default()
            .with_optional("SuppressionReason", query.reason)
            .with_optional("Origin", query.origin)
            .with_optional("EmailAddress", query.email_address.as_ref())
            .with_optional("fromdate", query.dates.from.as_ref())
            .with_optional("todate", query.dates.to.as_ref());
        let stream = query.message_stream.as_ref();
        let mut list: SuppressionList = self
            .transport
            .get(&["message-streams", stream, "suppressions", "dump"], &params)
            .await?;
        stream.clone_into(&mut list.message_stream);
        Ok(list)
    }
}

/// Route of each statistic. `open_read_times` is the exception among these, in
/// that Postmark's API reference does not list it; the official server uses it.
fn stat_route(stat: DeliveryStat) -> &'static [&'static str] {
    match stat {
        DeliveryStat::Summary | DeliveryStat::Overview => &["stats", "outbound"],
        DeliveryStat::Sent => &["stats", "outbound", "sends"],
        DeliveryStat::Bounces => &["stats", "outbound", "bounces"],
        DeliveryStat::Spam => &["stats", "outbound", "spam"],
        DeliveryStat::Tracked => &["stats", "outbound", "tracked"],
        DeliveryStat::Opens => &["stats", "outbound", "opens"],
        DeliveryStat::OpenPlatforms => &["stats", "outbound", "opens", "platforms"],
        DeliveryStat::OpenClients => &["stats", "outbound", "opens", "emailclients"],
        DeliveryStat::OpenReadTimes => &["stats", "outbound", "opens", "readtimes"],
        DeliveryStat::Clicks => &["stats", "outbound", "clicks"],
        DeliveryStat::ClickBrowsers => &["stats", "outbound", "clicks", "browserfamilies"],
        DeliveryStat::ClickPlatforms => &["stats", "outbound", "clicks", "platforms"],
        DeliveryStat::ClickLocation => &["stats", "outbound", "clicks", "location"],
    }
}

#[async_trait]
impl ReadsStats for PostmarkApi {
    async fn delivery_stats(&self, query: &StatsQuery) -> Result<DeliveryStats, GatewayError> {
        let params = QueryParams::default()
            .with_optional("tag", query.tag.as_ref())
            .with_optional("messagestream", query.message_stream.as_ref())
            .with_optional("fromdate", query.dates.from.as_ref())
            .with_optional("todate", query.dates.to.as_ref());
        let response: JsonObject = self.transport.get(stat_route(query.stat), &params).await?;
        Ok((query.stat, response).into())
    }
}

#[async_trait]
impl SendsEmail for PostmarkApi {
    async fn send_email(&self, email: &OutboundEmail) -> Result<SendReceipt, GatewayError> {
        let item: BatchItem = self.transport.post(&["email"], email).await?;
        item.try_into()
    }

    async fn send_templated_email(&self, email: &TemplatedEmail) -> Result<SendReceipt, GatewayError> {
        let item: BatchItem = self.transport.post(&["email", "withTemplate"], email).await?;
        item.try_into()
    }

    async fn send_batch(&self, emails: &[OutboundEmail]) -> Result<BatchOutcome, GatewayError> {
        let items: Vec<BatchItem> = self.transport.post(&["email", "batch"], &emails).await?;
        Ok((items, emails.iter().map(OutboundEmail::to).collect()).into())
    }

    async fn send_templated_batch(&self, emails: &[TemplatedEmail]) -> Result<BatchOutcome, GatewayError> {
        let body = TemplatedBatch { messages: emails };
        let items: Vec<BatchItem> = self.transport.post(&["email", "batchWithTemplates"], &body).await?;
        Ok((items, emails.iter().map(TemplatedEmail::to).collect()).into())
    }
}

/// Body of `POST /email/batchWithTemplates`.
#[derive(Debug, Serialize)]
struct TemplatedBatch<'a> {
    #[serde(rename = "Messages")]
    messages: &'a [TemplatedEmail],
}

#[async_trait]
impl ReadsWebhooks for PostmarkApi {
    async fn list_webhooks(&self, message_stream: Option<&MessageStreamId>) -> Result<WebhookList, GatewayError> {
        let params = QueryParams::default().with_optional("MessageStream", message_stream);
        self.transport.get(&["webhooks"], &params).await
    }
}

/// Postmark's plain acknowledgement, as a delete returns. It can carry a refusal
/// with a success status, so the code decides.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Acknowledgement {
    error_code: Option<i64>,
    #[serde(default)]
    message: Option<String>,
}

impl From<Acknowledgement> for Result<(), GatewayError> {
    fn from(ack: Acknowledgement) -> Self {
        match ack.error_code {
            Some(0) => Ok(()),
            None => Err(GatewayError::InvalidResponse {
                detail: "Postmark's reply did not confirm the change".to_owned(),
            }),
            Some(code) => Err(GatewayError::Rejected {
                status: 200,
                error_code: Some(code),
                message: ack.message.unwrap_or_default(),
            }),
        }
    }
}

#[async_trait]
impl ChangesTemplates for PostmarkApi {
    async fn create_template(&self, template: &NewTemplate) -> Result<TemplateSummary, GatewayError> {
        self.transport.post(&["templates"], template).await
    }

    async fn edit_template(
        &self,
        id_or_alias: &ResourceId,
        change: &TemplateChange,
    ) -> Result<TemplateSummary, GatewayError> {
        let current = self.get_template(id_or_alias).await?;
        let update = TemplateUpdate::from((current, change));
        self.transport
            .put(&["templates", id_or_alias.as_ref()], Some(&update))
            .await
    }

    async fn delete_template(&self, id_or_alias: &ResourceId) -> Result<Deletion, GatewayError> {
        let ack: Acknowledgement = self.transport.delete(&["templates", id_or_alias.as_ref()]).await?;
        Result::<(), GatewayError>::from(ack)?;
        Ok(Deletion {
            deleted: id_or_alias.as_ref().to_owned(),
        })
    }
}

/// Response of `PUT /bounces/{id}/activate`.
#[derive(Debug, Deserialize)]
struct Activation {
    #[serde(rename = "Bounce")]
    bounce: Bounce,
}

#[async_trait]
impl ChangesBounces for PostmarkApi {
    async fn activate_bounce(&self, bounce_id: BounceId) -> Result<Bounce, GatewayError> {
        let id = bounce_id.to_string();
        let activation: Activation = self.transport.put::<(), _>(&["bounces", &id, "activate"], None).await?;
        Ok(activation.bounce)
    }
}

/// Body of both suppression changes.
#[derive(Debug, Serialize)]
struct SuppressionRequest {
    #[serde(rename = "Suppressions")]
    suppressions: Vec<SuppressionEntry>,
}

#[derive(Debug, Serialize)]
struct SuppressionEntry {
    #[serde(rename = "EmailAddress")]
    email_address: String,
}

impl From<&SuppressionChange> for SuppressionRequest {
    fn from(change: &SuppressionChange) -> Self {
        Self {
            suppressions: change
                .addresses
                .iter()
                .map(|address| SuppressionEntry {
                    email_address: address.to_string(),
                })
                .collect(),
        }
    }
}

#[async_trait]
impl ChangesSuppressions for PostmarkApi {
    async fn create_suppressions(&self, change: &SuppressionChange) -> Result<SuppressionResults, GatewayError> {
        let stream = change.message_stream.as_ref();
        let response: SuppressionResponse = self
            .transport
            .post(
                &["message-streams", stream, "suppressions"],
                &SuppressionRequest::from(change),
            )
            .await?;
        Ok((&change.message_stream, response).into())
    }

    async fn delete_suppressions(&self, change: &SuppressionChange) -> Result<SuppressionResults, GatewayError> {
        let stream = change.message_stream.as_ref();
        let response: SuppressionResponse = self
            .transport
            .post(
                &["message-streams", stream, "suppressions", "delete"],
                &SuppressionRequest::from(change),
            )
            .await?;
        Ok((&change.message_stream, response).into())
    }
}

#[async_trait]
impl ChangesWebhooks for PostmarkApi {
    async fn create_webhook(&self, webhook: &NewWebhook) -> Result<Webhook, GatewayError> {
        self.transport.post(&["webhooks"], webhook).await
    }

    async fn delete_webhook(&self, webhook_id: WebhookId) -> Result<Deletion, GatewayError> {
        let id = webhook_id.to_string();
        let ack: Acknowledgement = self.transport.delete(&["webhooks", &id]).await?;
        Result::<(), GatewayError>::from(ack)?;
        Ok(Deletion { deleted: id })
    }
}
