//! Contract tests for the send tools. Sending cannot be undone, so these care
//! as much about what is refused, and what a failure says about whether mail
//! went out, as about what is sent.

mod support;

use serde_json::{Value, json};
use support::{DEFAULT_STREAM, TestError, TestResult, assert_refused, failure, success};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn accepted() -> Value {
    json!({
        "To": "to@example.com",
        "SubmittedAt": "2026-01-02T03:04:05-05:00",
        "MessageID": "0ac29aee",
        "ErrorCode": 0,
        "Message": "OK",
    })
}

/// Starts a stand-in that accepts only this exact JSON body at this route.
async fn expecting(route: &str, body: Value, answer: Value) -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route))
        .and(body_json(body))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .expect(1)
        .mount(&postmark)
        .await;
    postmark
}

async fn answering(route: &str, response: ResponseTemplate) -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(response)
        .mount(&postmark)
        .await;
    postmark
}

fn minimal() -> Value {
    json!({"to": "to@example.com", "subject": "Hello", "text_body": "Hi"})
}

#[tokio::test]
async fn send_email_applies_the_defaults_and_leaves_tracking_to_the_server() -> TestResult {
    // An exact body, so `TrackOpens` or `TrackLinks` appearing would fail this.
    let postmark = expecting(
        "/email",
        json!({
            "From": "sender@example.com",
            "To": "to@example.com",
            "Subject": "Hello",
            "TextBody": "Hi",
            "MessageStream": DEFAULT_STREAM,
        }),
        accepted(),
    )
    .await;

    let result = success(&postmark, "send_email", minimal()).await?;

    assert_eq!(
        result,
        json!({"message_id": "0ac29aee", "to": "to@example.com", "submitted_at": "2026-01-02T03:04:05-05:00"})
    );
    Ok(())
}

#[tokio::test]
async fn send_email_sends_every_field_supplied() -> TestResult {
    let postmark = expecting(
        "/email",
        json!({
            "From": "Support <support@example.com>",
            "To": "a@example.com, Bee <b@example.com>",
            "Cc": "c@example.com",
            "Bcc": "d@example.com, e@example.com",
            "Subject": "Hello",
            "HtmlBody": "<p>Hi</p>",
            "TextBody": "Hi",
            "ReplyTo": "reply@example.com",
            "Tag": "welcome",
            "MessageStream": DEFAULT_STREAM,
        }),
        accepted(),
    )
    .await;

    success(
        &postmark,
        "send_email",
        json!({
            "from": "Support <support@example.com>",
            "to": ["a@example.com", "Bee <b@example.com>"],
            "cc": "c@example.com",
            "bcc": ["d@example.com", "e@example.com"],
            "subject": "Hello",
            "html_body": "<p>Hi</p>",
            "text_body": "Hi",
            "reply_to": "reply@example.com",
            "tag": "welcome",
        }),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn send_email_accepts_an_html_body_alone() -> TestResult {
    let postmark = answering("/email", ResponseTemplate::new(200).set_body_json(accepted())).await;

    success(
        &postmark,
        "send_email",
        json!({"to": "to@example.com", "subject": "Hello", "html_body": "<p>Hi</p>"}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn send_email_refuses_what_it_cannot_send_safely() -> TestResult {
    let fifty_one: Vec<_> = (0..51).map(|index| format!("r{index}@example.com")).collect();
    let thirty: Vec<_> = (0..30).map(|index| format!("c{index}@example.com")).collect();
    let twenty_one: Vec<_> = (0..21).map(|index| format!("b{index}@example.com")).collect();
    for arguments in [
        json!({"subject": "Hello", "text_body": "Hi"}),
        json!({"to": [], "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "not-an-address", "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "a@example.com, b@example.com", "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "\"a@example.com\" <b@example.com>", "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "a@example.com\nBcc: b@example.com", "subject": "Hello", "text_body": "Hi"}),
        json!({"to": fifty_one, "subject": "Hello", "text_body": "Hi"}),
        json!({"to": thirty, "cc": twenty_one, "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "to@example.com", "text_body": "Hi"}),
        json!({"to": "to@example.com", "subject": "", "text_body": "Hi"}),
        json!({"to": "to@example.com", "subject": "x\r\nBcc: b@example.com", "text_body": "Hi"}),
        json!({"to": "to@example.com", "subject": "x".repeat(2001), "text_body": "Hi"}),
        json!({"to": "to@example.com", "subject": "Hello"}),
        json!({"to": "to@example.com", "subject": "Hello", "text_body": ""}),
        json!({"to": "to@example.com", "subject": "Hello", "text_body": "Hi", "from": "nobody"}),
        json!({"to": "to@example.com", "subject": "Hello", "text_body": "Hi", "reply_to": "nobody"}),
        json!({"to": "to@example.com", "subject": "Hello", "text_body": "Hi", "tag": "x".repeat(1001)}),
        json!({"to": "to@example.com", "subject": "Hello", "text_body": "Hi", "cc": "nobody"}),
        json!({"to": "to@example.com", "subject": "Hello", "text_body": "Hi", "unexpected": true}),
    ] {
        assert_refused("send_email", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn send_email_allows_exactly_fifty_recipients_in_total() -> TestResult {
    let thirty: Vec<_> = (0..30).map(|index| format!("c{index}@example.com")).collect();
    let twenty: Vec<_> = (0..20).map(|index| format!("b{index}@example.com")).collect();
    let postmark = answering("/email", ResponseTemplate::new(200).set_body_json(accepted())).await;

    success(
        &postmark,
        "send_email",
        json!({"to": thirty, "cc": twenty, "subject": "Hello", "text_body": "Hi"}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn refusals_postmark_gives_with_success_status_are_failures() -> TestResult {
    let mut refused = accepted();
    refused["ErrorCode"] = json!(406);
    refused["Message"] = json!("Inactive recipient");
    let postmark = answering("/email", ResponseTemplate::new(200).set_body_json(refused)).await;

    let result = failure(&postmark, "send_email", minimal()).await?;

    assert_eq!(result.get("code"), Some(&json!("postmark/rejected")));
    assert!(result.to_string().contains("Inactive recipient"));
    Ok(())
}

async fn failure_code(response: ResponseTemplate) -> Result<Value, TestError> {
    let postmark = answering("/email", response).await;
    let result = failure(&postmark, "send_email", minimal()).await?;
    assert_eq!(
        postmark.received_requests().await.unwrap_or_default().len(),
        1,
        "a send must never be retried automatically"
    );
    Ok(result)
}

#[tokio::test]
async fn an_ambiguous_failure_says_the_mail_may_have_gone_out() -> TestResult {
    for response in [
        ResponseTemplate::new(500),
        ResponseTemplate::new(503),
        ResponseTemplate::new(200).set_body_string("not json"),
        ResponseTemplate::new(302).insert_header("Location", "https://elsewhere.example.com/"),
    ] {
        let result = failure_code(response).await?;

        assert_eq!(result.get("code"), Some(&json!("send/outcome-unknown")));
        let remediation = result.get("remediation").and_then(Value::as_str).unwrap_or_default();
        assert!(remediation.contains("search_outbound_messages"), "{remediation}");
        assert!(!remediation.to_lowercase().contains("retry"), "{remediation}");
    }
    Ok(())
}

#[tokio::test]
async fn a_definite_refusal_keeps_postmarks_own_code() -> TestResult {
    let rejected = ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 300, "Message": "Invalid"}));
    let unauthorised = ResponseTemplate::new(401).set_body_json(json!({"ErrorCode": 10, "Message": "Bad token"}));

    let rejected = failure_code(rejected).await?;
    let unauthorised = failure_code(unauthorised).await?;
    let limited = failure_code(ResponseTemplate::new(429)).await?;

    assert_eq!(rejected.get("code"), Some(&json!("postmark/rejected")));
    assert_eq!(unauthorised.get("code"), Some(&json!("postmark/unauthorised")));
    assert_eq!(limited.get("code"), Some(&json!("postmark/rate-limited")));
    Ok(())
}

#[tokio::test]
async fn template_sends_use_an_id_or_an_alias() -> TestResult {
    let by_id = expecting(
        "/email/withTemplate",
        json!({
            "TemplateId": 7,
            "TemplateModel": {"name": "Ada"},
            "From": "sender@example.com",
            "To": "to@example.com",
            "MessageStream": DEFAULT_STREAM,
        }),
        accepted(),
    )
    .await;
    let by_alias = expecting(
        "/email/withTemplate",
        json!({
            "TemplateAlias": "welcome",
            "TemplateModel": {},
            "From": "support@example.com",
            "To": "to@example.com",
            "Cc": "c@example.com",
            "Bcc": "b@example.com",
            "ReplyTo": "reply@example.com",
            "Tag": "welcome",
            "MessageStream": DEFAULT_STREAM,
        }),
        accepted(),
    )
    .await;

    let result = success(
        &by_id,
        "send_email_with_template",
        json!({"to": "to@example.com", "template_id": 7, "template_model": {"name": "Ada"}}),
    )
    .await?;
    success(
        &by_alias,
        "send_email_with_template",
        json!({
            "to": "to@example.com",
            "template_alias": "welcome",
            "template_model": {},
            "from": "support@example.com",
            "cc": "c@example.com",
            "bcc": "b@example.com",
            "reply_to": "reply@example.com",
            "tag": "welcome",
        }),
    )
    .await?;

    assert_eq!(result.get("message_id"), Some(&json!("0ac29aee")));
    Ok(())
}

#[tokio::test]
async fn template_sends_refuse_an_unclear_template_or_model() -> TestResult {
    for arguments in [
        json!({"to": "to@example.com", "template_model": {}}),
        json!({"to": "to@example.com", "template_id": 7, "template_alias": "a", "template_model": {}}),
        json!({"to": "to@example.com", "template_id": 0, "template_model": {}}),
        json!({"to": "to@example.com", "template_alias": "", "template_model": {}}),
        json!({"to": "to@example.com", "template_id": 7}),
        json!({"to": "to@example.com", "template_id": 7, "template_model": "not an object"}),
        json!({"to": "nobody", "template_id": 7, "template_model": {}}),
    ] {
        assert_refused("send_email_with_template", arguments).await?;
    }
    Ok(())
}

fn batch_result(to: Option<&str>, code: i64) -> Value {
    let mut result = json!({"ErrorCode": code, "Message": if code == 0 { "OK" } else { "Inactive recipient" }});
    if code == 0 {
        result["MessageID"] = json!(format!("id-{}", to.unwrap_or("none")));
        result["SubmittedAt"] = json!("2026-01-02T03:04:05-05:00");
    }
    if let Some(to) = to {
        result["To"] = json!(to);
    }
    result
}

#[tokio::test]
async fn a_batch_reports_each_message_by_position() -> TestResult {
    let postmark = expecting(
        "/email/batch",
        json!([
            {"From": "sender@example.com", "To": "a@example.com", "Subject": "One", "TextBody": "1", "MessageStream": DEFAULT_STREAM},
            {"From": "sender@example.com", "To": "b@example.com", "Subject": "Two", "HtmlBody": "<p>2</p>", "MessageStream": DEFAULT_STREAM},
        ]),
        json!([batch_result(Some("a@example.com"), 0), batch_result(None, 406)]),
    )
    .await;

    let result = success(
        &postmark,
        "send_batch",
        json!({
            "messages": [
                {"to": "a@example.com", "subject": "One", "text_body": "1"},
                {"to": "b@example.com", "subject": "Two", "html_body": "<p>2</p>"},
            ],
        }),
    )
    .await?;

    assert_eq!(
        result,
        json!({
            "total": 2,
            "succeeded": 1,
            "failed": 1,
            "unaccounted": 0,
            "results": [
                {
                    "index": 0,
                    "to": "a@example.com",
                    "error_code": 0,
                    "message": "OK",
                    "message_id": "id-a@example.com",
                    "submitted_at": "2026-01-02T03:04:05-05:00",
                },
                {"index": 1, "to": "b@example.com", "error_code": 406, "message": "Inactive recipient"},
            ],
        })
    );
    Ok(())
}

#[tokio::test]
async fn messages_postmark_did_not_answer_for_are_counted_as_unaccounted() -> TestResult {
    let postmark = answering(
        "/email/batch",
        ResponseTemplate::new(200).set_body_json(json!([batch_result(Some("a@example.com"), 0)])),
    )
    .await;

    let result = success(
        &postmark,
        "send_batch",
        json!({
            "messages": [
                {"to": "a@example.com", "subject": "One", "text_body": "1"},
                {"to": "b@example.com", "subject": "Two", "text_body": "2"},
            ],
        }),
    )
    .await?;

    assert_eq!(result.get("unaccounted"), Some(&json!(1)));
    assert_eq!(result.get("succeeded"), Some(&json!(1)));
    Ok(())
}

#[tokio::test]
async fn one_invalid_message_refuses_the_whole_batch_before_anything_is_sent() -> TestResult {
    let valid = json!({"to": "a@example.com", "subject": "One", "text_body": "1"});
    let too_many: Vec<_> = (0..501).map(|_| valid.clone()).collect();
    for arguments in [
        json!({"messages": []}),
        json!({"messages": too_many}),
        json!({"messages": [valid, {"to": "nobody", "subject": "Two", "text_body": "2"}]}),
        json!({"messages": [valid, {"to": "b@example.com", "subject": "Two"}]}),
    ] {
        assert_refused("send_batch", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_batch_says_which_message_is_wrong() -> TestResult {
    let postmark = MockServer::start().await;
    let valid = json!({"to": "a@example.com", "subject": "One", "text_body": "1"});

    let plain = failure(
        &postmark,
        "send_batch",
        json!({"messages": [valid, {"to": "nobody", "subject": "Two", "text_body": "2"}]}),
    )
    .await?;
    let templated = failure(
        &postmark,
        "send_batch_with_template",
        json!({
            "template_id": 7,
            "recipients": [{"to": "a@example.com", "template_model": {}}, {"to": "nobody", "template_model": {}}],
        }),
    )
    .await?;

    for result in [plain, templated] {
        let message = result.get("message").and_then(Value::as_str).unwrap_or_default();
        assert!(message.starts_with("message 1:"), "{message}");
    }
    Ok(())
}

#[tokio::test]
async fn a_batch_of_five_hundred_is_allowed() -> TestResult {
    let messages: Vec<_> = (0..500)
        .map(|_| json!({"to": "a@example.com", "subject": "One", "text_body": "1"}))
        .collect();
    let answers: Vec<_> = (0..500).map(|_| batch_result(Some("a@example.com"), 0)).collect();
    let postmark = answering("/email/batch", ResponseTemplate::new(200).set_body_json(answers)).await;

    let result = success(&postmark, "send_batch", json!({"messages": messages})).await?;

    assert_eq!(result.get("succeeded"), Some(&json!(500)));
    Ok(())
}

#[tokio::test]
async fn an_ambiguous_batch_failure_covers_the_whole_batch() -> TestResult {
    let postmark = answering("/email/batch", ResponseTemplate::new(500)).await;

    let result = failure(
        &postmark,
        "send_batch",
        json!({"messages": [{"to": "a@example.com", "subject": "One", "text_body": "1"}]}),
    )
    .await?;

    assert_eq!(result.get("code"), Some(&json!("send/outcome-unknown")));
    Ok(())
}

#[tokio::test]
async fn a_template_batch_applies_shared_values_with_per_recipient_overrides() -> TestResult {
    let postmark = expecting(
        "/email/batchWithTemplates",
        json!({
            "Messages": [
                {
                    "TemplateAlias": "welcome",
                    "TemplateModel": {"name": "Ada"},
                    "From": "shared@example.com",
                    "To": "a@example.com",
                    "Tag": "shared",
                    "MessageStream": DEFAULT_STREAM,
                },
                {
                    "TemplateAlias": "welcome",
                    "TemplateModel": {"name": "Bo"},
                    "From": "own@example.com",
                    "To": "b@example.com",
                    "Tag": "own",
                    "MessageStream": DEFAULT_STREAM,
                },
            ],
        }),
        json!([
            batch_result(Some("a@example.com"), 0),
            batch_result(Some("b@example.com"), 0)
        ]),
    )
    .await;

    let result = success(
        &postmark,
        "send_batch_with_template",
        json!({
            "template_alias": "welcome",
            "from": "shared@example.com",
            "tag": "shared",
            "recipients": [
                {"to": "a@example.com", "template_model": {"name": "Ada"}},
                {"to": "b@example.com", "template_model": {"name": "Bo"}, "from": "own@example.com", "tag": "own"},
            ],
        }),
    )
    .await?;

    assert_eq!(result.get("succeeded"), Some(&json!(2)));
    Ok(())
}

#[tokio::test]
async fn a_template_batch_falls_back_to_the_default_sender() -> TestResult {
    let postmark = expecting(
        "/email/batchWithTemplates",
        json!({
            "Messages": [{
                "TemplateId": 7,
                "TemplateModel": {},
                "From": "sender@example.com",
                "To": "a@example.com",
                "MessageStream": DEFAULT_STREAM,
            }],
        }),
        json!([batch_result(Some("a@example.com"), 0)]),
    )
    .await;

    success(
        &postmark,
        "send_batch_with_template",
        json!({"template_id": 7, "recipients": [{"to": "a@example.com", "template_model": {}}]}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn template_batches_refuse_an_unclear_template_or_invalid_message() -> TestResult {
    let valid = json!({"to": "a@example.com", "template_model": {}});
    let too_many: Vec<_> = (0..501).map(|_| valid.clone()).collect();
    for arguments in [
        json!({"recipients": [valid]}),
        json!({"template_id": 7, "template_alias": "a", "recipients": [valid]}),
        json!({"template_id": 7, "recipients": []}),
        json!({"template_id": 7, "recipients": too_many}),
        json!({"template_id": 7, "recipients": [valid, {"to": "nobody", "template_model": {}}]}),
    ] {
        assert_refused("send_batch_with_template", arguments).await?;
    }
    Ok(())
}
