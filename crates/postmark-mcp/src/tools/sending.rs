//! Tools that send email. Every message is validated, and every recipient checked
//! against policy, before any request is made, so a batch either goes to Postmark
//! whole or not at all.

use postmark_mcp_core::{
    BatchOutcome, EmailRequest, Envelope, EnvelopeRequest, MAX_BATCH_MESSAGES, MailAddress, OutboundEmail,
    SendDefaults, SendReceipt, TemplateChoice, TemplatedEmail, ValueError,
};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::JsonObject;
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::{SendFailure, ToolFailure};

/// One address, or several, as a caller may send either.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl From<OneOrMany> for Vec<String> {
    fn from(value: OneOrMany) -> Self {
        match value {
            OneOrMany::One(address) => vec![address],
            OneOrMany::Many(addresses) => addresses,
        }
    }
}

fn listed(value: Option<OneOrMany>) -> Vec<String> {
    value.map(Vec::from).unwrap_or_default()
}

/// Arguments of `send_email`, and each message of `send_batch`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SendEmailArgs {
    /// Recipient, or an array of them. Up to 50 across to, cc, and bcc.
    #[schemars(with = "Vec<String>")]
    to: OneOrMany,
    /// Subject line.
    subject: String,
    /// Plain text body. Supply this, `html_body`, or both.
    text_body: Option<String>,
    /// HTML body. Supply this, `text_body`, or both.
    html_body: Option<String>,
    /// Sender, which must be verified in Postmark. Defaults to the configured sender.
    from: Option<String>,
    /// Copied recipient, or an array of them.
    #[schemars(with = "Option<Vec<String>>")]
    cc: Option<OneOrMany>,
    /// Blind copied recipient, or an array of them.
    #[schemars(with = "Option<Vec<String>>")]
    bcc: Option<OneOrMany>,
    /// Address for replies.
    reply_to: Option<String>,
    /// Tag for statistics.
    tag: Option<String>,
}

impl From<SendEmailArgs> for EmailRequest {
    fn from(args: SendEmailArgs) -> Self {
        Self {
            envelope: EnvelopeRequest {
                from: args.from,
                to: args.to.into(),
                cc: listed(args.cc),
                bcc: listed(args.bcc),
                reply_to: args.reply_to,
                tag: args.tag,
            },
            subject: args.subject,
            html_body: args.html_body,
            text_body: args.text_body,
        }
    }
}

/// Arguments of `send_email_with_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SendTemplateArgs {
    /// Recipient, or an array of them. Up to 50 across to, cc, and bcc.
    #[schemars(with = "Vec<String>")]
    to: OneOrMany,
    /// Numeric template ID. Supply this or `template_alias`, not both.
    template_id: Option<u64>,
    /// Template alias. Supply this or `template_id`, not both.
    template_alias: Option<String>,
    /// Values for the template's variables.
    template_model: JsonObject,
    /// Sender, which must be verified in Postmark. Defaults to the configured sender.
    from: Option<String>,
    /// Copied recipient, or an array of them.
    #[schemars(with = "Option<Vec<String>>")]
    cc: Option<OneOrMany>,
    /// Blind copied recipient, or an array of them.
    #[schemars(with = "Option<Vec<String>>")]
    bcc: Option<OneOrMany>,
    /// Address for replies.
    reply_to: Option<String>,
    /// Tag for statistics.
    tag: Option<String>,
}

/// Arguments of `send_batch`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SendBatchArgs {
    /// The messages, up to 500.
    messages: Vec<SendEmailArgs>,
}

/// One recipient of `send_batch_with_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateRecipientArgs {
    /// Recipient.
    #[schemars(with = "Vec<String>")]
    to: OneOrMany,
    /// Values for the template's variables, for this recipient.
    template_model: JsonObject,
    /// Sender for this recipient, overriding the shared one.
    from: Option<String>,
    /// Copied recipient, or an array of them.
    #[schemars(with = "Option<Vec<String>>")]
    cc: Option<OneOrMany>,
    /// Blind copied recipient, or an array of them.
    #[schemars(with = "Option<Vec<String>>")]
    bcc: Option<OneOrMany>,
    /// Address for replies.
    reply_to: Option<String>,
    /// Tag for this recipient, overriding the shared one.
    tag: Option<String>,
}

/// Arguments of `send_batch_with_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SendTemplateBatchArgs {
    /// Numeric template ID. Supply this or `template_alias`, not both.
    template_id: Option<u64>,
    /// Template alias. Supply this or `template_id`, not both.
    template_alias: Option<String>,
    /// Sender for every message, unless a recipient overrides it. Defaults to the configured sender.
    from: Option<String>,
    /// Tag for every message, unless a recipient overrides it.
    tag: Option<String>,
    /// The recipients, up to 500, each with their own template model.
    recipients: Vec<TemplateRecipientArgs>,
}

fn check_batch_size(size: usize) -> Result<(), ValueError> {
    if (1..=MAX_BATCH_MESSAGES).contains(&size) {
        Ok(())
    } else {
        Err(ValueError::BatchSizeOutOfRange {
            limit: MAX_BATCH_MESSAGES,
        })
    }
}

impl PostmarkMcpServer {
    fn send_defaults(&self) -> SendDefaults<'_> {
        SendDefaults {
            sender: self.defaults.sender.as_ref(),
            message_stream: &self.defaults.message_stream,
        }
    }

    /// Refuses the send if any recipient falls outside the operator's allowlist.
    fn check_recipients<'a>(&self, recipients: impl Iterator<Item = &'a MailAddress>) -> Result<(), ToolFailure> {
        let Some(allowlist) = &self.policy.recipient_allowlist else {
            return Ok(());
        };
        match recipients.into_iter().find(|address| !allowlist.permits(address)) {
            Some(address) => Err(ToolFailure::recipient_not_allowed(address)),
            None => Ok(()),
        }
    }
}

#[tool_router(router = sending_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "send_email",
        description = "Send one email through Postmark. This cannot be undone. Up to 50 recipients across to, cc, and bcc. The sender must be verified in Postmark. Open and link tracking follow the Postmark server's settings. If this fails with send/outcome-unknown, the email may have gone out, so check before resending. Use send_batch for several different messages.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        input_schema = input_schema_for::<SendEmailArgs>(),
        output_schema = fallible_output_schema_for::<SendReceipt>()
    )]
    async fn send_email(&self, Parameters(args): Parameters<SendEmailArgs>) -> Result<Json<SendReceipt>, ToolFailure> {
        let email = OutboundEmail::try_from((EmailRequest::from(args), self.send_defaults()))?;
        self.check_recipients(email.recipients().iter())?;
        Ok(Json(self.gateway.send_email(&email).await.map_err(SendFailure)?))
    }

    #[tool(
        name = "send_email_with_template",
        description = "Send one email rendered from a saved Postmark template. This cannot be undone. Supply template_id or template_alias, plus template_model with the template's variables. If this fails with send/outcome-unknown, the email may have gone out, so check before resending.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        input_schema = input_schema_for::<SendTemplateArgs>(),
        output_schema = fallible_output_schema_for::<SendReceipt>()
    )]
    async fn send_email_with_template(
        &self,
        Parameters(args): Parameters<SendTemplateArgs>,
    ) -> Result<Json<SendReceipt>, ToolFailure> {
        let template = TemplateChoice::try_from((args.template_id, args.template_alias))?;
        let request = EnvelopeRequest {
            from: args.from,
            to: args.to.into(),
            cc: listed(args.cc),
            bcc: listed(args.bcc),
            reply_to: args.reply_to,
            tag: args.tag,
        };
        let envelope = (request, self.send_defaults()).try_into()?;
        let email = TemplatedEmail::new(envelope, &template, args.template_model);
        self.check_recipients(email.recipients().iter())?;
        Ok(Json(
            self.gateway.send_templated_email(&email).await.map_err(SendFailure)?,
        ))
    }

    #[tool(
        name = "send_batch",
        description = "Send up to 500 separate emails in one call. This cannot be undone. Every message is validated first, so one invalid message refuses the whole batch and nothing is sent. Postmark then accepts or refuses each message on its own, so check failed and unaccounted in the result. If this fails with send/outcome-unknown, any of the emails may have gone out.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        input_schema = input_schema_for::<SendBatchArgs>(),
        output_schema = fallible_output_schema_for::<BatchOutcome>()
    )]
    async fn send_batch(&self, Parameters(args): Parameters<SendBatchArgs>) -> Result<Json<BatchOutcome>, ToolFailure> {
        check_batch_size(args.messages.len())?;
        let defaults = self.send_defaults();
        let emails = args
            .messages
            .into_iter()
            .enumerate()
            .map(|(index, message)| {
                OutboundEmail::try_from((EmailRequest::from(message), defaults))
                    .map_err(|error| ToolFailure::from(error).for_message(index))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.check_recipients(emails.iter().flat_map(OutboundEmail::recipients))?;
        Ok(Json(self.gateway.send_batch(&emails).await.map_err(SendFailure)?))
    }

    #[tool(
        name = "send_batch_with_template",
        description = "Send one saved Postmark template to up to 500 recipients in one call, each with their own template_model. This cannot be undone. Every recipient is validated first, so one invalid recipient refuses the whole batch and nothing is sent. Postmark then accepts or refuses each message on its own, so check failed and unaccounted in the result. If this fails with send/outcome-unknown, any of the emails may have gone out.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        input_schema = input_schema_for::<SendTemplateBatchArgs>(),
        output_schema = fallible_output_schema_for::<BatchOutcome>()
    )]
    async fn send_batch_with_template(
        &self,
        Parameters(args): Parameters<SendTemplateBatchArgs>,
    ) -> Result<Json<BatchOutcome>, ToolFailure> {
        let template = TemplateChoice::try_from((args.template_id, args.template_alias))?;
        check_batch_size(args.recipients.len())?;
        let defaults = self.send_defaults();
        let mut emails = Vec::with_capacity(args.recipients.len());
        for (index, recipient) in args.recipients.into_iter().enumerate() {
            let request = EnvelopeRequest {
                from: recipient.from.or_else(|| args.from.clone()),
                to: recipient.to.into(),
                cc: listed(recipient.cc),
                bcc: listed(recipient.bcc),
                reply_to: recipient.reply_to,
                tag: recipient.tag.or_else(|| args.tag.clone()),
            };
            let envelope =
                Envelope::try_from((request, defaults)).map_err(|error| ToolFailure::from(error).for_message(index))?;
            emails.push(TemplatedEmail::new(envelope, &template, recipient.template_model));
        }
        self.check_recipients(emails.iter().flat_map(TemplatedEmail::recipients))?;
        Ok(Json(
            self.gateway.send_templated_batch(&emails).await.map_err(SendFailure)?,
        ))
    }
}
