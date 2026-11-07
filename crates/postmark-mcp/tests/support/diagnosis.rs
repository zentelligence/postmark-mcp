//! Fixtures for the `diagnose_delivery` contract tests.

use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{DEFAULT_STREAM, TestError, success};

pub const RECIPIENT: &str = "alice@example.com";

/// What the stand-in for Postmark answers on each route. `None` makes the route
/// fail, as Postmark does for a server error.
pub struct Scenario {
    pub messages: Option<Value>,
    pub details: Option<Value>,
    pub suppressions: Option<Value>,
    pub bounces: Option<Value>,
    pub message_bounces: MessageBounces,
}

/// How the bounce lookup by message ID is answered.
#[derive(Default)]
pub enum MessageBounces {
    /// The same as the bounce history.
    #[default]
    SameAsHistory,
    /// These records.
    Answer(Value),
    /// A server error.
    Fails,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            messages: Some(json!([])),
            details: None,
            suppressions: Some(json!([])),
            bounces: Some(json!([])),
            message_bounces: MessageBounces::SameAsHistory,
        }
    }
}

pub fn response(body: Option<Value>) -> ResponseTemplate {
    match body {
        Some(body) => ResponseTemplate::new(200).set_body_json(body),
        None => ResponseTemplate::new(500),
    }
}

pub async fn start(scenario: Scenario) -> MockServer {
    let postmark = MockServer::start().await;
    let answer = match scenario.message_bounces {
        MessageBounces::SameAsHistory => None,
        MessageBounces::Answer(items) => Some(Some(items)),
        MessageBounces::Fails => Some(None),
    };
    if let Some(answer) = answer {
        Mock::given(path("/bounces"))
            .and(query_param("messageID", "m1"))
            .respond_with(response(answer.map(|items| json!({"Bounces": items, "TotalCount": 0}))))
            .with_priority(1)
            .mount(&postmark)
            .await;
    }
    let wrap = |key: &str, value: Value| json!({key: value, "TotalCount": 0});
    for (route, body) in [
        (
            "/messages/outbound".to_owned(),
            scenario.messages.map(|items| wrap("Messages", items)),
        ),
        ("/messages/outbound/m1/details".to_owned(), scenario.details),
        (
            format!("/message-streams/{DEFAULT_STREAM}/suppressions/dump"),
            scenario.suppressions.map(|items| wrap("Suppressions", items)),
        ),
        (
            "/bounces".to_owned(),
            scenario.bounces.map(|items| wrap("Bounces", items)),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response(body))
            .mount(&postmark)
            .await;
    }
    postmark
}

/// Answers the suppression and bounce routes with nothing found.
pub async fn answer_empty_for(postmark: &MockServer, stream: &str) {
    Mock::given(path(format!("/message-streams/{stream}/suppressions/dump")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Suppressions": []})))
        .mount(postmark)
        .await;
    Mock::given(path("/bounces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 0, "Bounces": []})))
        .mount(postmark)
        .await;
}

pub fn sent_message() -> Value {
    json!([{"MessageID": "m1", "Recipients": [RECIPIENT], "Subject": "Hello", "Status": "Sent", "ReceivedAt": "2026-01-02T03:04:05-05:00"}])
}

pub fn details(status: &str, events: &Value) -> Value {
    json!({
        "MessageID": "m1",
        "Recipients": [RECIPIENT],
        "Subject": "Hello",
        "Status": status,
        "ReceivedAt": "2026-01-02T03:04:05-05:00",
        "MessageEvents": events,
    })
}

pub fn event(kind: &str, recipient: &str) -> Value {
    json!({"Type": kind, "Recipient": recipient, "ReceivedAt": "2026-01-02T03:04:06-05:00", "Details": {}})
}

pub fn suppression(address: &str, reason: &str, origin: &str) -> Value {
    json!({"EmailAddress": address, "SuppressionReason": reason, "Origin": origin, "CreatedAt": "2026-01-01T00:00:00-05:00"})
}

pub fn bounce(id: i64, address: &str, message_id: &str, can_activate: bool) -> Value {
    typed_bounce(id, "HardBounce", address, message_id, can_activate)
}

pub fn typed_bounce(id: i64, kind: &str, address: &str, message_id: &str, deactivated: bool) -> Value {
    json!({
        "ID": id,
        "Type": kind,
        "Email": address,
        "MessageID": message_id,
        "CanActivate": deactivated,
        "Inactive": deactivated,
        "Description": "Unknown user",
    })
}

pub async fn diagnose(scenario: Scenario) -> Result<Value, TestError> {
    success(
        &start(scenario).await,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT}),
    )
    .await
}

pub fn verdict(result: &Value) -> Option<&str> {
    result.get("verdict").and_then(Value::as_str)
}

pub fn incomplete_lookups(result: &Value) -> Vec<String> {
    result
        .get("incomplete")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("lookup").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub fn query_of(requests: &[wiremock::Request], route: &str) -> Result<Vec<(String, String)>, TestError> {
    let request = requests
        .iter()
        .find(|request| request.url.path() == route)
        .ok_or_else(|| format!("no request to {route}"))?;
    let mut pairs: Vec<_> = request.url.query_pairs().into_owned().collect();
    pairs.sort();
    Ok(pairs)
}

pub fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut pairs: Vec<_> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    pairs.sort();
    pairs
}
