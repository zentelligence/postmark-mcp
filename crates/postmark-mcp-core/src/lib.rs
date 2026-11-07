//! Domain types and ports for the Postmark MCP server.
//!
//! This crate is the centre of the hexagon: every other workspace crate depends
//! on it, and it depends on no transport, HTTP, or MCP implementation.
//!
//! Result types decode directly from Postmark's `PascalCase` JSON and serialise
//! as `snake_case` for tool results, so one type serves both ends. They model
//! only the fields worth returning: anything else, such as `ApiTokens` or
//! webhook passwords, is discarded during decoding.

mod bounces;
mod diagnosis;
mod gateway;
mod managing;
mod messages;
mod sending;
mod server;
mod stats;
mod suppressions;
mod templates;
mod values;
mod webhooks;
mod wire;

pub use bounces::{Bounce, BounceDump, BounceList, BounceOutcome, BounceSearch, BounceType, ReadsBounces};
pub use diagnosis::{
    DeliveryDiagnosis, DiagnosedMessage, DiagnosisQuery, IncompleteLookup, Verdict, diagnose_delivery,
};
pub use gateway::{GatewayError, PostmarkGateway};
pub use managing::{
    ChangesBounces, ChangesSuppressions, ChangesTemplates, ChangesWebhooks, Deletion, MAX_SUPPRESSION_ADDRESSES,
    NewTemplate, NewTemplateRequest, NewWebhook, SuppressionChange, SuppressionOutcome, SuppressionResponse,
    SuppressionResults, TemplateChange, TemplateChangeRequest, TemplateUpdate, WebhookTrigger, WebhookUrl,
    WebhookUrlAllowlist,
};
pub use messages::{
    MessageDetails, MessageEvent, MessageList, MessageSearch, MessageStatus, MessageSummary, ReadsMessages,
};
pub use sending::{
    BatchItem, BatchOutcome, EmailRequest, Envelope, EnvelopeRequest, MAX_BATCH_MESSAGES, OutboundEmail, SendDefaults,
    SendReceipt, SendsEmail, TemplateChoice, TemplatedEmail,
};
pub use server::{ReadsServer, ServerSummary};
pub use stats::{DeliveryRates, DeliveryStat, DeliveryStats, ReadsStats, StatsQuery};
pub use suppressions::{
    ReadsSuppressions, Suppression, SuppressionList, SuppressionOrigin, SuppressionQuery, SuppressionReason,
};
pub use templates::{
    ContentValidation, ReadsTemplates, Template, TemplateList, TemplateQuery, TemplateSummary, TemplateType,
    TemplateTypeFilter, TemplateValidation, ValidationError, ValidationResult,
};
pub use values::{
    AgentLabel, BounceId, DateRange, FilterText, MailAddress, MessageStreamId, Page, PageRequest, PostmarkDate,
    RecipientAddress, RecipientAllowlist, ResourceId, SenderAddress, ServerToken, ValueError, WebhookId,
};
pub use webhooks::{ReadsWebhooks, Webhook, WebhookList};
pub use wire::JsonObject;
