//! Delivery diagnosis: why an email may not have reached a recipient.
//!
//! People act on this verdict, so the rule throughout is that a diagnosis never
//! overstates what it knows:
//!
//! - A failed lookup is reported as incomplete, never as an empty result.
//! - Postmark's address filters may match partially, so everything returned is
//!   checked against the exact address.
//! - Only evidence attributable to this recipient and this message counts.
//! - A conclusion that rests on the absence of something needs every lookup that
//!   could have found it.

use std::collections::HashSet;

use futures_util::future::join3;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use serde_with::skip_serializing_none;

use crate::wire::vocabulary;
use crate::{
    Bounce, BounceList, BounceOutcome, BounceSearch, DateRange, FilterText, GatewayError, MessageDetails, MessageEvent,
    MessageSearch, MessageStreamId, MessageSummary, Page, PostmarkGateway, RecipientAddress, ResourceId, Suppression,
    SuppressionList, SuppressionQuery,
};

/// Messages requested when searching. More than the one used, because a partial
/// match on another address may come first.
const MESSAGE_SEARCH_COUNT: u32 = 10;

/// Bounces requested. More than are returned, for the same reason.
const BOUNCE_SEARCH_COUNT: u32 = 50;

/// Bounces returned in a diagnosis.
const MAX_RECENT_BOUNCES: usize = 10;

const STATUS_QUEUED: &str = "Queued";
const EVENT_BOUNCED: &str = "Bounced";
const EVENT_TRANSIENT: &str = "Transient";

/// Events proving the receiving server accepted the message. An open or click
/// counts, since neither can happen to mail that did not arrive.
const EVENTS_PROVING_DELIVERY: [&str; 3] = ["Delivered", "Opened", "LinkClicked"];

vocabulary! {
    /// The conclusion of a diagnosis.
    Verdict {
        Inconclusive => "inconclusive",
        SuppressedSpamComplaint => "suppressed_spam_complaint",
        SuppressedHardBounce => "suppressed_hard_bounce",
        SuppressedManual => "suppressed_manual",
        SuppressedOther => "suppressed_other",
        Sandboxed => "sandboxed",
        Bounced => "bounced",
        Delivered => "delivered",
        Delayed => "delayed",
        Queued => "queued",
        SentNoDeliveryEvent => "sent_no_delivery_event",
        BounceHistoryOnly => "bounce_history_only",
        NoRecentMessage => "no_recent_message",
    }
}

/// What to diagnose.
#[derive(Clone, Debug)]
pub struct DiagnosisQuery {
    /// The address to investigate.
    pub recipient: RecipientAddress,
    /// A specific message, instead of the most recent one found.
    pub message_id: Option<ResourceId>,
    /// Stream to investigate. A given message's own stream takes precedence.
    pub message_stream: MessageStreamId,
    /// Restricts the search for a message, and the bounce history. A suppression
    /// applies whenever it was created, so dates do not restrict that lookup.
    pub dates: DateRange,
}

/// The message a diagnosis is about.
#[skip_serializing_none]
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct DiagnosedMessage {
    /// Postmark's message identifier.
    pub message_id: String,
    /// Stream the message was sent on.
    pub message_stream: Option<String>,
    /// Subject line.
    pub subject: Option<String>,
    /// `Sent`, `Processed`, or `Queued`.
    pub status: Option<String>,
    /// When the message was sent.
    pub received_at: Option<String>,
    /// Whether the message was sandboxed, and so never left Postmark.
    pub sandboxed: Option<bool>,
    /// Events for this recipient. Omitted when they could not be read.
    pub events: Option<Vec<MessageEvent>>,
}

/// Something the diagnosis could not establish.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct IncompleteLookup {
    /// `message`, `message_details`, `suppressions`, `bounces`, or `message_bounces`.
    pub lookup: String,
    /// Stable, machine-readable code.
    pub code: String,
    /// What could not be established.
    pub message: String,
    /// What to do about it.
    pub remediation: String,
}

impl From<(&str, &GatewayError)> for IncompleteLookup {
    fn from((lookup, error): (&str, &GatewayError)) -> Self {
        Self {
            lookup: lookup.to_owned(),
            code: error.code().to_owned(),
            message: error.to_string(),
            remediation: error.remediation().to_owned(),
        }
    }
}

/// A limit on what the message lookup established, despite it succeeding.
#[derive(Clone, Copy, Debug)]
enum MessageDoubt {
    RecipientMismatch,
    RecipientUnconfirmed,
    ResultsTruncated,
    BouncesTruncated,
    StreamUnconfirmed,
}

impl From<MessageDoubt> for IncompleteLookup {
    fn from(doubt: MessageDoubt) -> Self {
        let (lookup, code, message, remediation) = match doubt {
            MessageDoubt::RecipientMismatch => (
                "message",
                "diagnosis/recipient-mismatch",
                "the given message was not sent to this recipient",
                "Check message_id, or leave it out to find this recipient's most recent message.",
            ),
            MessageDoubt::RecipientUnconfirmed => (
                "message",
                "diagnosis/recipient-unconfirmed",
                "Postmark did not say who a message was sent to, so it cannot be tied to this recipient",
                "Use search_outbound_messages to review recent messages.",
            ),
            MessageDoubt::ResultsTruncated => (
                "message",
                "diagnosis/results-truncated",
                "more messages matched than were read, and those read were for other addresses",
                "Narrow the search with from_date and to_date, or give a message_id.",
            ),
            MessageDoubt::BouncesTruncated => (
                "bounces",
                "diagnosis/results-truncated",
                "more bounces matched than were read, and those read were for other addresses",
                "Narrow the search with from_date and to_date, or use search_bounces.",
            ),
            MessageDoubt::StreamUnconfirmed => (
                "message",
                "diagnosis/stream-unconfirmed",
                "Postmark gave no usable stream for this message, so the requested stream was checked instead",
                "Run this again with the message's message_stream, if you know it.",
            ),
        };
        Self {
            lookup: lookup.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
            remediation: remediation.to_owned(),
        }
    }
}

/// Why an email may not have reached a recipient.
#[skip_serializing_none]
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct DeliveryDiagnosis {
    /// The address investigated.
    pub recipient: String,
    /// The stream investigated.
    pub message_stream: String,
    /// The conclusion. `inconclusive` means it could not be established.
    pub verdict: Verdict,
    /// What to do next.
    pub recommendation: String,
    /// The suppression on this address on this stream. Absent means none was
    /// found, unless `incomplete` lists `suppressions`.
    pub suppression: Option<Suppression>,
    /// The message investigated. Named to stay distinct from the `message` field
    /// of a tool failure, since a tool's output schema describes both.
    pub investigated_message: Option<DiagnosedMessage>,
    /// Bounce records of this exact address, the investigated message's first,
    /// including ones that are not delivery failures, such as out of office replies.
    pub recent_bounces: Vec<Bounce>,
    /// The bounce that deactivated this address, for a hard bounce suppression
    /// that can be reactivated.
    pub reactivatable_bounce_id: Option<i64>,
    /// What could not be established. If this is not empty, treat absent data as
    /// unknown.
    pub incomplete: Vec<IncompleteLookup>,
}

/// Outcome of finding the message to diagnose.
enum MessageLookup {
    /// No message confirmed as this recipient's was found.
    NotFound { doubt: Option<MessageDoubt> },
    /// The message and its events.
    Detailed(MessageDetails),
    /// The message was found, but its details could not be read.
    SummaryOnly(MessageSummary, GatewayError),
}

impl MessageLookup {
    /// The message's identifier, if it is this recipient's as far as is known.
    fn message_id(&self, recipient: &RecipientAddress) -> Option<&str> {
        match self {
            Self::Detailed(details) if addressing(recipient, &details.recipients) != Addressing::ToOthers => {
                Some(&details.message_id)
            }
            Self::SummaryOnly(summary, _) => Some(&summary.message_id),
            Self::NotFound { .. } | Self::Detailed(_) => None,
        }
    }

    /// The stream to investigate, which is the message's own where it has one.
    fn message_stream(&self, recipient: &RecipientAddress) -> Option<MessageStreamId> {
        match self {
            Self::Detailed(details) if addressing(recipient, &details.recipients) != Addressing::ToOthers => {
                details.message_stream.clone().and_then(|stream| stream.try_into().ok())
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Addressing {
    ToRecipient,
    ToOthers,
    Unknown,
}

fn addressing(recipient: &RecipientAddress, recipients: &[String]) -> Addressing {
    if recipients.is_empty() {
        Addressing::Unknown
    } else if recipients.iter().any(|address| recipient.matches(address)) {
        Addressing::ToRecipient
    } else {
        Addressing::ToOthers
    }
}

async fn find_message<G>(gateway: &G, query: &DiagnosisQuery) -> Result<MessageLookup, GatewayError>
where
    G: PostmarkGateway + ?Sized,
{
    if let Some(id) = &query.message_id {
        return gateway.message_details(id).await.map(MessageLookup::Detailed);
    }

    let search = MessageSearch {
        page: Page {
            count: MESSAGE_SEARCH_COUNT,
            offset: 0,
        },
        message_stream: query.message_stream.clone(),
        recipient: Some(query.recipient.filter()),
        from_email: None,
        tag: None,
        subject: None,
        status: None,
        dates: query.dates.clone(),
    };
    let list = gateway.search_outbound_messages(&search).await?;
    let unconfirmed = list
        .messages
        .iter()
        .any(|message| addressing(&query.recipient, &message.recipients) == Addressing::Unknown);
    let truncated = list.total_count > i64::from(MESSAGE_SEARCH_COUNT);
    let found = list
        .messages
        .into_iter()
        .find(|message| addressing(&query.recipient, &message.recipients) == Addressing::ToRecipient);
    let Some(summary) = found else {
        let doubt = if unconfirmed {
            Some(MessageDoubt::RecipientUnconfirmed)
        } else {
            truncated.then_some(MessageDoubt::ResultsTruncated)
        };
        return Ok(MessageLookup::NotFound { doubt });
    };

    let details = match ResourceId::try_from(("message_id", summary.message_id.clone())) {
        Ok(id) => gateway.message_details(&id).await,
        Err(error) => Err(GatewayError::InvalidResponse {
            detail: error.to_string(),
        }),
    };
    Ok(match details {
        Ok(details) => MessageLookup::Detailed(details),
        Err(error) => MessageLookup::SummaryOnly(summary, error),
    })
}

/// Finds the message first, because its own stream decides where to look next,
/// then runs the remaining lookups concurrently and concludes.
///
/// # Errors
///
/// Returns the first [`GatewayError`] only when every lookup fails, as with a
/// rejected token, since there is then nothing to diagnose. A partial failure
/// is reported in [`DeliveryDiagnosis::incomplete`] instead.
pub async fn diagnose_delivery<G>(gateway: &G, query: &DiagnosisQuery) -> Result<DeliveryDiagnosis, GatewayError>
where
    G: PostmarkGateway + ?Sized,
{
    let recipient = &query.recipient;
    let message = find_message(gateway, query).await;
    let lookup = message.as_ref().ok();
    let own_stream = lookup.and_then(|found| found.message_stream(recipient));
    // A search already ran on the requested stream, so only a message named by ID
    // can be on another one.
    let stream_unconfirmed = query.message_id.is_some()
        && own_stream.is_none()
        && lookup.is_some_and(|found| found.message_id(recipient).is_some());
    let stream = own_stream.unwrap_or_else(|| query.message_stream.clone());
    let message_id: Option<FilterText> = lookup
        .and_then(|found| found.message_id(recipient))
        .and_then(|id| FilterText::try_from(("message_id", id.to_owned())).ok());

    let suppression_query = SuppressionQuery {
        message_stream: stream.clone(),
        reason: None,
        origin: None,
        email_address: Some(recipient.filter()),
        dates: DateRange::default(),
    };
    let bounce_search = BounceSearch {
        page: Page {
            count: BOUNCE_SEARCH_COUNT,
            offset: 0,
        },
        message_stream: stream.clone(),
        bounce_type: None,
        inactive: None,
        email_filter: Some(recipient.filter()),
        tag: None,
        message_id: None,
        dates: query.dates.clone(),
    };
    // Looked up directly, so a long bounce history cannot hide this message's.
    let message_bounce_search = message_id.map(|id| BounceSearch {
        message_id: Some(id),
        dates: DateRange::default(),
        ..bounce_search.clone()
    });

    let (suppressions, bounces, message_bounces) = join3(
        gateway.list_suppressions(&suppression_query),
        gateway.search_bounces(&bounce_search),
        async {
            match &message_bounce_search {
                Some(search) => Some(gateway.search_bounces(search).await),
                None => None,
            }
        },
    )
    .await;

    if let (Err(error), Err(_), Err(_)) = (&message, &suppressions, &bounces) {
        return Err(error.clone());
    }
    Ok(Findings {
        recipient,
        stream: &stream,
        stream_unconfirmed,
        message,
        suppressions,
        bounces,
        message_bounces,
    }
    .into())
}

/// Everything the lookups returned.
struct Findings<'a> {
    recipient: &'a RecipientAddress,
    stream: &'a MessageStreamId,
    stream_unconfirmed: bool,
    message: Result<MessageLookup, GatewayError>,
    suppressions: Result<SuppressionList, GatewayError>,
    bounces: Result<BounceList, GatewayError>,
    message_bounces: Option<Result<BounceList, GatewayError>>,
}

impl From<MessageDetails> for DiagnosedMessage {
    fn from(details: MessageDetails) -> Self {
        Self {
            message_id: details.message_id,
            message_stream: details.message_stream,
            subject: details.subject,
            status: details.status,
            received_at: details.received_at,
            sandboxed: details.sandboxed,
            events: Some(details.events),
        }
    }
}

impl From<MessageSummary> for DiagnosedMessage {
    fn from(summary: MessageSummary) -> Self {
        Self {
            message_id: summary.message_id,
            message_stream: summary.message_stream,
            subject: summary.subject,
            status: summary.status,
            received_at: summary.received_at,
            sandboxed: None,
            events: None,
        }
    }
}

/// What the message lookup established.
struct MessageReading {
    message: Option<DiagnosedMessage>,
    /// Whether the lookup can support a conclusion about this recipient.
    reliable: bool,
}

/// Keeps only events attributable to this recipient. Another recipient's
/// delivery says nothing about this one, and an event naming nobody can only be
/// attributed when this recipient is the sole recipient.
fn keep_recipients_events(details: &mut MessageDetails, recipient: &RecipientAddress) {
    let sole_recipient = details.recipients.len() == 1;
    details.events.retain(|event| match event.recipient.as_deref() {
        Some(address) => recipient.matches(address),
        None => sole_recipient,
    });
}

fn read_message(
    lookup: Result<MessageLookup, GatewayError>,
    recipient: &RecipientAddress,
    gaps: &mut Vec<IncompleteLookup>,
) -> MessageReading {
    let (message, reliable) = match lookup {
        Ok(MessageLookup::NotFound { doubt }) => {
            gaps.extend(doubt.map(IncompleteLookup::from));
            (None, doubt.is_none())
        }
        Ok(MessageLookup::Detailed(mut details)) => match addressing(recipient, &details.recipients) {
            Addressing::ToOthers => {
                gaps.push(MessageDoubt::RecipientMismatch.into());
                (None, false)
            }
            Addressing::Unknown => {
                gaps.push(MessageDoubt::RecipientUnconfirmed.into());
                keep_recipients_events(&mut details, recipient);
                (Some(details.into()), false)
            }
            Addressing::ToRecipient => {
                keep_recipients_events(&mut details, recipient);
                (Some(details.into()), true)
            }
        },
        Ok(MessageLookup::SummaryOnly(summary, error)) => {
            gaps.push(("message_details", &error).into());
            (Some(summary.into()), true)
        }
        Err(error) => {
            gaps.push(("message", &error).into());
            (None, false)
        }
    };
    MessageReading { message, reliable }
}

/// Returns the suppression on this exact address, and whether the address is
/// known to have none.
fn read_suppressions(
    lookup: Result<SuppressionList, GatewayError>,
    recipient: &RecipientAddress,
    gaps: &mut Vec<IncompleteLookup>,
) -> (Option<Suppression>, bool) {
    match lookup {
        Ok(list) => {
            let found = list
                .suppressions
                .into_iter()
                .find(|entry| recipient.matches(&entry.email_address));
            let unsuppressed = found.is_none();
            (found, unsuppressed)
        }
        Err(error) => {
            gaps.push(("suppressions", &error).into());
            (None, false)
        }
    }
}

/// Returns the bounces of this exact address, and whether they can be relied on
/// as complete. A page cut off with no exact match is not evidence of absence.
fn read_bounces(
    lookup: Result<BounceList, GatewayError>,
    name: &str,
    recipient: &RecipientAddress,
    gaps: &mut Vec<IncompleteLookup>,
) -> (Vec<Bounce>, bool) {
    match lookup {
        Ok(list) => {
            let truncated = list.total_count > i64::from(BOUNCE_SEARCH_COUNT);
            let exact: Vec<_> = list
                .bounces
                .into_iter()
                .filter(|bounce| {
                    bounce
                        .email
                        .as_deref()
                        .is_some_and(|address| recipient.matches(address))
                })
                .collect();
            let cut_off = truncated && exact.is_empty();
            if cut_off {
                gaps.push(MessageDoubt::BouncesTruncated.into());
            }
            (exact, !cut_off)
        }
        Err(error) => {
            gaps.push((name, &error).into());
            (Vec::new(), false)
        }
    }
}

/// Whether a `Bounced` event is a real failure. Postmark logs out of office
/// replies as bounces too, so a linked record can overrule the event, but only
/// by saying it is something else: a record that does not say stands no chance
/// against an event that does.
fn is_failed_bounce_event(event: &MessageEvent, bounces: &[Bounce]) -> bool {
    let bounce_id = event
        .details
        .as_ref()
        .and_then(|details| details.get("BounceID"))
        .and_then(|id| match id {
            Value::String(text) => text.parse::<i64>().ok(),
            other => other.as_i64(),
        });
    bounce_id
        .and_then(|id| bounces.iter().find(|bounce| bounce.id == id))
        .is_none_or(|bounce| matches!(bounce.outcome(), BounceOutcome::Failed | BounceOutcome::Unclassified))
}

/// What the lookups other than the message one established.
struct Evidence<'a> {
    suppression: Option<&'a Suppression>,
    /// Every bounce record of this address that was read.
    bounces: &'a [Bounce],
    /// Records for the message investigated.
    message_bounces: &'a [Bounce],
    /// Whether the bounce lookups can be relied on as complete.
    bounces_read: bool,
    /// Whether nothing was found by lookups that all succeeded.
    all_clear: bool,
}

/// Concludes from a message whose events are known, when no suppression applies.
fn message_verdict(message: &DiagnosedMessage, events: &[MessageEvent], evidence: &Evidence<'_>) -> Verdict {
    let happened = |kind: &str| events.iter().any(|event| event.r#type == kind);
    let logged = |outcome: BounceOutcome| {
        evidence
            .message_bounces
            .iter()
            .any(|bounce| bounce.outcome() == outcome)
    };
    let bounced = events
        .iter()
        .filter(|event| event.r#type == EVENT_BOUNCED)
        .any(|event| is_failed_bounce_event(event, evidence.bounces));

    if message.sandboxed == Some(true) {
        Verdict::Sandboxed
    } else if bounced || logged(BounceOutcome::Failed) {
        Verdict::Bounced
    } else if EVENTS_PROVING_DELIVERY.iter().any(|kind| happened(kind)) {
        Verdict::Delivered
    } else if !evidence.bounces_read {
        // Anything less than delivery could yet be refuted by a bounce not read.
        Verdict::Inconclusive
    } else if happened(EVENT_TRANSIENT) || logged(BounceOutcome::Delayed) {
        Verdict::Delayed
    } else if message.status.as_deref() == Some(STATUS_QUEUED) {
        Verdict::Queued
    } else if evidence.all_clear {
        Verdict::SentNoDeliveryEvent
    } else {
        Verdict::Inconclusive
    }
}

fn suppression_verdict(suppression: &Suppression) -> Verdict {
    match suppression.suppression_reason.as_deref() {
        Some("SpamComplaint") => Verdict::SuppressedSpamComplaint,
        Some("HardBounce") => Verdict::SuppressedHardBounce,
        Some("ManualSuppression") => Verdict::SuppressedManual,
        _ => Verdict::SuppressedOther,
    }
}

fn conclude(reading: &MessageReading, evidence: &Evidence<'_>) -> Verdict {
    if let Some(found) = evidence.suppression {
        return suppression_verdict(found);
    }
    if !reading.reliable {
        return Verdict::Inconclusive;
    }
    match &reading.message {
        Some(found) => found
            .events
            .as_ref()
            .map_or(Verdict::Inconclusive, |events| message_verdict(found, events, evidence)),
        None if !evidence.bounces.is_empty() => Verdict::BounceHistoryOnly,
        None if evidence.all_clear => Verdict::NoRecentMessage,
        None => Verdict::Inconclusive,
    }
}

fn recommendation(verdict: Verdict, suppression: Option<&Suppression>, bounce_id: Option<i64>) -> String {
    let text = match verdict {
        Verdict::SuppressedSpamComplaint => {
            "The recipient marked earlier mail as spam. Postmark does not allow this suppression to be removed, so do not retry."
        }
        Verdict::SuppressedHardBounce => {
            if let Some(id) = bounce_id {
                return format!(
                    "The address hard bounced. Confirm it is valid, then reactivate bounce {id} in Postmark before resending."
                );
            }
            "The address hard bounced. Confirm it is valid before trying to reinstate it."
        }
        Verdict::SuppressedManual if suppression.is_some_and(|found| found.origin.as_deref() == Some("Recipient")) => {
            "The recipient opted out of this stream. Respect that: only the recipient should reverse it."
        }
        Verdict::SuppressedManual => {
            "The address was suppressed by your team or by Postmark. Confirm why before removing the suppression in Postmark."
        }
        Verdict::SuppressedOther => {
            "The address is suppressed on this stream, for a reason this server does not recognise. Review it in Postmark."
        }
        Verdict::Sandboxed => {
            "The message was sandboxed, so it was never sent to the recipient. Send it from a live server to test delivery."
        }
        Verdict::Bounced => {
            "The message bounced. See recent_bounces, and use get_bounce_dump if more detail is needed."
        }
        Verdict::Delivered => {
            "The receiving server accepted the message. Ask the recipient to check their spam folder, or to allow the sender."
        }
        Verdict::Delayed => {
            "Delivery hit a temporary failure. Check again later, and review the events if nothing changes."
        }
        Verdict::Queued => "The message is still queued. Check again in a few minutes.",
        Verdict::SentNoDeliveryEvent => {
            "Postmark sent the message, but no delivery has been recorded for this recipient yet. Check again later, and review the events."
        }
        Verdict::BounceHistoryOnly => {
            "No message was found, but this address has bounce records. See recent_bounces, and check the address."
        }
        Verdict::NoRecentMessage => {
            "No message to this address was found on this stream. Check the address and stream, or widen the dates. Postmark keeps message history for 45 days by default."
        }
        Verdict::Inconclusive => {
            "This could not be established. See incomplete for what is missing and what to do about it."
        }
    };
    text.to_owned()
}

impl From<Findings<'_>> for DeliveryDiagnosis {
    fn from(findings: Findings<'_>) -> Self {
        let recipient = findings.recipient;
        let mut gaps = Vec::new();
        let reading = read_message(findings.message, recipient, &mut gaps);
        let (suppression, unsuppressed) = read_suppressions(findings.suppressions, recipient, &mut gaps);
        let (history, history_read) = read_bounces(findings.bounces, "bounces", recipient, &mut gaps);
        let (direct, direct_read) = match findings.message_bounces {
            Some(lookup) => read_bounces(lookup, "message_bounces", recipient, &mut gaps),
            None => (Vec::new(), true),
        };
        if findings.stream_unconfirmed {
            gaps.push(MessageDoubt::StreamUnconfirmed.into());
        }

        // Every record read, the investigated message's first, each once.
        let investigated = reading.message.as_ref().map(|message| message.message_id.as_str());
        let is_investigated =
            |bounce: &Bounce| bounce.message_id.is_some() && bounce.message_id.as_deref() == investigated;
        let (message_bounces, others): (Vec<_>, Vec<_>) = direct.into_iter().chain(history).partition(is_investigated);
        let mut seen = HashSet::new();
        let message_bounces: Vec<_> = message_bounces
            .into_iter()
            .filter(|bounce| seen.insert(bounce.id))
            .collect();
        let others = others.into_iter().filter(|bounce| seen.insert(bounce.id));
        let all_bounces: Vec<_> = message_bounces.iter().cloned().chain(others).collect();

        let bounces_read = history_read && direct_read;
        let verdict = conclude(
            &reading,
            &Evidence {
                suppression: suppression.as_ref(),
                bounces: &all_bounces,
                message_bounces: &message_bounces,
                bounces_read,
                all_clear: unsuppressed && bounces_read && !findings.stream_unconfirmed,
            },
        );

        // Offered only where reactivating is the advice. `CanActivate` alone does
        // not identify the bounce that deactivated the address.
        let reactivatable_bounce_id = (verdict == Verdict::SuppressedHardBounce)
            .then(|| {
                all_bounces
                    .iter()
                    .find(|bounce| bounce.can_activate == Some(true) && bounce.inactive == Some(true))
                    .map(|bounce| bounce.id)
            })
            .flatten();

        Self {
            recipient: recipient.to_string(),
            message_stream: findings.stream.to_string(),
            verdict,
            recommendation: recommendation(verdict, suppression.as_ref(), reactivatable_bounce_id),
            suppression,
            investigated_message: reading.message,
            recent_bounces: all_bounces.into_iter().take(MAX_RECENT_BOUNCES).collect(),
            reactivatable_bounce_id,
            incomplete: gaps,
        }
    }
}

#[cfg(test)]
#[path = "diagnosis_test.rs"]
mod tests;
