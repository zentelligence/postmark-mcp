//! Contract tests for `get_server_info`, and for how the catalogue handles an
//! argument mistake or a Postmark failure.

mod support;

use serde_json::{Map, Value, json};
use support::{DEFAULT_STREAM, TestResult, call, call_raw, failure, postmark_answering, success};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn healthy() -> MockServer {
    postmark_answering(
        "GET",
        "/server",
        json!({
            "ID": 42,
            "Name": "Production",
            "ApiTokens": ["secret-from-postmark"],
            "SmtpApiActivated": true,
            "TrackLinks": "HtmlAndText",
            "BounceHookUrl": "https://user:pass@hooks.example.com/bounce",
        }),
    )
    .await
}

#[tokio::test]
async fn get_server_info_returns_the_server_and_effective_defaults() -> TestResult {
    let result = success(&healthy().await, "get_server_info", json!({})).await?;

    assert_eq!(
        result,
        json!({
            "server_id": 42,
            "name": "Production",
            "smtp_api_activated": true,
            "track_links": "HtmlAndText",
            "bounce_hook_url": "https://hooks.example.com/bounce",
            "default_sender": "sender@example.com",
            "default_message_stream": DEFAULT_STREAM,
        })
    );
    Ok(())
}

#[tokio::test]
async fn get_server_info_accepts_a_call_with_no_arguments_object() -> TestResult {
    let result = call_raw(&healthy().await, "get_server_info", None).await?;

    assert_eq!(result.is_error, Some(false));
    Ok(())
}

#[tokio::test]
async fn unknown_arguments_are_rejected_by_name() -> TestResult {
    let mut arguments = Map::new();
    arguments.insert("unexpected".to_owned(), json!(true));

    let result = call_raw(&healthy().await, "get_server_info", Some(arguments)).await?;

    assert_eq!(result.is_error, Some(true), "unknown argument was accepted: {result:?}");
    assert!(serde_json::to_string(&result)?.contains("unknown field `unexpected`"));
    Ok(())
}

#[tokio::test]
async fn rejected_tokens_become_structured_tool_errors() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/server"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"ErrorCode": 10, "Message": "Bad token"})))
        .mount(&postmark)
        .await;

    let failure = failure(&postmark, "get_server_info", json!({})).await?;

    assert_eq!(failure.get("code"), Some(&json!("postmark/unauthorised")));
    assert_eq!(
        failure.get("message"),
        Some(&json!("Postmark rejected the server token"))
    );
    assert!(
        failure
            .get("remediation")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn results_also_carry_a_text_block_for_hosts_without_structured_content() -> TestResult {
    let result = call(&healthy().await, "get_server_info", json!({})).await?;

    assert!(serde_json::to_string(&result.content)?.contains("Production"));
    Ok(())
}
