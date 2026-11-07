//! End-to-end tests of the guardrails around sending and the tool log, driving
//! the real binary so that the environment variables are proved to work too.

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};
use tokio::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;
type Client = RunningService<RoleClient, ()>;

const SEND_TOOLS: [&str; 4] = [
    "send_batch",
    "send_batch_with_template",
    "send_email",
    "send_email_with_template",
];

fn binary(postmark: &MockServer, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_postmark-mcp"));
    // A closed stdin ends a server that starts when it should have refused, so a
    // test fails rather than hangs.
    command
        .stdin(Stdio::null())
        .env_clear()
        .env("POSTMARK_SERVER_TOKEN", "policy-test-token")
        .env("DEFAULT_SENDER_EMAIL", "sender@example.com")
        .env("DEFAULT_MESSAGE_STREAM", "outbound")
        .env("POSTMARK_API_BASE_URL", postmark.uri())
        .env("POSTMARK_SKIP_VERIFY", "true");
    for (name, value) in env {
        command.env(name, value);
    }
    command
}

async fn client_with_args(postmark: &MockServer, args: &[&str]) -> Result<Client, Box<dyn Error + Send + Sync>> {
    let mut command = binary(postmark, &[]);
    command.args(args);
    Ok(().serve(TokioChildProcess::new(command)?).await?)
}

async fn client(postmark: &MockServer, env: &[(&str, &str)]) -> Result<Client, Box<dyn Error + Send + Sync>> {
    Ok(().serve(TokioChildProcess::new(binary(postmark, env))?).await?)
}

async fn accepting() -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/email"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "To": "to@example.com", "SubmittedAt": "2026-01-02T03:04:05-05:00", "MessageID": "abc", "ErrorCode": 0, "Message": "OK",
        })))
        .mount(&postmark)
        .await;
    postmark
}

async fn sends(postmark: &MockServer) -> usize {
    postmark.received_requests().await.unwrap_or_default().len()
}

fn params(name: &'static str, arguments: Value) -> Result<CallToolRequestParams, Box<dyn Error + Send + Sync>> {
    let Value::Object(arguments) = arguments else {
        return Err("arguments must be an object".into());
    };
    Ok(CallToolRequestParams::new(name).with_arguments(arguments))
}

fn email(to: &str) -> Value {
    json!({"to": to, "subject": "Hello", "text_body": "Hi"})
}

/// Calls `send_email`, returning its failure code, if any.
async fn send_code(client: &Client, arguments: Value) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let result = client.call_tool(params("send_email", arguments)?).await?;
    Ok(result
        .structured_content
        .as_ref()
        .and_then(|content| content.get("code"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

#[tokio::test]
async fn sending_is_available_by_default() -> TestResult {
    let postmark = accepting().await;
    let client = client(&postmark, &[]).await?;

    let names: HashSet<_> = client
        .list_all_tools()
        .await?
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    client.cancel().await?;

    for tool in SEND_TOOLS {
        assert!(names.contains(tool), "{tool} is missing");
    }
    Ok(())
}

#[tokio::test]
async fn the_read_toolset_alone_removes_the_send_tools_entirely() -> TestResult {
    let postmark = accepting().await;
    let client = client_with_args(&postmark, &["--toolsets", "read"]).await?;

    let names: HashSet<_> = client
        .list_all_tools()
        .await?
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    let call = client.call_tool(params("send_email", email("to@example.com"))?).await;
    client.cancel().await?;

    for tool in SEND_TOOLS {
        assert!(!names.contains(tool), "{tool} is still listed");
    }
    assert!(names.contains("get_server_info"));
    assert!(call.is_err(), "send_email was callable: {call:?}");
    assert_eq!(sends(&postmark).await, 0);
    Ok(())
}

#[tokio::test]
async fn the_allowlist_permits_exact_addresses_and_whole_domains() -> TestResult {
    let postmark = accepting().await;
    let allowlist = [("POSTMARK_RECIPIENT_ALLOWLIST", "friend@elsewhere.com, @example.com")];
    let client = client(&postmark, &allowlist).await?;

    for to in [
        "friend@elsewhere.com",
        "FRIEND@ELSEWHERE.COM",
        "anyone@example.com",
        "Anyone <anyone@EXAMPLE.com>",
    ] {
        assert_eq!(send_code(&client, email(to)).await?, None, "{to}");
    }
    client.cancel().await?;

    assert_eq!(sends(&postmark).await, 4);
    Ok(())
}

#[tokio::test]
async fn the_allowlist_refuses_everything_else_before_any_request() -> TestResult {
    let postmark = accepting().await;
    let allowlist = [("POSTMARK_RECIPIENT_ALLOWLIST", "friend@elsewhere.com,@example.com")];
    let client = client(&postmark, &allowlist).await?;

    for arguments in [
        email("stranger@other.com"),
        email("other@elsewhere.com"),
        email("a@sub.example.com"),
        email("a@example.com.evil.com"),
        json!({"to": ["a@example.com", "stranger@other.com"], "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "a@example.com", "cc": "stranger@other.com", "subject": "Hello", "text_body": "Hi"}),
        json!({"to": "a@example.com", "bcc": "stranger@other.com", "subject": "Hello", "text_body": "Hi"}),
    ] {
        assert_eq!(
            send_code(&client, arguments.clone()).await?.as_deref(),
            Some("policy/recipient-not-allowed"),
            "{arguments}"
        );
    }
    client.cancel().await?;

    assert_eq!(sends(&postmark).await, 0);
    Ok(())
}

#[tokio::test]
async fn the_allowlist_refuses_a_whole_batch_for_one_recipient() -> TestResult {
    let postmark = MockServer::start().await;
    let client = client(&postmark, &[("POSTMARK_RECIPIENT_ALLOWLIST", "@example.com")]).await?;

    let plain = client
        .call_tool(params(
            "send_batch",
            json!({"messages": [email("a@example.com"), email("stranger@other.com")]}),
        )?)
        .await?;
    let templated = client
        .call_tool(params(
            "send_batch_with_template",
            json!({
                "template_id": 7,
                "recipients": [
                    {"to": "a@example.com", "template_model": {}},
                    {"to": "stranger@other.com", "template_model": {}},
                ],
            }),
        )?)
        .await?;
    let single = client
        .call_tool(params(
            "send_email_with_template",
            json!({"to": "stranger@other.com", "template_id": 7, "template_model": {}}),
        )?)
        .await?;
    client.cancel().await?;

    for result in [plain, templated, single] {
        assert_eq!(
            result
                .structured_content
                .as_ref()
                .and_then(|content| content.get("code")),
            Some(&json!("policy/recipient-not-allowed"))
        );
    }
    assert_eq!(sends(&postmark).await, 0);
    Ok(())
}

#[tokio::test]
async fn a_refusal_does_not_reveal_the_allowlist() -> TestResult {
    let postmark = MockServer::start().await;
    let client = client(
        &postmark,
        &[("POSTMARK_RECIPIENT_ALLOWLIST", "secret-partner@example.com")],
    )
    .await?;

    let result = client
        .call_tool(params("send_email", email("stranger@other.com"))?)
        .await?;
    client.cancel().await?;

    let serialised = serde_json::to_string(&result)?;
    assert!(serialised.contains("policy/recipient-not-allowed"));
    assert!(!serialised.contains("secret-partner"));
    Ok(())
}

#[tokio::test]
async fn invalid_guardrail_settings_stop_the_server_rather_than_being_ignored() -> TestResult {
    let postmark = MockServer::start().await;
    for setting in [
        ("POSTMARK_TOOLSETS", "read,write"),
        ("POSTMARK_RECIPIENT_ALLOWLIST", "not-an-address"),
        ("POSTMARK_RECIPIENT_ALLOWLIST", "@"),
        ("POSTMARK_RECIPIENT_ALLOWLIST", "a@example.com,,b@example.com"),
        ("LOG_EMAIL_FULL", "maybe"),
    ] {
        let output = binary(&postmark, &[setting]).output().await?;

        assert!(!output.status.success(), "{setting:?} was accepted");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(setting.0),
            "{setting:?}"
        );
    }
    Ok(())
}

fn log_path(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!("postmark-mcp-{}-{test}.log", std::process::id()))
}

fn log_entries(path: &PathBuf) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    fs::read_to_string(path)?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

#[tokio::test]
async fn the_log_file_records_calls_without_their_content() -> TestResult {
    let log = log_path("content");
    let postmark = accepting().await;
    let log_setting = log.to_string_lossy().into_owned();
    let client = client(&postmark, &[("LOG_FILE", &log_setting)]).await?;

    client
        .call_tool(params(
            "send_email",
            json!({
                "to": "recipient@example.com",
                "cc": ["Carol Danvers <carol@example.com>"],
                "subject": "Your secret subject",
                "text_body": "The secret body",
                "html_body": "<p>The secret html</p>",
                "tag": "welcome",
            }),
        )?)
        .await?;
    client.cancel().await?;

    let raw = fs::read_to_string(&log)?;
    let entries = log_entries(&log)?;
    fs::remove_file(&log)?;

    for secret in [
        "secret subject",
        "secret body",
        "secret html",
        "recipient@",
        "carol@",
        "Carol",
        "policy-test-token",
    ] {
        assert!(!raw.contains(secret), "{secret} reached the log");
    }
    let entry = entries.first().ok_or("nothing was logged")?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entry.get("tool"), Some(&json!("send_email")));
    assert_eq!(entry.get("status"), Some(&json!("ok")));
    assert!(entry.get("duration_ms").is_some_and(Value::is_u64));
    assert!(entry.get("timestamp_ms").is_some_and(Value::is_u64));
    assert_eq!(entry.pointer("/args/to"), Some(&json!("r*******t@example.com")));
    assert_eq!(entry.pointer("/args/cc/0"), Some(&json!("c***l@example.com")));
    assert_eq!(entry.pointer("/args/text_body"), Some(&json!("[15 chars]")));
    assert_eq!(entry.pointer("/args/tag"), Some(&json!("welcome")));
    Ok(())
}

#[tokio::test]
async fn the_log_file_records_failures_with_their_code() -> TestResult {
    let log = log_path("failure");
    let postmark = MockServer::start().await;
    let log_setting = log.to_string_lossy().into_owned();
    let client = client(
        &postmark,
        &[
            ("LOG_FILE", &log_setting),
            ("POSTMARK_RECIPIENT_ALLOWLIST", "@example.com"),
        ],
    )
    .await?;

    client
        .call_tool(params("send_email", email("stranger@other.com"))?)
        .await?;
    client.cancel().await?;

    let entries = log_entries(&log)?;
    fs::remove_file(&log)?;

    let entry = entries.first().ok_or("nothing was logged")?;
    assert_eq!(entry.get("status"), Some(&json!("error")));
    assert_eq!(entry.get("code"), Some(&json!("policy/recipient-not-allowed")));
    Ok(())
}

#[tokio::test]
async fn full_addresses_are_logged_only_on_request() -> TestResult {
    let log = log_path("full");
    let postmark = accepting().await;
    let log_setting = log.to_string_lossy().into_owned();
    let client = client(&postmark, &[("LOG_FILE", &log_setting), ("LOG_EMAIL_FULL", "true")]).await?;

    client
        .call_tool(params("send_email", email("recipient@example.com"))?)
        .await?;
    client.cancel().await?;

    let entries = log_entries(&log)?;
    fs::remove_file(&log)?;

    assert_eq!(
        entries.first().and_then(|entry| entry.pointer("/args/to")),
        Some(&json!("recipient@example.com"))
    );
    Ok(())
}

#[tokio::test]
async fn an_unwritable_log_file_stops_the_server() -> TestResult {
    let postmark = MockServer::start().await;
    let missing = std::env::temp_dir().join("postmark-mcp-no-such-directory").join("log");
    let missing = missing.to_string_lossy().into_owned();

    let output = binary(&postmark, &[("LOG_FILE", &missing)]).output().await?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("LOG_FILE"));
    Ok(())
}

#[tokio::test]
async fn content_under_unexpected_names_never_reaches_the_log_or_stderr() -> TestResult {
    let log = log_path("unexpected");
    let postmark = accepting().await;
    let log_setting = log.to_string_lossy().into_owned();
    let (transport, stderr) = TokioChildProcess::builder(binary(&postmark, &[("LOG_FILE", &log_setting)]))
        .stderr(Stdio::piped())
        .spawn()?;
    let client = ().serve(transport).await?;

    let arguments = json!({
        "to": "recipient@example.com",
        "subject": {"nested": "secret subject in an object"},
        "textBody": "secret reset code 123456",
        "HtmlBody": "<p>secret html</p>",
        "templateModel": {"link": "https://example.com/reset?t=secrettoken"},
        "template_model": "secret model as a string",
        "body": "secret plain body",
    });
    let refused = client.call_tool(params("send_email", arguments)?).await?;
    client.cancel().await?;

    let mut stderr_text = String::new();
    if let Some(mut stderr) = stderr {
        tokio::io::AsyncReadExt::read_to_string(&mut stderr, &mut stderr_text).await?;
    }
    let logged = fs::read_to_string(&log)?;
    fs::remove_file(&log)?;

    assert_eq!(refused.is_error, Some(true));
    assert!(logged.contains("send_email"), "the call was not logged");
    for (channel, text) in [("LOG_FILE", &logged), ("stderr", &stderr_text)] {
        for secret in ["secret", "123456", "recipient@"] {
            assert!(!text.contains(secret), "{secret} reached {channel}: {text}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_allowed_address_cannot_cover_for_another_in_its_display_name() -> TestResult {
    let postmark = accepting().await;
    let client = client(&postmark, &[("POSTMARK_RECIPIENT_ALLOWLIST", "friend@elsewhere.com")]).await?;

    let result = client
        .call_tool(params(
            "send_email",
            email("stranger@other.com <friend@elsewhere.com>"),
        )?)
        .await?;
    client.cancel().await?;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(sends(&postmark).await, 0);
    Ok(())
}

#[tokio::test]
async fn the_webhook_allowlist_refuses_other_destinations_before_any_request() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhooks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"ID": 1, "Url": "https://hooks.example.com/postmark/in"})),
        )
        .mount(&postmark)
        .await;
    let client = client_with_args(
        &postmark,
        &["--webhook-url-allowlist", "https://hooks.example.com/postmark"],
    )
    .await?;

    let mut codes = Vec::new();
    for url in [
        "https://hooks.example.com/postmark-evil",
        "https://evil.example.com/postmark",
        "https://hooks.example.com/postmark/in",
    ] {
        let result = client
            .call_tool(params("create_webhook", json!({"url": url, "triggers": ["Open"]}))?)
            .await?;
        codes.push(
            result
                .structured_content
                .as_ref()
                .and_then(|content| content.get("code"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        );
    }
    client.cancel().await?;

    assert_eq!(
        codes,
        [
            Some("policy/webhook-url-not-allowed".to_owned()),
            Some("policy/webhook-url-not-allowed".to_owned()),
            None
        ]
    );
    assert_eq!(sends(&postmark).await, 1, "only the permitted webhook reached Postmark");
    Ok(())
}

#[tokio::test]
async fn the_webhook_allowlist_variable_is_honoured_too() -> TestResult {
    let postmark = MockServer::start().await;
    let client = client(&postmark, &[("WEBHOOK_URL_ALLOWLIST", "https://hooks.example.com")]).await?;

    let result = client
        .call_tool(params(
            "create_webhook",
            json!({"url": "https://evil.example.com/x", "triggers": ["Open"]}),
        )?)
        .await?;
    client.cancel().await?;

    assert_eq!(
        result
            .structured_content
            .as_ref()
            .and_then(|content| content.get("code")),
        Some(&json!("policy/webhook-url-not-allowed"))
    );
    assert_eq!(sends(&postmark).await, 0);
    Ok(())
}
