//! Contract tests for the tools that change account state: templates,
//! suppressions, bounce reactivation, and webhooks.

mod support;

use serde_json::{Value, json};
use support::{DEFAULT_STREAM, TestResult, assert_refused, failure, success};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn expecting(verb: &str, route: &str, body: Option<Value>, answer: Value) -> MockServer {
    let postmark = MockServer::start().await;
    let mut mock = Mock::given(method(verb)).and(path(route));
    if let Some(body) = body {
        mock = mock.and(body_json(body));
    }
    mock.respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .expect(1)
        .mount(&postmark)
        .await;
    postmark
}

async fn answering(route: &str, response: ResponseTemplate) -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(path(route)).respond_with(response).mount(&postmark).await;
    postmark
}

fn template_answer() -> Value {
    json!({"TemplateId": 7, "Name": "Welcome", "Active": true, "Alias": "welcome", "TemplateType": "Standard", "LayoutTemplate": null})
}

#[tokio::test]
async fn create_template_sends_only_what_was_given() -> TestResult {
    let postmark = expecting(
        "POST",
        "/templates",
        Some(json!({"Name": "Welcome", "Subject": "Hi {{name}}", "TextBody": "Hello", "Alias": "welcome"})),
        template_answer(),
    )
    .await;

    let result = success(
        &postmark,
        "create_template",
        json!({"name": "Welcome", "subject": "Hi {{name}}", "text_body": "Hello", "alias": "welcome"}),
    )
    .await?;

    assert_eq!(
        result,
        json!({"template_id": 7, "name": "Welcome", "active": true, "alias": "welcome", "template_type": "Standard"})
    );
    Ok(())
}

#[tokio::test]
async fn create_template_supports_layouts_and_templates_that_use_them() -> TestResult {
    let layout = expecting(
        "POST",
        "/templates",
        Some(json!({"Name": "Base", "HtmlBody": "<main>{{{@content}}}</main>", "TemplateType": "Layout", "Alias": "base"})),
        template_answer(),
    )
    .await;
    let standard = expecting(
        "POST",
        "/templates",
        Some(json!({"Name": "Welcome", "Subject": "Hi", "HtmlBody": "<p>Hi</p>", "LayoutTemplate": "base"})),
        template_answer(),
    )
    .await;

    success(
        &layout,
        "create_template",
        json!({"name": "Base", "html_body": "<main>{{{@content}}}</main>", "template_type": "Layout", "alias": "base"}),
    )
    .await?;
    success(
        &standard,
        "create_template",
        json!({"name": "Welcome", "subject": "Hi", "html_body": "<p>Hi</p>", "layout_template": "base"}),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn create_template_refuses_what_postmark_would() -> TestResult {
    for arguments in [
        json!({"subject": "Hi", "text_body": "Hello"}),
        json!({"name": "", "subject": "Hi", "text_body": "Hello"}),
        json!({"name": "Welcome", "subject": "Hi"}),
        json!({"name": "Welcome", "text_body": "Hello"}),
        json!({"name": "Base", "subject": "Hi", "html_body": "{{{@content}}}", "template_type": "Layout"}),
        json!({"name": "Base", "html_body": "{{{@content}}}", "template_type": "Layout", "layout_template": "other"}),
        json!({"name": "Welcome", "subject": "Hi", "text_body": "Hello", "alias": "1starts-with-digit"}),
        json!({"name": "Welcome", "subject": "Hi", "text_body": "Hello", "alias": "has space"}),
        json!({"name": "Welcome", "subject": "Hi", "text_body": "Hello", "template_type": "Other"}),
        json!({"name": "Welcome", "subject": "", "text_body": "Hello"}),
        json!({"name": "Welcome", "subject": "Hi", "text_body": ""}),
        json!({"name": "Welcome", "subject": "Hi", "html_body": "", "text_body": "Hello"}),
        json!({"name": "Base", "subject": "", "html_body": "{{{@content}}}", "template_type": "Layout"}),
    ] {
        assert_refused("create_template", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_ambiguous_create_says_the_template_may_exist() -> TestResult {
    let postmark = answering("/templates", ResponseTemplate::new(503)).await;

    let result = failure(
        &postmark,
        "create_template",
        json!({"name": "Welcome", "subject": "Hi", "text_body": "Hello"}),
    )
    .await?;

    assert_eq!(result.get("code"), Some(&json!("change/outcome-unknown")));
    let remediation = result.get("remediation").and_then(Value::as_str).unwrap_or_default();
    assert!(remediation.contains("list_templates"), "{remediation}");
    assert_eq!(postmark.received_requests().await.unwrap_or_default().len(), 1);
    Ok(())
}

fn current_template() -> Value {
    json!({
        "TemplateId": 7,
        "Name": "Welcome",
        "Subject": "Old subject",
        "HtmlBody": "<p>Old</p>",
        "TextBody": "Old",
        "Alias": "welcome",
        "TemplateType": "Standard",
        "LayoutTemplate": "base",
        "Active": true,
    })
}

/// A stand-in that serves the current template, and accepts only this exact edit.
async fn editing(route: &str, current: Value, expected: Value) -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(current))
        .mount(&postmark)
        .await;
    Mock::given(method("PUT"))
        .and(path(route))
        .and(body_json(expected))
        .respond_with(ResponseTemplate::new(200).set_body_json(template_answer()))
        .expect(1)
        .mount(&postmark)
        .await;
    postmark
}

#[tokio::test]
async fn edit_template_sends_every_current_value_with_only_the_changes_applied() -> TestResult {
    // Postmark's documentation does not settle whether an edit keeps the fields
    // it leaves out, so the whole template is sent, changed only where asked.
    let postmark = editing(
        "/templates/welcome",
        current_template(),
        json!({
            "Name": "Welcome",
            "Subject": "New subject",
            "HtmlBody": "<p>Old</p>",
            "TextBody": "Old",
            "Alias": "welcome",
            "LayoutTemplate": "base",
        }),
    )
    .await;

    let result = success(
        &postmark,
        "edit_template",
        json!({"template_id_or_alias": "welcome", "subject": "New subject"}),
    )
    .await?;

    assert_eq!(result.get("template_id"), Some(&json!(7)));
    Ok(())
}

#[tokio::test]
async fn edit_template_can_detach_a_layout() -> TestResult {
    let postmark = editing(
        "/templates/7",
        current_template(),
        json!({
            "Name": "Welcome",
            "Subject": "Old subject",
            "HtmlBody": "<p>Old</p>",
            "TextBody": "Old",
            "Alias": "welcome",
            "LayoutTemplate": "",
        }),
    )
    .await;

    success(
        &postmark,
        "edit_template",
        json!({"template_id_or_alias": 7, "detach_layout": true}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn edit_template_keeps_a_layout_free_of_a_subject() -> TestResult {
    let mut layout = current_template();
    layout["TemplateType"] = json!("Layout");
    layout["Subject"] = Value::Null;
    layout["LayoutTemplate"] = Value::Null;
    let postmark = editing(
        "/templates/base",
        layout,
        json!({"Name": "Renamed", "HtmlBody": "<p>Old</p>", "TextBody": "Old", "Alias": "welcome"}),
    )
    .await;

    success(
        &postmark,
        "edit_template",
        json!({"template_id_or_alias": "base", "name": "Renamed"}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn edit_template_changes_nothing_if_the_template_cannot_be_read() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 1101, "Message": "Not found"})))
        .mount(&postmark)
        .await;

    let result = failure(
        &postmark,
        "edit_template",
        json!({"template_id_or_alias": "missing", "subject": "x"}),
    )
    .await?;

    assert_eq!(result.get("code"), Some(&json!("postmark/rejected")));
    let methods: Vec<_> = postmark
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|request| request.method.to_string())
        .collect();
    assert_eq!(methods, ["GET"]);
    Ok(())
}

#[tokio::test]
async fn edit_template_refuses_an_empty_or_contradictory_change() -> TestResult {
    for arguments in [
        json!({"template_id_or_alias": "welcome"}),
        json!({"template_id_or_alias": "welcome", "subject": ""}),
        json!({"template_id_or_alias": "welcome", "detach_layout": false}),
        json!({"template_id_or_alias": "welcome", "layout_template": "base", "detach_layout": true}),
        json!({"template_id_or_alias": "..", "subject": "x"}),
        json!({"subject": "x"}),
    ] {
        assert_refused("edit_template", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn delete_template_reports_what_was_deleted() -> TestResult {
    let postmark = expecting(
        "DELETE",
        "/templates/welcome",
        None,
        json!({"ErrorCode": 0, "Message": "Template 7 removed."}),
    )
    .await;

    let result = success(&postmark, "delete_template", json!({"template_id_or_alias": "welcome"})).await?;

    assert_eq!(result, json!({"deleted": "welcome"}));
    Ok(())
}

#[tokio::test]
async fn a_refused_delete_is_a_failure_even_with_a_success_status() -> TestResult {
    let postmark = answering(
        "/templates/base",
        ResponseTemplate::new(200).set_body_json(json!({"ErrorCode": 1130, "Message": "Layout in use"})),
    )
    .await;

    let result = failure(&postmark, "delete_template", json!({"template_id_or_alias": "base"})).await?;

    assert_eq!(result.get("code"), Some(&json!("postmark/rejected")));
    Ok(())
}

#[tokio::test]
async fn activate_bounce_returns_the_reactivated_bounce() -> TestResult {
    let postmark = expecting(
        "PUT",
        "/bounces/99/activate",
        None,
        json!({
            "Message": "OK",
            "Bounce": {"ID": 99, "Type": "HardBounce", "Email": "to@example.com", "Inactive": false, "CanActivate": true, "Content": "secret"},
        }),
    )
    .await;

    let result = success(&postmark, "activate_bounce", json!({"bounce_id": 99})).await?;

    assert_eq!(
        result,
        json!({"bounce_id": 99, "type": "HardBounce", "email": "to@example.com", "inactive": false, "can_activate": true})
    );
    Ok(())
}

#[tokio::test]
async fn activate_bounce_sends_an_empty_body_with_its_length() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/bounces/99/activate"))
        .and(wiremock::matchers::header("content-length", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Bounce": {"ID": 99}})))
        .expect(1)
        .mount(&postmark)
        .await;

    success(&postmark, "activate_bounce", json!({"bounce_id": 99})).await?;

    Ok(())
}

#[tokio::test]
async fn create_webhook_uses_the_default_stream_when_none_is_given() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhooks"))
        .and(wiremock::matchers::body_partial_json(
            json!({"MessageStream": DEFAULT_STREAM}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(webhook_answer()))
        .expect(1)
        .mount(&postmark)
        .await;

    success(
        &postmark,
        "create_webhook",
        json!({"url": "https://hooks.example.com/postmark", "triggers": ["Open"]}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn activate_bounce_refuses_impossible_ids() -> TestResult {
    assert_refused("activate_bounce", json!({"bounce_id": 0})).await?;
    assert_refused("activate_bounce", json!({})).await
}

#[tokio::test]
async fn suppressions_are_added_on_the_default_stream_and_reported_per_address() -> TestResult {
    let route = format!("/message-streams/{DEFAULT_STREAM}/suppressions");
    let postmark = expecting(
        "POST",
        &route,
        Some(json!({"Suppressions": [{"EmailAddress": "a@example.com"}, {"EmailAddress": "b@example.com"}]})),
        json!({"Suppressions": [
            {"EmailAddress": "a@example.com", "Status": "Suppressed", "Message": null},
            {"EmailAddress": "b@example.com", "Status": "Failed", "Message": "An invalid email address was provided."},
        ]}),
    )
    .await;

    let result = success(
        &postmark,
        "create_suppressions",
        json!({"email_addresses": ["a@example.com", "b@example.com"]}),
    )
    .await?;

    assert_eq!(
        result,
        json!({
            "message_stream": DEFAULT_STREAM,
            "succeeded": 1,
            "failed": 1,
            "results": [
                {"email_address": "a@example.com", "status": "Suppressed"},
                {"email_address": "b@example.com", "status": "Failed", "message": "An invalid email address was provided."},
            ],
        })
    );
    Ok(())
}

#[tokio::test]
async fn suppressions_are_lifted_on_a_chosen_stream() -> TestResult {
    let postmark = expecting(
        "POST",
        "/message-streams/broadcast/suppressions/delete",
        Some(json!({"Suppressions": [{"EmailAddress": "a@example.com"}]})),
        json!({"Suppressions": [{"EmailAddress": "a@example.com", "Status": "Deleted", "Message": null}]}),
    )
    .await;

    let result = success(
        &postmark,
        "delete_suppressions",
        json!({"email_addresses": ["a@example.com"], "message_stream": "broadcast"}),
    )
    .await?;

    assert_eq!(result.get("succeeded"), Some(&json!(1)));
    assert_eq!(result.get("message_stream"), Some(&json!("broadcast")));
    Ok(())
}

#[tokio::test]
async fn suppression_changes_refuse_what_cannot_be_an_address_list() -> TestResult {
    let fifty_one: Vec<_> = (0..51).map(|index| format!("r{index}@example.com")).collect();
    for tool in ["create_suppressions", "delete_suppressions"] {
        for arguments in [
            json!({"email_addresses": []}),
            json!({"email_addresses": fifty_one.clone()}),
            json!({"email_addresses": ["not-an-address"]}),
            json!({"email_addresses": ["Name <a@example.com>"]}),
            json!({"email_addresses": ["a@example.com"], "message_stream": "a/b"}),
        ] {
            assert_refused(tool, arguments).await?;
        }
    }
    Ok(())
}

fn webhook_answer() -> Value {
    json!({
        "ID": 12,
        "Url": "https://hooks.example.com/postmark",
        "MessageStream": "outbound",
        "Status": "verified",
        "HttpAuth": {"Username": "u", "Password": "p"},
        "Triggers": {"Open": {"Enabled": true}, "Bounce": {"Enabled": true}},
    })
}

#[tokio::test]
async fn create_webhook_enables_exactly_the_triggers_asked_for() -> TestResult {
    let postmark = expecting(
        "POST",
        "/webhooks",
        Some(json!({
            "Url": "https://hooks.example.com/postmark",
            "MessageStream": "outbound",
            "Triggers": {
                "Open": {"Enabled": true},
                "Click": {"Enabled": false},
                "Delivery": {"Enabled": false},
                "Bounce": {"Enabled": true},
                "SpamComplaint": {"Enabled": false},
                "SubscriptionChange": {"Enabled": false},
            },
        })),
        webhook_answer(),
    )
    .await;

    let result = success(
        &postmark,
        "create_webhook",
        json!({
            "url": "https://hooks.example.com/postmark",
            "message_stream": "outbound",
            "triggers": ["Open", "Bounce"],
        }),
    )
    .await?;

    assert_eq!(
        result,
        json!({
            "id": 12,
            "url": "https://hooks.example.com/postmark",
            "message_stream": "outbound",
            "status": "verified",
            "triggers": ["Open", "Bounce"],
        })
    );
    Ok(())
}

#[tokio::test]
async fn create_webhook_refuses_unsafe_or_empty_hooks() -> TestResult {
    for arguments in [
        json!({"url": "http://hooks.example.com/postmark", "triggers": ["Open"]}),
        json!({"url": "https://user:pass@hooks.example.com/postmark", "triggers": ["Open"]}),
        json!({"url": "not a url", "triggers": ["Open"]}),
        json!({"url": "https://hooks.example.com/postmark", "triggers": []}),
        json!({"url": "https://hooks.example.com/postmark", "triggers": ["Opened"]}),
        json!({"url": "https://hooks.example.com/postmark"}),
        json!({"url": "https://hooks.example.com/allowed/..%2Fother", "triggers": ["Open"]}),
        json!({"url": "https://hooks.example.com/a%2fb", "triggers": ["Open"]}),
    ] {
        assert_refused("create_webhook", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_ambiguous_webhook_create_says_it_may_exist() -> TestResult {
    let postmark = answering("/webhooks", ResponseTemplate::new(500)).await;

    let result = failure(
        &postmark,
        "create_webhook",
        json!({"url": "https://hooks.example.com/postmark", "triggers": ["Open"]}),
    )
    .await?;

    assert_eq!(result.get("code"), Some(&json!("change/outcome-unknown")));
    let remediation = result.get("remediation").and_then(Value::as_str).unwrap_or_default();
    assert!(remediation.contains("list_webhooks"), "{remediation}");
    Ok(())
}

#[tokio::test]
async fn delete_webhook_reports_what_was_deleted() -> TestResult {
    let postmark = expecting(
        "DELETE",
        "/webhooks/12",
        None,
        json!({"ErrorCode": 0, "Message": "Webhook 12 removed."}),
    )
    .await;

    let result = success(&postmark, "delete_webhook", json!({"webhook_id": 12})).await?;

    assert_eq!(result, json!({"deleted": "12"}));
    Ok(())
}

#[tokio::test]
async fn delete_webhook_refuses_impossible_ids() -> TestResult {
    assert_refused("delete_webhook", json!({"webhook_id": 0})).await
}

#[tokio::test]
async fn a_delete_that_may_have_happened_says_to_check_rather_than_retry() -> TestResult {
    for (tool, route, arguments, check) in [
        (
            "delete_webhook",
            "/webhooks/12",
            json!({"webhook_id": 12}),
            "list_webhooks",
        ),
        (
            "delete_template",
            "/templates/welcome",
            json!({"template_id_or_alias": "welcome"}),
            "list_templates",
        ),
    ] {
        for response in [
            ResponseTemplate::new(503),
            ResponseTemplate::new(200).set_body_json(json!({"Message": "OK"})),
        ] {
            let postmark = answering(route, response).await;

            let result = failure(&postmark, tool, arguments.clone()).await?;

            assert_eq!(result.get("code"), Some(&json!("change/outcome-unknown")), "{tool}");
            let remediation = result.get("remediation").and_then(Value::as_str).unwrap_or_default();
            assert!(remediation.contains(check), "{tool}: {remediation}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_delete_postmark_refuses_keeps_postmarks_code() -> TestResult {
    let postmark = answering(
        "/webhooks/12",
        ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 1200, "Message": "Not found"})),
    )
    .await;

    let result = failure(&postmark, "delete_webhook", json!({"webhook_id": 12})).await?;

    assert_eq!(result.get("code"), Some(&json!("postmark/rejected")));
    Ok(())
}

#[tokio::test]
async fn encoded_dot_segments_are_resolved_before_any_check() -> TestResult {
    // The URL parser resolves `%2e%2e` as `..`, so the webhook is registered at
    // the resolved path, which is the path every check sees.
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhooks"))
        .and(wiremock::matchers::body_partial_json(
            json!({"Url": "https://hooks.example.com/x"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(webhook_answer()))
        .expect(1)
        .mount(&postmark)
        .await;

    success(
        &postmark,
        "create_webhook",
        json!({"url": "https://hooks.example.com/a/%2e%2e/x", "triggers": ["Open"]}),
    )
    .await?;

    Ok(())
}
