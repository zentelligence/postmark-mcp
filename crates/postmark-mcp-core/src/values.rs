//! Validated value types for security-significant or protocol-significant
//! strings. Each is only constructible through `TryFrom`, so holding one proves
//! the value was checked at the boundary it entered through.

use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

/// Upper bound on [`AgentLabel`], which is sent on every API request.
const MAX_AGENT_LABEL_LEN: usize = 128;

/// Upper bound on one email address, as Postmark documents it.
const MAX_ADDRESS_LEN: usize = 254;

/// Upper bound on a free-text search filter.
const MAX_FILTER_LEN: usize = 512;

/// Upper bound on a resource identifier or alias.
const MAX_RESOURCE_ID_LEN: usize = 255;

/// Postmark's maximum page size.
const MAX_PAGE_COUNT: u32 = 500;

/// Postmark's limit on `count + offset`. Results beyond it need narrower dates.
const MAX_PAGE_DEPTH: u32 = 10_000;

/// A value failed validation.
///
/// No variant carries the rejected input: a malformed token is still likely to
/// be mostly secret, and these messages reach logs and operators.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ValueError {
    /// The value was empty or only whitespace.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The value cannot be carried in an HTTP header.
    #[error("{field} must contain only visible ASCII characters")]
    NotHeaderSafe {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The sender is not an address, or carries control characters that could
    /// inject additional mail headers.
    #[error("sender must be an email address, optionally as `Name <address>`, with no control characters")]
    MalformedSender,
    /// The message stream identifier has characters Postmark does not use.
    #[error("message stream ID must contain only ASCII letters, digits, hyphens, and underscores")]
    MalformedMessageStream,
    /// The value exceeds its permitted length.
    #[error("{field} must not exceed {limit} characters")]
    TooLong {
        /// Name of the value that failed validation.
        field: &'static str,
        /// Maximum permitted length.
        limit: usize,
    },
    /// The value contains control characters.
    #[error("{field} must not contain control characters")]
    ControlCharacters {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The value cannot identify a resource.
    #[error("{field} is not a valid identifier")]
    InvalidIdentifier {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The value is not a real calendar date.
    #[error("{field} must be a real date in YYYY-MM-DD format")]
    InvalidDate {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// The end of a date range precedes its start.
    #[error("to_date must not be earlier than from_date")]
    InvertedDateRange,
    /// The requested page size is out of range.
    #[error("count must be between 1 and {MAX_PAGE_COUNT}")]
    PageCountOutOfRange,
    /// The requested page reaches beyond what Postmark can return.
    #[error("count plus offset must not exceed {MAX_PAGE_DEPTH}. Narrow the filters to reach further results")]
    PageTooDeep,
    /// The value is not a single bare email address.
    #[error("recipient must be a single email address, such as name@example.com")]
    MalformedRecipient,
    /// Dates cannot apply to one specific message.
    #[error("give either message_id or dates, not both")]
    MessageIdWithDates,
    /// The value is not a single mail address.
    #[error("{field} must be one email address, optionally as `Name <address>`. Give several as an array")]
    MalformedAddress {
        /// Name of the value that failed validation.
        field: &'static str,
    },
    /// A message has too many recipients.
    #[error("to, cc, and bcc must not exceed {limit} recipients in total")]
    TooManyRecipients {
        /// Maximum permitted recipients.
        limit: usize,
    },
    /// A message has nothing to say.
    #[error("at least one of text_body or html_body must be provided")]
    NoMessageBody,
    /// No template was identified.
    #[error("one of template_id or template_alias must be provided")]
    TemplateNotChosen,
    /// Two templates were identified.
    #[error("provide only one of template_id or template_alias")]
    TemplateChosenTwice,
    /// A batch is empty or too large.
    #[error("a batch must have between 1 and {limit} messages")]
    BatchSizeOutOfRange {
        /// Maximum permitted messages.
        limit: usize,
    },
    /// A recipient allowlist entry is unusable.
    #[error("each entry must be an address such as name@example.com, or a domain such as @example.com")]
    MalformedAllowlistEntry,
    /// No content was supplied to validate.
    #[error("at least one of subject, html_body, or text_body must be provided")]
    NothingToValidate,
}

fn require_header_safe(field: &'static str, value: &str) -> Result<(), ValueError> {
    if value.trim().is_empty() {
        return Err(ValueError::Empty { field });
    }
    if value.bytes().all(|byte| byte.is_ascii_graphic() || byte == b' ') {
        Ok(())
    } else {
        Err(ValueError::NotHeaderSafe { field })
    }
}

/// A Postmark server API token.
///
/// The wrapped secret is redacted from `Debug` output and zeroed on drop. Read
/// it with [`ServerToken::expose`] only at the point it is placed in a request.
#[derive(Clone, Debug)]
pub struct ServerToken(SecretString);

impl ServerToken {
    /// Returns the secret token for placing in an HTTP header.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl TryFrom<String> for ServerToken {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const FIELD: &str = "server token";
        require_header_safe(FIELD, &value)?;
        if value.contains(' ') {
            return Err(ValueError::NotHeaderSafe { field: FIELD });
        }
        Ok(Self(SecretString::from(value)))
    }
}

/// A sender address, as `address@example.com` or `Name <address@example.com>`.
///
/// This proves the value is shaped like a sender and is safe to place in a
/// message. Postmark remains the authority on whether the sender is verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SenderAddress(String);

impl TryFrom<String> for SenderAddress {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(ValueError::Empty { field: "sender" });
        }
        let has_mailbox_and_domain = value
            .rsplit_once('@')
            .is_some_and(|(mailbox, domain)| !mailbox.trim().is_empty() && !domain.trim().is_empty());
        if has_mailbox_and_domain && !value.chars().any(char::is_control) {
            Ok(Self(value))
        } else {
            Err(ValueError::MalformedSender)
        }
    }
}

impl AsRef<str> for SenderAddress {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SenderAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A Postmark message stream identifier, such as `outbound`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageStreamId(String);

impl TryFrom<String> for MessageStreamId {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(ValueError::Empty {
                field: "message stream ID",
            });
        }
        if value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            Ok(Self(value))
        } else {
            Err(ValueError::MalformedMessageStream)
        }
    }
}

impl AsRef<str> for MessageStreamId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MessageStreamId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// An operator-chosen label identifying this instance in Postmark request logs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLabel(String);

impl TryFrom<String> for AgentLabel {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const FIELD: &str = "agent label";
        require_header_safe(FIELD, &value)?;
        if value.len() > MAX_AGENT_LABEL_LEN {
            return Err(ValueError::TooLong {
                field: FIELD,
                limit: MAX_AGENT_LABEL_LEN,
            });
        }
        Ok(Self(value))
    }
}

impl AsRef<str> for AgentLabel {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A free-text search filter, such as a tag or recipient.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilterText(String);

impl TryFrom<(&'static str, String)> for FilterText {
    type Error = ValueError;

    fn try_from((field, value): (&'static str, String)) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(ValueError::Empty { field });
        }
        if value.chars().any(char::is_control) {
            return Err(ValueError::ControlCharacters { field });
        }
        if value.chars().count() > MAX_FILTER_LEN {
            return Err(ValueError::TooLong {
                field,
                limit: MAX_FILTER_LEN,
            });
        }
        Ok(Self(value))
    }
}

impl fmt::Display for FilterText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One bare recipient address, such as `name@example.com`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientAddress(FilterText);

impl RecipientAddress {
    /// Whether another address is this one, which may be `Name <address>`. Case
    /// is ignored, as mail providers treat addresses that way, and Postmark's
    /// filters do not promise an exact match, so results must be checked with this.
    #[must_use]
    pub fn matches(&self, address: &str) -> bool {
        let bare = address
            .rsplit_once('<')
            .and_then(|(_, rest)| rest.strip_suffix('>').or_else(|| rest.trim_end().strip_suffix('>')))
            .unwrap_or(address);
        self.0.0.eq_ignore_ascii_case(bare.trim())
    }

    /// This address as a search filter.
    #[must_use]
    pub fn filter(&self) -> FilterText {
        self.0.clone()
    }
}

impl TryFrom<String> for RecipientAddress {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let valid = value
            .split_once('@')
            .is_some_and(|(mailbox, domain)| !mailbox.is_empty() && !domain.is_empty() && !domain.contains('@'))
            && !value
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ','));
        if !valid {
            return Err(ValueError::MalformedRecipient);
        }
        Ok(Self(FilterText::try_from(("recipient", value))?))
    }
}

impl fmt::Display for RecipientAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// One mail address, as `address@example.com` or `Name <address@example.com>`.
///
/// The bare address is held apart, so a policy check sees where mail really goes.
/// Commas and quotes are refused: either can make one string mean several
/// recipients, or hide the real one from a check like this.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailAddress {
    full: String,
    bare: String,
}

impl MailAddress {
    /// The address mail is delivered to, without any display name.
    #[must_use]
    pub fn bare(&self) -> &str {
        &self.bare
    }
}

/// Characters with meaning in an address header. In a display name, each could
/// lead a lenient parser to find a second address, or a different one, so they are
/// refused rather than trusted to be read as a name.
fn is_address_special(c: char) -> bool {
    matches!(c, '@' | ':' | '(' | ')' | '[' | ']' | '\\' | '<' | '>')
}

/// Whether a domain is shaped like a hostname, such as `example.com`.
fn is_domain(value: &str) -> bool {
    value.contains('.')
        && !value.starts_with(['.', '-'])
        && !value.ends_with(['.', '-'])
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
}

fn is_bare_address(value: &str) -> bool {
    let shaped = value
        .split_once('@')
        .is_some_and(|(mailbox, domain)| !mailbox.is_empty() && !domain.is_empty() && !domain.contains('@'));
    shaped && value.len() <= MAX_ADDRESS_LEN && !value.chars().any(|c| c.is_whitespace() || matches!(c, '<' | '>'))
}

impl TryFrom<(&'static str, String)> for MailAddress {
    type Error = ValueError;

    fn try_from((field, value): (&'static str, String)) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        let unsafe_character = |c: char| c.is_control() || matches!(c, ',' | ';' | '"');
        if trimmed.chars().any(unsafe_character) {
            return Err(ValueError::MalformedAddress { field });
        }
        let bare = match trimmed.strip_suffix('>').map(|rest| rest.split_once('<')) {
            None => trimmed,
            Some(Some((name, address))) if !name.chars().any(is_address_special) => address,
            Some(_) => return Err(ValueError::MalformedAddress { field }),
        };
        if !is_bare_address(bare) {
            return Err(ValueError::MalformedAddress { field });
        }
        Ok(Self {
            bare: bare.to_owned(),
            full: trimmed.to_owned(),
        })
    }
}

impl fmt::Display for MailAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.full)
    }
}

/// Where mail may be sent, when an operator restricts it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientAllowlist {
    /// Lower-cased bare addresses.
    addresses: Vec<String>,
    /// Lower-cased domains, matched whole, so subdomains need their own entry.
    domains: Vec<String>,
}

impl RecipientAllowlist {
    /// Whether mail may be sent to this address.
    #[must_use]
    pub fn permits(&self, address: &MailAddress) -> bool {
        let bare = address.bare().to_ascii_lowercase();
        let domain = bare.rsplit_once('@').map_or("", |(_, domain)| domain);
        self.addresses.contains(&bare) || self.domains.iter().any(|entry| entry == domain)
    }
}

impl TryFrom<&str> for RecipientAllowlist {
    type Error = ValueError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut addresses = Vec::new();
        let mut domains = Vec::new();
        for entry in value.split(',').map(|entry| entry.trim().to_ascii_lowercase()) {
            match entry.strip_prefix('@') {
                Some(domain) if is_domain(domain) => domains.push(domain.to_owned()),
                None if is_bare_address(&entry) => addresses.push(entry),
                _ => return Err(ValueError::MalformedAllowlistEntry),
            }
        }
        Ok(Self { addresses, domains })
    }
}

/// An identifier or alias placed in a request path, such as a message ID.
///
/// `.` and `..` are refused because URL normalisation would remove them, turning
/// the request into one for a different resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceId(String);

impl TryFrom<(&'static str, String)> for ResourceId {
    type Error = ValueError;

    fn try_from((field, value): (&'static str, String)) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(ValueError::Empty { field });
        }
        if value == "." || value == ".." || value.chars().any(char::is_control) {
            return Err(ValueError::InvalidIdentifier { field });
        }
        if value.chars().count() > MAX_RESOURCE_ID_LEN {
            return Err(ValueError::TooLong {
                field,
                limit: MAX_RESOURCE_ID_LEN,
            });
        }
        Ok(Self(value))
    }
}

impl AsRef<str> for ResourceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A Postmark bounce identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BounceId(u64);

impl BounceId {
    /// The numeric identifier.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for BounceId {
    type Error = ValueError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(ValueError::InvalidIdentifier { field: "bounce_id" })
        } else {
            Ok(Self(value))
        }
    }
}

impl fmt::Display for BounceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// A calendar date in `YYYY-MM-DD` form, as Postmark's date filters take.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PostmarkDate(String);

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl TryFrom<(&'static str, String)> for PostmarkDate {
    type Error = ValueError;

    fn try_from((field, value): (&'static str, String)) -> Result<Self, Self::Error> {
        let digits_only = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
        let parts: Vec<_> = value.split('-').collect();
        let [year, month, day] = parts.as_slice() else {
            return Err(ValueError::InvalidDate { field });
        };
        if year.len() != 4 || month.len() != 2 || day.len() != 2 || ![year, month, day].iter().all(|p| digits_only(p)) {
            return Err(ValueError::InvalidDate { field });
        }
        let (Ok(year), Ok(month), Ok(day)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) else {
            return Err(ValueError::InvalidDate { field });
        };
        if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
            return Err(ValueError::InvalidDate { field });
        }
        Ok(Self(value))
    }
}

impl fmt::Display for PostmarkDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// An optional inclusive date range. `YYYY-MM-DD` sorts chronologically, so the
/// dates compare directly.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DateRange {
    /// Start of the range, inclusive.
    pub from: Option<PostmarkDate>,
    /// End of the range, inclusive.
    pub to: Option<PostmarkDate>,
}

impl TryFrom<(Option<String>, Option<String>)> for DateRange {
    type Error = ValueError;

    fn try_from((from, to): (Option<String>, Option<String>)) -> Result<Self, Self::Error> {
        let from = from
            .map(|date| PostmarkDate::try_from(("from_date", date)))
            .transpose()?;
        let to = to.map(|date| PostmarkDate::try_from(("to_date", date))).transpose()?;
        if let (Some(start), Some(end)) = (&from, &to)
            && start > end
        {
            return Err(ValueError::InvertedDateRange);
        }
        Ok(Self { from, to })
    }
}

/// A requested page, before defaults are applied.
#[derive(Clone, Copy, Debug)]
pub struct PageRequest {
    /// Requested page size.
    pub count: Option<u32>,
    /// Requested number of results to skip.
    pub offset: Option<u32>,
    /// Page size used when none is requested.
    pub default_count: u32,
}

/// A page of results within Postmark's limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Page {
    /// Number of results to return.
    pub count: u32,
    /// Number of results to skip.
    pub offset: u32,
}

impl TryFrom<PageRequest> for Page {
    type Error = ValueError;

    fn try_from(request: PageRequest) -> Result<Self, Self::Error> {
        let count = request.count.unwrap_or(request.default_count);
        let offset = request.offset.unwrap_or(0);
        if !(1..=MAX_PAGE_COUNT).contains(&count) {
            return Err(ValueError::PageCountOutOfRange);
        }
        if count.saturating_add(offset) > MAX_PAGE_DEPTH {
            return Err(ValueError::PageTooDeep);
        }
        Ok(Self { count, offset })
    }
}

#[cfg(test)]
#[path = "values_test.rs"]
mod tests;
