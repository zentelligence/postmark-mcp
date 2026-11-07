//! Changes to account state: templates, suppressions, bounces, and webhooks.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use url::Url;

use crate::wire::{blank_as_none, null_as_default, vocabulary};
use crate::{
    Bounce, BounceId, GatewayError, MessageStreamId, RecipientAddress, ResourceId, Template, TemplateSummary,
    TemplateType, ValueError, Webhook, WebhookId,
};

/// Postmark's limit on addresses in one suppression change.
pub const MAX_SUPPRESSION_ADDRESSES: usize = 50;

fn named_text(field: &'static str, value: String) -> Result<String, ValueError> {
    if value.trim().is_empty() {
        return Err(ValueError::Empty { field });
    }
    if value.chars().any(char::is_control) {
        return Err(ValueError::ControlCharacters { field });
    }
    Ok(value)
}

/// Content, which may span lines, but must not be empty.
fn content(field: &'static str, value: String) -> Result<String, ValueError> {
    if value.trim().is_empty() {
        Err(ValueError::Empty { field })
    } else {
        Ok(value)
    }
}

/// Postmark's alias rule: letters, digits, `.`, `-`, and `_`, starting with a letter.
fn alias(value: String) -> Result<String, ValueError> {
    let mut characters = value.chars();
    let starts_with_letter = characters.next().is_some_and(|first| first.is_ascii_alphabetic());
    if starts_with_letter && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
        Ok(value)
    } else {
        Err(ValueError::InvalidIdentifier { field: "alias" })
    }
}

/// A template to create, before validation.
#[derive(Clone, Debug, Default)]
pub struct NewTemplateRequest {
    /// Name of the template.
    pub name: String,
    /// Subject. Required for a standard template, refused for a layout.
    pub subject: Option<String>,
    /// HTML body.
    pub html_body: Option<String>,
    /// Plain text body.
    pub text_body: Option<String>,
    /// String identifier.
    pub alias: Option<String>,
    /// `Standard` or `Layout`. Defaults to `Standard`.
    pub template_type: Option<TemplateType>,
    /// Alias of a layout for a standard template to use.
    pub layout_template: Option<String>,
}

/// A validated template to create, in the shape Postmark takes it.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NewTemplate {
    name: String,
    subject: Option<String>,
    html_body: Option<String>,
    text_body: Option<String>,
    alias: Option<String>,
    template_type: Option<TemplateType>,
    layout_template: Option<String>,
}

impl TryFrom<NewTemplateRequest> for NewTemplate {
    type Error = ValueError;

    fn try_from(request: NewTemplateRequest) -> Result<Self, Self::Error> {
        // An empty value is refused, not read as absent, so a mistake is not
        // silently turned into a different template.
        let html_body = request.html_body.map(|body| content("html_body", body)).transpose()?;
        let text_body = request.text_body.map(|body| content("text_body", body)).transpose()?;
        if html_body.is_none() && text_body.is_none() {
            return Err(ValueError::NoMessageBody);
        }
        let layout = request.template_type == Some(TemplateType::Layout);
        let subject = request
            .subject
            .map(|subject| named_text("subject", subject))
            .transpose()?;
        match (layout, &subject) {
            (true, Some(_)) => return Err(ValueError::LayoutWithSubject),
            (false, None) => return Err(ValueError::Empty { field: "subject" }),
            _ => {}
        }
        if layout && request.layout_template.is_some() {
            return Err(ValueError::LayoutInLayout);
        }
        Ok(Self {
            name: named_text("name", request.name)?,
            subject,
            html_body,
            text_body,
            alias: request.alias.map(alias).transpose()?,
            template_type: request.template_type,
            layout_template: request
                .layout_template
                .map(|layout| named_text("layout_template", layout))
                .transpose()?,
        })
    }
}

/// Changes to a template, before validation.
#[derive(Clone, Debug, Default)]
pub struct TemplateChangeRequest {
    /// New name.
    pub name: Option<String>,
    /// New subject.
    pub subject: Option<String>,
    /// New HTML body.
    pub html_body: Option<String>,
    /// New plain text body.
    pub text_body: Option<String>,
    /// New alias.
    pub alias: Option<String>,
    /// Alias of a layout to use instead.
    pub layout_template: Option<String>,
    /// Stops the template using any layout.
    pub detach_layout: Option<bool>,
}

/// Validated changes to a template, in the shape Postmark takes them. Fields left
/// out are not sent, so they are not changed.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TemplateChange {
    name: Option<String>,
    subject: Option<String>,
    html_body: Option<String>,
    text_body: Option<String>,
    alias: Option<String>,
    /// An empty string detaches the layout, as Postmark documents.
    layout_template: Option<String>,
}

impl TryFrom<TemplateChangeRequest> for TemplateChange {
    type Error = ValueError;

    fn try_from(request: TemplateChangeRequest) -> Result<Self, Self::Error> {
        let detach = request.detach_layout == Some(true);
        if detach && request.layout_template.is_some() {
            return Err(ValueError::LayoutChangeConflict);
        }
        let layout_template = if detach {
            Some(String::new())
        } else {
            request
                .layout_template
                .map(|layout| named_text("layout_template", layout))
                .transpose()?
        };
        let change = Self {
            name: request.name.map(|name| named_text("name", name)).transpose()?,
            subject: request
                .subject
                .map(|subject| named_text("subject", subject))
                .transpose()?,
            html_body: request.html_body.map(|body| content("html_body", body)).transpose()?,
            text_body: request.text_body.map(|body| content("text_body", body)).transpose()?,
            alias: request.alias.map(alias).transpose()?,
            layout_template,
        };
        let changes_something = change.name.is_some()
            || change.subject.is_some()
            || change.html_body.is_some()
            || change.text_body.is_some()
            || change.alias.is_some()
            || change.layout_template.is_some();
        if changes_something {
            Ok(change)
        } else {
            Err(ValueError::NothingToChange)
        }
    }
}

/// A template's full new state, in the shape Postmark takes it: the current values
/// with only the requested changes applied. Sending every field avoids relying on
/// whether Postmark keeps fields an edit leaves out, which its documentation does
/// not settle.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TemplateUpdate {
    name: String,
    subject: Option<String>,
    html_body: Option<String>,
    text_body: Option<String>,
    alias: Option<String>,
    layout_template: Option<String>,
}

impl From<(Template, &TemplateChange)> for TemplateUpdate {
    fn from((current, change): (Template, &TemplateChange)) -> Self {
        let pick = |changed: &Option<String>, current: Option<String>| changed.clone().or(current);
        Self {
            name: change.name.clone().unwrap_or(current.name),
            subject: pick(&change.subject, current.subject),
            html_body: pick(&change.html_body, current.html_body),
            text_body: pick(&change.text_body, current.text_body),
            alias: pick(&change.alias, current.alias),
            layout_template: pick(&change.layout_template, current.layout_template),
        }
    }
}

/// Something that was deleted.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct Deletion {
    /// The identifier, as given.
    pub deleted: String,
}

/// Addresses to suppress, or to stop suppressing, on one stream.
#[derive(Clone, Debug)]
pub struct SuppressionChange {
    /// The stream.
    pub message_stream: MessageStreamId,
    /// The addresses, from 1 to 50.
    pub addresses: Vec<RecipientAddress>,
}

impl TryFrom<(MessageStreamId, Vec<String>)> for SuppressionChange {
    type Error = ValueError;

    fn try_from((message_stream, addresses): (MessageStreamId, Vec<String>)) -> Result<Self, Self::Error> {
        if !(1..=MAX_SUPPRESSION_ADDRESSES).contains(&addresses.len()) {
            return Err(ValueError::AddressCountOutOfRange {
                limit: MAX_SUPPRESSION_ADDRESSES,
            });
        }
        Ok(Self {
            message_stream,
            addresses: addresses
                .into_iter()
                .map(RecipientAddress::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Postmark's answer about one address in a suppression change.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct SuppressionOutcome {
    /// The address.
    pub email_address: String,
    /// `Suppressed` or `Deleted` on success, `Failed` otherwise.
    pub status: String,
    /// Why it failed, if it did.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub message: Option<String>,
}

/// Postmark's answer about a suppression change. Each address succeeds or fails
/// on its own.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
pub struct SuppressionResults {
    /// The stream changed.
    pub message_stream: String,
    /// Addresses changed.
    pub succeeded: usize,
    /// Addresses Postmark refused to change.
    pub failed: usize,
    /// Each answer, as Postmark gave it.
    pub results: Vec<SuppressionOutcome>,
}

/// Response of either suppression change.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SuppressionResponse {
    #[serde(default, deserialize_with = "null_as_default")]
    suppressions: Vec<SuppressionOutcome>,
}

impl From<(&MessageStreamId, SuppressionResponse)> for SuppressionResults {
    fn from((stream, response): (&MessageStreamId, SuppressionResponse)) -> Self {
        let succeeded = response
            .suppressions
            .iter()
            .filter(|outcome| matches!(outcome.status.as_str(), "Suppressed" | "Deleted"))
            .count();
        Self {
            message_stream: stream.to_string(),
            failed: response.suppressions.len() - succeeded,
            succeeded,
            results: response.suppressions,
        }
    }
}

vocabulary! {
    /// An event a webhook can be told about.
    WebhookTrigger {
        Open => "Open",
        Click => "Click",
        Delivery => "Delivery",
        Bounce => "Bounce",
        SpamComplaint => "SpamComplaint",
        SubscriptionChange => "SubscriptionChange",
    }
}

/// A webhook destination: HTTPS, with no credentials in the URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookUrl(Url);

impl WebhookUrl {
    /// The parsed URL.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.0
    }
}

impl TryFrom<String> for WebhookUrl {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let url = Url::parse(&value).map_err(|_| ValueError::InsecureWebhookUrl)?;
        let has_credentials = !url.username().is_empty() || url.password().is_some();
        if url.scheme() != "https" || has_credentials || url.host_str().is_none() {
            return Err(ValueError::InsecureWebhookUrl);
        }
        // An encoded slash or backslash survives parsing, and the receiving server
        // could decode it into a path the allowlist never saw. Encoded dots need no
        // check: the parser resolves them as dot segments.
        let path = url.path().to_ascii_lowercase();
        if ["%2f", "%5c"].iter().any(|encoded| path.contains(encoded)) {
            return Err(ValueError::InsecureWebhookUrl);
        }
        Ok(Self(url))
    }
}

/// Where webhooks may be registered: HTTPS origins, each with an optional path
/// prefix that matches whole path segments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookUrlAllowlist(Vec<Url>);

impl WebhookUrlAllowlist {
    /// Whether a webhook may be registered at this URL. `/hooks` covers
    /// `/hooks/postmark`, but not `/hooks-other`.
    #[must_use]
    pub fn permits(&self, url: &WebhookUrl) -> bool {
        let url = url.url();
        self.0.iter().any(|entry| {
            let prefix = entry.path().trim_end_matches('/');
            entry.origin() == url.origin()
                && (prefix.is_empty() || url.path() == prefix || url.path().starts_with(&format!("{prefix}/")))
        })
    }
}

impl TryFrom<&str> for WebhookUrlAllowlist {
    type Error = ValueError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value
            .split(',')
            .map(|entry| {
                WebhookUrl::try_from(entry.trim().to_owned())
                    .map(|url| url.0)
                    .map_err(|_| ValueError::MalformedWebhookAllowlistEntry)
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

/// A webhook to create, in the shape Postmark takes it.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NewWebhook {
    url: String,
    message_stream: Option<String>,
    triggers: TriggerSettings,
    #[serde(skip)]
    destination: WebhookUrl,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct Enabled {
    #[serde(rename = "Enabled")]
    enabled: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct TriggerSettings {
    open: Enabled,
    click: Enabled,
    delivery: Enabled,
    bounce: Enabled,
    spam_complaint: Enabled,
    subscription_change: Enabled,
}

impl NewWebhook {
    /// Where the webhook would send events.
    #[must_use]
    pub fn destination(&self) -> &WebhookUrl {
        &self.destination
    }
}

impl TryFrom<(WebhookUrl, Option<MessageStreamId>, Vec<WebhookTrigger>)> for NewWebhook {
    type Error = ValueError;

    fn try_from(
        (destination, message_stream, triggers): (WebhookUrl, Option<MessageStreamId>, Vec<WebhookTrigger>),
    ) -> Result<Self, Self::Error> {
        if triggers.is_empty() {
            return Err(ValueError::NoWebhookTriggers);
        }
        let on = |trigger| Enabled {
            enabled: triggers.contains(&trigger),
        };
        Ok(Self {
            url: destination.url().to_string(),
            message_stream: message_stream.map(|stream| stream.to_string()),
            triggers: TriggerSettings {
                open: on(WebhookTrigger::Open),
                click: on(WebhookTrigger::Click),
                delivery: on(WebhookTrigger::Delivery),
                bounce: on(WebhookTrigger::Bounce),
                spam_complaint: on(WebhookTrigger::SpamComplaint),
                subscription_change: on(WebhookTrigger::SubscriptionChange),
            },
            destination,
        })
    }
}

/// Creates, changes, and deletes templates.
#[async_trait]
pub trait ChangesTemplates: Send + Sync {
    /// Creates a template.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails. Use
    /// [`GatewayError::outcome_is_known`] before suggesting a retry.
    async fn create_template(&self, template: &NewTemplate) -> Result<TemplateSummary, GatewayError>;

    /// Changes a template.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn edit_template(
        &self,
        id_or_alias: &ResourceId,
        change: &TemplateChange,
    ) -> Result<TemplateSummary, GatewayError>;

    /// Deletes a template.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails or Postmark refuses it.
    async fn delete_template(&self, id_or_alias: &ResourceId) -> Result<Deletion, GatewayError>;
}

/// Reactivates bounced addresses.
#[async_trait]
pub trait ChangesBounces: Send + Sync {
    /// Reactivates the address a bounce deactivated.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails or Postmark refuses it.
    async fn activate_bounce(&self, bounce_id: BounceId) -> Result<Bounce, GatewayError>;
}

/// Adds and lifts suppressions.
#[async_trait]
pub trait ChangesSuppressions: Send + Sync {
    /// Suppresses addresses on a stream.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn create_suppressions(&self, change: &SuppressionChange) -> Result<SuppressionResults, GatewayError>;

    /// Lifts suppressions on a stream.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn delete_suppressions(&self, change: &SuppressionChange) -> Result<SuppressionResults, GatewayError>;
}

/// Creates and deletes webhooks.
#[async_trait]
pub trait ChangesWebhooks: Send + Sync {
    /// Creates a webhook.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails. Use
    /// [`GatewayError::outcome_is_known`] before suggesting a retry.
    async fn create_webhook(&self, webhook: &NewWebhook) -> Result<Webhook, GatewayError>;

    /// Deletes a webhook.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails or Postmark refuses it.
    async fn delete_webhook(&self, webhook_id: WebhookId) -> Result<Deletion, GatewayError>;
}

#[cfg(test)]
#[path = "managing_test.rs"]
mod tests;
