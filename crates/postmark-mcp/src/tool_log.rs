//! The record of tool calls. It holds what was asked and how it went, not what was
//! said: bodies, subjects, and template data are reduced to their size, and
//! addresses are masked unless the operator asks otherwise.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rmcp::model::JsonObject;
use serde::Serialize;
use serde_json::{Value, json};

/// Strings longer than this are reduced to their length, whatever they hold.
const MAX_LOGGED_STRING: usize = 300;

/// Array items kept in the log.
const MAX_LOGGED_ITEMS: usize = 20;

/// Names longer than this are cut, since a caller chooses them.
const MAX_LOGGED_NAME: usize = 64;

/// Fields whose values describe a request rather than carry its content. Only
/// these are logged as given. Everything else is reduced to its size, so content
/// under a name this list does not expect, such as `textBody`, stays out.
const PLAIN_FIELDS: [&str; 20] = [
    "bounce_id",
    "count",
    "from_date",
    "inactive",
    "layout_template",
    "message_id",
    "message_stream",
    "offset",
    "origin",
    "stat",
    "status",
    "suppression_reason",
    "tag",
    "template_alias",
    "template_id",
    "template_id_or_alias",
    "template_type",
    "to_date",
    "type",
    "unexpected",
];

/// Fields holding a list of messages or recipients, whose items are sanitised
/// with these same rules.
const NESTED_FIELDS: [&str; 2] = ["messages", "recipients"];

/// Fields holding addresses, or fragments of them.
const ADDRESS_FIELDS: [&str; 9] = [
    "bcc",
    "cc",
    "email_address",
    "email_filter",
    "from",
    "from_email",
    "recipient",
    "reply_to",
    "to",
];

/// Writes the record of tool calls to stderr, and to `LOG_FILE` if configured.
#[derive(Clone, Debug, Default)]
pub struct ToolLog {
    file: Option<Arc<Mutex<File>>>,
    full_addresses: bool,
}

impl ToolLog {
    /// Opens the log.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` cannot be opened for appending.
    pub fn open(path: Option<&Path>, full_addresses: bool) -> io::Result<Self> {
        let file = path
            .map(|path| {
                let mut options = OpenOptions::new();
                options.create(true).append(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                options.open(path)
            })
            .transpose()?;
        Ok(Self {
            file: file.map(|file| Arc::new(Mutex::new(file))),
            full_addresses,
        })
    }

    /// Records a finished tool call.
    pub(crate) async fn record(&self, call: ToolCall<'_>) {
        let entry = Entry {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
                .unwrap_or_default(),
            tool: bounded_name(call.tool),
            status: if call.succeeded { "ok" } else { "error" },
            code: call.code,
            duration_ms: u64::try_from(call.duration.as_millis()).unwrap_or(u64::MAX),
            args: sanitised_arguments(call.arguments, self.full_addresses),
        };
        let line = match serde_json::to_string(&entry) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(%error, "could not serialise a tool log entry");
                return;
            }
        };
        tracing::info!(target: "tool_call", "{line}");

        let Some(file) = self.file.clone() else {
            return;
        };
        let written = tokio::task::spawn_blocking(move || {
            let mut file = file.lock().map_err(|_| io::Error::other("log file lock is poisoned"))?;
            writeln!(file, "{line}")
        })
        .await;
        match written {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "could not write to LOG_FILE"),
            Err(error) => tracing::warn!(%error, "could not write to LOG_FILE"),
        }
    }
}

/// A finished tool call.
pub(crate) struct ToolCall<'a> {
    pub(crate) tool: &'a str,
    pub(crate) arguments: Option<&'a JsonObject>,
    pub(crate) succeeded: bool,
    pub(crate) code: Option<String>,
    pub(crate) duration: Duration,
}

#[derive(Serialize)]
struct Entry<'a> {
    timestamp_ms: u64,
    tool: String,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    duration_ms: u64,
    args: Value,
}

fn sized(value: &str) -> Value {
    json!(format!("[{} chars]", value.chars().count()))
}

/// Cuts a caller-chosen name, such as a tool or field name, to a safe length.
pub(crate) fn bounded_name(name: &str) -> String {
    name.chars().take(MAX_LOGGED_NAME).collect()
}

/// Masks an address, dropping any display name, which can identify the person.
/// `user@example.com` becomes `u**r@example.com`.
fn masked_address(value: &str) -> Option<String> {
    let bare = value
        .rsplit_once('<')
        .and_then(|(_, rest)| rest.trim_end().strip_suffix('>'))
        .unwrap_or(value)
        .trim();
    let (mailbox, domain) = bare.rsplit_once('@')?;
    if domain.is_empty() || bare.contains(char::is_whitespace) {
        return None;
    }
    let characters: Vec<_> = mailbox.chars().collect();
    Some(match characters.as_slice() {
        [] => return None,
        [only] => format!("{only}*@{domain}"),
        [first, .., last] => {
            let stars = "*".repeat(characters.len().saturating_sub(2).max(1));
            format!("{first}{stars}{last}@{domain}")
        }
    })
}

/// Logs an address, masked unless the operator asked for addresses in full. A
/// value that is not an address, such as a partial one, is reduced to its size.
fn logged_address(value: &str, full_addresses: bool) -> Value {
    if value.chars().count() > MAX_LOGGED_STRING {
        return sized(value);
    }
    match masked_address(value) {
        Some(_) if full_addresses => json!(value),
        Some(masked) => json!(masked),
        None => sized(value),
    }
}

/// Reduces a value to a description of its size.
fn size_of(value: &Value) -> Value {
    match value {
        Value::String(text) => sized(text),
        Value::Object(object) => json!(format!("[{} keys]", object.len())),
        Value::Array(items) => json!(format!("[{} items]", items.len())),
        Value::Number(_) => json!("[number]"),
        Value::Bool(_) | Value::Null => value.clone(),
    }
}

fn each_item(items: &[Value], keep: impl Fn(&Value) -> Value) -> Value {
    let mut kept: Vec<_> = items.iter().take(MAX_LOGGED_ITEMS).map(keep).collect();
    if items.len() > MAX_LOGGED_ITEMS {
        kept.push(json!(format!("+{} more", items.len() - MAX_LOGGED_ITEMS)));
    }
    Value::Array(kept)
}

fn sanitised(field: &str, value: &Value, full_addresses: bool) -> Value {
    if ADDRESS_FIELDS.contains(&field) {
        return match value {
            Value::String(text) => logged_address(text, full_addresses),
            Value::Array(items) => each_item(items, |item| match item {
                Value::String(text) => logged_address(text, full_addresses),
                other => size_of(other),
            }),
            other => size_of(other),
        };
    }
    if NESTED_FIELDS.contains(&field)
        && let Value::Array(items) = value
    {
        return each_item(items, |item| match item {
            Value::Object(object) => sanitised_object(object, full_addresses),
            other => size_of(other),
        });
    }
    if !PLAIN_FIELDS.contains(&field) {
        return size_of(value);
    }
    match value {
        // An address can turn up anywhere, such as in a tag.
        Value::String(text) if text.chars().count() > MAX_LOGGED_STRING => sized(text),
        Value::String(text) => masked_address(text)
            .filter(|_| !full_addresses)
            .map_or_else(|| json!(text), |masked| json!(masked)),
        Value::Number(_) | Value::Bool(_) | Value::Null => value.clone(),
        other => size_of(other),
    }
}

fn sanitised_object(object: &JsonObject, full_addresses: bool) -> Value {
    Value::Object(
        object
            .iter()
            .map(|(name, value)| (bounded_name(name), sanitised(name, value, full_addresses)))
            .collect(),
    )
}

fn sanitised_arguments(arguments: Option<&JsonObject>, full_addresses: bool) -> Value {
    arguments.map_or(Value::Null, |object| sanitised_object(object, full_addresses))
}

#[cfg(test)]
#[path = "tool_log_test.rs"]
mod tests;
