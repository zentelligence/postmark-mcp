//! Sending email.
//!
//! Sending cannot be undone, so every message is fully validated, and every
//! recipient is available for policy checks, before anything leaves this process.
//! Open and link tracking are deliberately not set, so each Postmark server's own
//! settings decide, rather than this server opting recipients in.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::wire::{JsonObject, blank_as_none};
use crate::{GatewayError, MailAddress, MessageStreamId, ResourceId, ValueError};

/// Postmark's limit on recipients across `To`, `Cc`, and `Bcc`.
const MAX_RECIPIENTS: usize = 50;

/// Postmark's limit on a subject, in characters.
const MAX_SUBJECT_LEN: usize = 2000;

/// Postmark's limit on a tag, in characters.
const MAX_TAG_LEN: usize = 1000;

/// Postmark's limit on messages in one batch.
pub const MAX_BATCH_MESSAGES: usize = 500;

/// Values applied when a message does not supply its own.
#[derive(Clone, Copy, Debug)]
pub struct SendDefaults<'a> {
    /// Sender of the message.
    pub sender: &'a str,
    /// Stream the message is sent on.
    pub message_stream: &'a MessageStreamId,
}

/// The addressing of a message, before validation.
#[derive(Clone, Debug, Default)]
pub struct EnvelopeRequest {
    /// Sender, if not the default.
    pub from: Option<String>,
    /// Primary recipients.
    pub to: Vec<String>,
    /// Copied recipients.
    pub cc: Vec<String>,
    /// Blind copied recipients.
    pub bcc: Vec<String>,
    /// Address for replies.
    pub reply_to: Option<String>,
    /// Tag for statistics.
    pub tag: Option<String>,
}

/// The validated addressing of a message, in the shape Postmark takes it.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Envelope {
    from: String,
    to: String,
    cc: Option<String>,
    bcc: Option<String>,
    reply_to: Option<String>,
    tag: Option<String>,
    message_stream: String,
    /// Everyone this message reaches, for policy checks.
    #[serde(skip)]
    recipients: Vec<MailAddress>,
}

fn addresses(field: &'static str, values: Vec<String>) -> Result<Vec<MailAddress>, ValueError> {
    values
        .into_iter()
        .map(|value| MailAddress::try_from((field, value)))
        .collect()
}

fn joined(addresses: &[MailAddress]) -> Option<String> {
    (!addresses.is_empty()).then(|| addresses.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))
}

fn validated_line(field: &'static str, value: String, limit: usize) -> Result<String, ValueError> {
    if value.trim().is_empty() {
        return Err(ValueError::Empty { field });
    }
    if value.chars().any(char::is_control) {
        return Err(ValueError::ControlCharacters { field });
    }
    if value.chars().count() > limit {
        return Err(ValueError::TooLong { field, limit });
    }
    Ok(value)
}

impl TryFrom<(EnvelopeRequest, SendDefaults<'_>)> for Envelope {
    type Error = ValueError;

    fn try_from((request, defaults): (EnvelopeRequest, SendDefaults<'_>)) -> Result<Self, Self::Error> {
        let to = addresses("to", request.to)?;
        let cc = addresses("cc", request.cc)?;
        let bcc = addresses("bcc", request.bcc)?;
        let Some(primary) = joined(&to) else {
            return Err(ValueError::Empty { field: "to" });
        };
        if to.len() + cc.len() + bcc.len() > MAX_RECIPIENTS {
            return Err(ValueError::TooManyRecipients { limit: MAX_RECIPIENTS });
        }
        let from = request
            .from
            .map(|from| MailAddress::try_from(("from", from)).map(|address| address.to_string()))
            .transpose()?
            .unwrap_or_else(|| defaults.sender.to_owned());
        let reply_to = request
            .reply_to
            .map(|address| MailAddress::try_from(("reply_to", address)).map(|address| address.to_string()))
            .transpose()?;
        Ok(Self {
            from,
            cc: joined(&cc),
            bcc: joined(&bcc),
            reply_to,
            tag: request
                .tag
                .map(|tag| validated_line("tag", tag, MAX_TAG_LEN))
                .transpose()?,
            message_stream: defaults.message_stream.to_string(),
            to: primary,
            recipients: to.into_iter().chain(cc).chain(bcc).collect(),
        })
    }
}

/// A message with its own content, before validation.
#[derive(Clone, Debug)]
pub struct EmailRequest {
    /// Addressing of the message.
    pub envelope: EnvelopeRequest,
    /// Subject line.
    pub subject: String,
    /// HTML body.
    pub html_body: Option<String>,
    /// Plain text body.
    pub text_body: Option<String>,
}

/// A validated message, in the shape Postmark takes it.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OutboundEmail {
    #[serde(flatten)]
    envelope: Envelope,
    subject: String,
    html_body: Option<String>,
    text_body: Option<String>,
}

impl TryFrom<(EmailRequest, SendDefaults<'_>)> for OutboundEmail {
    type Error = ValueError;

    fn try_from((request, defaults): (EmailRequest, SendDefaults<'_>)) -> Result<Self, Self::Error> {
        let html_body = request.html_body.filter(|body| !body.is_empty());
        let text_body = request.text_body.filter(|body| !body.is_empty());
        if html_body.is_none() && text_body.is_none() {
            return Err(ValueError::NoMessageBody);
        }
        Ok(Self {
            envelope: (request.envelope, defaults).try_into()?,
            subject: validated_line("subject", request.subject, MAX_SUBJECT_LEN)?,
            html_body,
            text_body,
        })
    }
}

impl OutboundEmail {
    /// Everyone this message reaches.
    #[must_use]
    pub fn recipients(&self) -> &[MailAddress] {
        &self.envelope.recipients
    }

    /// The `To` value, for reporting a batch result Postmark leaves it out of.
    #[must_use]
    pub fn to(&self) -> &str {
        &self.envelope.to
    }
}

/// Which template to send.
#[derive(Clone, Debug)]
pub enum TemplateChoice {
    /// A template by numeric ID.
    Id(u64),
    /// A template by alias.
    Alias(ResourceId),
}

impl TryFrom<(Option<u64>, Option<String>)> for TemplateChoice {
    type Error = ValueError;

    fn try_from((id, alias): (Option<u64>, Option<String>)) -> Result<Self, Self::Error> {
        match (id, alias) {
            (Some(0), _) | (None, None) => Err(ValueError::TemplateNotChosen),
            (Some(_), Some(_)) => Err(ValueError::TemplateChosenTwice),
            (Some(id), None) => Ok(Self::Id(id)),
            (None, Some(alias)) => ResourceId::try_from(("template_alias", alias)).map(Self::Alias),
        }
    }
}

/// A validated template message, in the shape Postmark takes it.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TemplatedEmail {
    #[serde(flatten)]
    envelope: Envelope,
    template_id: Option<u64>,
    template_alias: Option<String>,
    template_model: JsonObject,
}

impl TemplatedEmail {
    /// Creates a template message.
    #[must_use]
    pub fn new(envelope: Envelope, template: &TemplateChoice, template_model: JsonObject) -> Self {
        let (template_id, template_alias) = match template {
            TemplateChoice::Id(id) => (Some(*id), None),
            TemplateChoice::Alias(alias) => (None, Some(alias.as_ref().to_owned())),
        };
        Self {
            envelope,
            template_id,
            template_alias,
            template_model,
        }
    }

    /// Everyone this message reaches.
    #[must_use]
    pub fn recipients(&self) -> &[MailAddress] {
        &self.envelope.recipients
    }

    /// The `To` value, for reporting a batch result Postmark leaves it out of.
    #[must_use]
    pub fn to(&self) -> &str {
        &self.envelope.to
    }
}

/// Postmark's confirmation that it accepted a message.
#[skip_serializing_none]
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct SendReceipt {
    /// Postmark's identifier for the message.
    pub message_id: String,
    /// Who the message was addressed to.
    pub to: Option<String>,
    /// When Postmark accepted the message.
    pub submitted_at: Option<String>,
}

/// Postmark's answer about one message.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct BatchItem {
    /// Position of the message in the request, counting from 0.
    #[serde(default, skip_deserializing)]
    pub index: usize,
    /// Who the message was addressed to.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub to: Option<String>,
    /// `0` if accepted. Anything else means this message was refused.
    pub error_code: Option<i64>,
    /// Postmark's description of the outcome.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message: Option<String>,
    /// Postmark's identifier for the message, if accepted.
    #[serde(rename(serialize = "message_id", deserialize = "MessageID"))]
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message_id: Option<String>,
    /// When Postmark accepted the message.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub submitted_at: Option<String>,
}

impl TryFrom<BatchItem> for SendReceipt {
    type Error = GatewayError;

    fn try_from(item: BatchItem) -> Result<Self, Self::Error> {
        match (item.error_code, item.message_id) {
            (Some(0), Some(message_id)) => Ok(Self {
                message_id,
                to: item.to,
                submitted_at: item.submitted_at,
            }),
            (Some(0) | None, _) => Err(GatewayError::InvalidResponse {
                detail: "Postmark's reply did not confirm the message".to_owned(),
            }),
            (Some(code), _) => Err(GatewayError::Rejected {
                status: 200,
                error_code: Some(code),
                message: item.message.unwrap_or_default(),
            }),
        }
    }
}

/// Postmark's answer about a batch. Each message succeeds or fails on its own.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct BatchOutcome {
    /// Messages in the request.
    pub total: usize,
    /// Messages Postmark accepted.
    pub succeeded: usize,
    /// Messages Postmark refused.
    pub failed: usize,
    /// Messages Postmark gave no clear answer for. Do not assume these were sent, or
    /// that they were not: check with `search_outbound_messages` before resending.
    pub unaccounted: usize,
    /// Each answer, in request order.
    pub results: Vec<BatchItem>,
}

/// Postmark answers in request order, and may leave `To` out of a refusal, so the
/// address requested at each position fills that gap.
impl From<(Vec<BatchItem>, Vec<&str>)> for BatchOutcome {
    fn from((items, requested): (Vec<BatchItem>, Vec<&str>)) -> Self {
        let total = requested.len();
        // Answers beyond the messages sent cannot belong to any of them.
        let results: Vec<_> = items
            .into_iter()
            .take(total)
            .enumerate()
            .map(|(index, mut item)| {
                item.index = index;
                if item.to.is_none() {
                    item.to = requested.get(index).map(|to| (*to).to_owned());
                }
                item
            })
            .collect();
        let succeeded = results.iter().filter(|item| item.error_code == Some(0)).count();
        // Only a non-zero code is a refusal. An answer without a code says nothing
        // about whether the message went out.
        let failed = results
            .iter()
            .filter(|item| item.error_code.is_some_and(|code| code != 0))
            .count();
        Self {
            total,
            succeeded,
            failed,
            unaccounted: total - succeeded - failed,
            results,
        }
    }
}

#[cfg(test)]
#[path = "sending_test.rs"]
mod tests;

/// Sends email.
#[async_trait]
pub trait SendsEmail: Send + Sync {
    /// Sends one message.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the send fails. Use
    /// [`GatewayError::send_outcome_is_known`] before suggesting a resend.
    async fn send_email(&self, email: &OutboundEmail) -> Result<SendReceipt, GatewayError>;

    /// Sends one message from a template.
    ///
    /// # Errors
    ///
    /// As for [`SendsEmail::send_email`].
    async fn send_templated_email(&self, email: &TemplatedEmail) -> Result<SendReceipt, GatewayError>;

    /// Sends a batch of messages.
    ///
    /// # Errors
    ///
    /// As for [`SendsEmail::send_email`], and only when the whole batch fails.
    async fn send_batch(&self, emails: &[OutboundEmail]) -> Result<BatchOutcome, GatewayError>;

    /// Sends a batch of messages from templates.
    ///
    /// # Errors
    ///
    /// As for [`SendsEmail::send_batch`].
    async fn send_templated_batch(&self, emails: &[TemplatedEmail]) -> Result<BatchOutcome, GatewayError>;
}
