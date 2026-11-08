//! Configuration through command-line flags, which every MCP host can pass,
//! unlike environment variables, which some hosts cannot set. Each test drives the
//! real binary with no environment at all unless it is testing the fallback.

mod isolation;

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::{Output, Stdio};

use rmcp::ServiceExt;
use rmcp::transport::TokioChildProcess;
use serde_json::json;
use tokio::process::Command;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

/// The binary with an empty environment, a closed stdin, and these arguments.
fn binary(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_postmark-mcp"));
    isolation::isolate(&mut command).stdin(Stdio::null()).args(args);
    command
}

fn base_args(postmark: &MockServer) -> Vec<String> {
    [
        "--default-sender",
        "sender@example.com",
        "--default-message-stream",
        "outbound",
        "--api-base-url",
        &postmark.uri(),
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect()
}

fn with(mut args: Vec<String>, extra: &[&str]) -> Vec<String> {
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    args
}

fn refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// A stand-in for Postmark that accepts `/server` only with this token.
async fn postmark_accepting(token: &str) -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/server"))
        .and(header("X-Postmark-Server-Token", token))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ID": 1, "Name": "Test"})))
        .mount(&postmark)
        .await;
    postmark
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

async fn tool_names(args: &[String]) -> Result<HashSet<String>, Box<dyn Error + Send + Sync>> {
    let client = ().serve(TokioChildProcess::new(binary(&refs(args)))?).await?;
    let names = client
        .list_all_tools()
        .await?
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    client.cancel().await?;
    Ok(names)
}

const READ_TOOLS: [&str; 12] = [
    "diagnose_delivery",
    "get_bounce_dump",
    "get_delivery_stats",
    "get_message_details",
    "get_server_info",
    "get_template",
    "list_suppressions",
    "list_templates",
    "list_webhooks",
    "search_bounces",
    "search_outbound_messages",
    "validate_template",
];

const SEND_TOOLS: [&str; 4] = [
    "send_batch",
    "send_batch_with_template",
    "send_email",
    "send_email_with_template",
];

const MANAGE_TOOLS: [&str; 8] = [
    "activate_bounce",
    "create_suppressions",
    "create_template",
    "create_webhook",
    "delete_suppressions",
    "delete_template",
    "delete_webhook",
    "edit_template",
];

fn set(groups: &[&[&str]]) -> HashSet<String> {
    groups
        .iter()
        .flat_map(|group| group.iter())
        .map(|name| (*name).to_owned())
        .collect()
}

#[tokio::test]
async fn flags_alone_configure_the_server() -> TestResult {
    let postmark = postmark_accepting("flag-token").await;
    let args = with(base_args(&postmark), &["--server-token", "flag-token", "doctor"]);

    let output = binary(&refs(&args)).output().await?;

    assert!(output.status.success(), "{}", stderr(&output));
    Ok(())
}

#[tokio::test]
async fn the_token_can_be_read_from_a_file() -> TestResult {
    let file = std::env::temp_dir().join(format!("postmark-mcp-token-{}", std::process::id()));
    fs::write(&file, "file-token\n")?;
    let postmark = postmark_accepting("file-token").await;
    let file_arg = file.to_string_lossy().into_owned();
    let args = with(base_args(&postmark), &["--server-token-file", &file_arg, "doctor"]);

    let output = binary(&refs(&args)).output().await?;
    fs::remove_file(&file)?;

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stderr(&output).contains("file-token"), "the token reached stderr");
    Ok(())
}

#[tokio::test]
async fn an_unreadable_token_file_stops_the_server() -> TestResult {
    let postmark = MockServer::start().await;
    let missing: PathBuf = std::env::temp_dir().join("postmark-mcp-no-such-token");
    let missing = missing.to_string_lossy().into_owned();
    let args = with(base_args(&postmark), &["--server-token-file", &missing]);

    let output = binary(&refs(&args)).output().await?;

    assert!(!output.status.success());
    assert!(stderr(&output).contains("--server-token-file"), "{}", stderr(&output));
    Ok(())
}

#[tokio::test]
async fn two_token_flags_are_refused_rather_than_one_chosen() -> TestResult {
    let postmark = MockServer::start().await;
    let args = with(
        base_args(&postmark),
        &["--server-token", "a", "--server-token-file", "/dev/null", "doctor"],
    );

    let output = binary(&refs(&args)).output().await?;

    assert!(!output.status.success());
    Ok(())
}

#[tokio::test]
async fn a_flag_overrides_the_environment() -> TestResult {
    let postmark = postmark_accepting("flag-token").await;
    let args = with(base_args(&postmark), &["--server-token", "flag-token", "doctor"]);
    let mut command = binary(&refs(&args));
    command.env("POSTMARK_SERVER_TOKEN", "environment-token");

    let output = command.output().await?;

    assert!(output.status.success(), "{}", stderr(&output));
    Ok(())
}

#[tokio::test]
async fn the_environment_still_works_where_a_host_can_set_it() -> TestResult {
    let postmark = postmark_accepting("environment-token").await;
    let output = binary(&["doctor"])
        .env("POSTMARK_SERVER_TOKEN", "environment-token")
        .env("DEFAULT_SENDER_EMAIL", "sender@example.com")
        .env("DEFAULT_MESSAGE_STREAM", "outbound")
        .env("POSTMARK_API_BASE_URL", postmark.uri())
        .output()
        .await?;

    assert!(output.status.success(), "{}", stderr(&output));
    Ok(())
}

#[tokio::test]
async fn missing_settings_name_both_the_flag_and_the_variable() -> TestResult {
    let output = binary(&["doctor"]).output().await?;

    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("--server-token"), "{message}");
    assert!(message.contains("POSTMARK_SERVER_TOKEN"), "{message}");
    Ok(())
}

#[tokio::test]
async fn every_toolset_is_offered_by_default() -> TestResult {
    let postmark = MockServer::start().await;
    let args = with(base_args(&postmark), &["--server-token", "t", "--skip-verify"]);

    assert_eq!(
        tool_names(&args).await?,
        set(&[&READ_TOOLS, &SEND_TOOLS, &MANAGE_TOOLS])
    );
    Ok(())
}

#[tokio::test]
async fn toolsets_choose_exactly_which_tools_exist() -> TestResult {
    let postmark = MockServer::start().await;
    for (toolsets, expected) in [
        ("read", set(&[&READ_TOOLS])),
        ("read,send", set(&[&READ_TOOLS, &SEND_TOOLS])),
        ("send", set(&[&SEND_TOOLS])),
        ("manage", set(&[&MANAGE_TOOLS])),
        (" read , manage ", set(&[&READ_TOOLS, &MANAGE_TOOLS])),
    ] {
        let args = with(
            base_args(&postmark),
            &["--server-token", "t", "--skip-verify", "--toolsets", toolsets],
        );

        assert_eq!(tool_names(&args).await?, expected, "{toolsets}");
    }
    Ok(())
}

#[tokio::test]
async fn unusable_toolsets_stop_the_server() -> TestResult {
    let postmark = MockServer::start().await;
    for toolsets in ["", "read,write", "all", ","] {
        let args = with(
            base_args(&postmark),
            &["--server-token", "t", "--skip-verify", "--toolsets", toolsets],
        );

        let output = binary(&refs(&args)).output().await?;

        assert!(!output.status.success(), "{toolsets:?} was accepted");
        assert!(stderr(&output).contains("--toolsets"), "{}", stderr(&output));
    }
    Ok(())
}

#[tokio::test]
async fn every_other_setting_has_a_flag() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/server"))
        .and(header("X-Agent-Label", "flag label"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ID": 1, "Name": "Test"})))
        .expect(1)
        .mount(&postmark)
        .await;
    let log = std::env::temp_dir().join(format!("postmark-mcp-flag-log-{}", std::process::id()));
    let log_arg = log.to_string_lossy().into_owned();
    let args = with(
        base_args(&postmark),
        &[
            "--server-token",
            "t",
            "--agent-label",
            "flag label",
            "--recipient-allowlist",
            "@example.com",
            "--webhook-url-allowlist",
            "https://hooks.example.com/postmark",
            "--log-file",
            &log_arg,
            "--log-email-full",
        ],
    );

    let client = ().serve(TokioChildProcess::new(binary(&refs(&args)))?).await?;
    let refused = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("send_email").with_arguments(
                json!({"to": "stranger@other.com", "subject": "Hi", "text_body": "Hi"})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            ),
        )
        .await?;
    client.cancel().await?;
    let logged = fs::read_to_string(&log)?;
    fs::remove_file(&log)?;

    assert_eq!(
        refused
            .structured_content
            .as_ref()
            .and_then(|content| content.get("code")),
        Some(&json!("policy/recipient-not-allowed"))
    );
    assert!(
        logged.contains("stranger@other.com"),
        "--log-email-full was ignored: {logged}"
    );
    Ok(())
}

#[tokio::test]
async fn the_retired_read_only_variable_stops_the_server_rather_than_being_ignored() -> TestResult {
    let postmark = MockServer::start().await;
    let args = with(base_args(&postmark), &["--server-token", "t", "--skip-verify"]);
    for value in ["true", "false", ""] {
        let mut command = binary(&refs(&args));
        command.env("POSTMARK_READ_ONLY", value);

        let output = command.output().await?;

        assert!(!output.status.success(), "POSTMARK_READ_ONLY={value:?} was ignored");
        assert!(stderr(&output).contains("--toolsets"), "{}", stderr(&output));
    }
    Ok(())
}

#[tokio::test]
async fn an_empty_flag_is_refused_rather_than_hiding_its_variable() -> TestResult {
    let postmark = MockServer::start().await;
    for flag in [
        "--recipient-allowlist",
        "--webhook-url-allowlist",
        "--agent-label",
        "--log-file",
        "--server-token",
    ] {
        let mut args = with(base_args(&postmark), &["--skip-verify"]);
        if flag != "--server-token" {
            args = with(args, &["--server-token", "t"]);
        }
        let args = with(args, &[flag, ""]);
        let mut command = binary(&refs(&args));
        command.env("WEBHOOK_URL_ALLOWLIST", "https://hooks.example.com");
        command.env("POSTMARK_RECIPIENT_ALLOWLIST", "@example.com");
        command.env("POSTMARK_SERVER_TOKEN", "environment-token");

        let output = command.output().await?;

        assert!(!output.status.success(), "{flag} \"\" was accepted");
        assert!(stderr(&output).contains(flag), "{flag}: {}", stderr(&output));
    }
    Ok(())
}

#[tokio::test]
async fn a_mistyped_command_line_never_echoes_the_token() -> TestResult {
    for args in [
        vec!["--server-token abc-SECRET-123"],
        vec!["--server-token=", "abc-SECRET-123"],
        vec!["--server-token", "abc-SECRET-123", "--no-such-flag"],
        vec!["--server-token=abc-SECRET-123", "unexpected"],
    ] {
        let output = binary(&args).output().await?;

        assert!(!output.status.success(), "{args:?}");
        let printed = format!("{}{}", stderr(&output), String::from_utf8_lossy(&output.stdout));
        assert!(!printed.contains("SECRET"), "{args:?} printed: {printed}");
        assert!(
            printed.contains("--server-token"),
            "{args:?} gave no guidance: {printed}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn help_and_version_still_print() -> TestResult {
    let help = binary(&["--help"]).output().await?;
    let version = binary(&["--version"]).output().await?;

    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--toolsets"));
    assert!(version.status.success());
    Ok(())
}

// The updater checks every program in a release by running it with `--version`
// and reading the last word as the version, which must be bare, with no `v`.
#[tokio::test]
async fn version_ends_with_the_bare_version_the_updater_reads() -> TestResult {
    let version = binary(&["--version"]).output().await?;

    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        format!("postmark-mcp {}\n", env!("CARGO_PKG_VERSION"))
    );
    Ok(())
}

#[tokio::test]
async fn an_empty_token_file_is_reported_as_such() -> TestResult {
    let file = std::env::temp_dir().join(format!("postmark-mcp-empty-token-{}", std::process::id()));
    fs::write(&file, "\n")?;
    let postmark = MockServer::start().await;
    let file_arg = file.to_string_lossy().into_owned();
    let args = with(base_args(&postmark), &["--server-token-file", &file_arg, "doctor"]);

    let output = binary(&refs(&args)).output().await?;
    fs::remove_file(&file)?;

    assert!(!output.status.success());
    assert!(stderr(&output).contains("--server-token-file"), "{}", stderr(&output));
    assert!(stderr(&output).contains("empty"), "{}", stderr(&output));
    Ok(())
}

#[tokio::test]
async fn a_tool_outside_the_toolsets_cannot_be_called() -> TestResult {
    let postmark = MockServer::start().await;
    let args = with(
        base_args(&postmark),
        &["--server-token", "t", "--skip-verify", "--toolsets", "read,send"],
    );
    let client = ().serve(TokioChildProcess::new(binary(&refs(&args)))?).await?;

    let call = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("delete_template").with_arguments(
                json!({"template_id_or_alias": "welcome"})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            ),
        )
        .await;
    client.cancel().await?;

    assert!(call.is_err(), "delete_template was callable: {call:?}");
    assert!(postmark.received_requests().await.unwrap_or_default().is_empty());
    Ok(())
}

#[derive(Clone, Default)]
struct DiscoveryClient;

impl rmcp::ClientHandler for DiscoveryClient {}

#[tokio::test]
async fn toolsets_apply_to_a_2026_07_28_client_too() -> TestResult {
    use rmcp::{ClientLifecycleMode, ClientServiceExt};

    let postmark = MockServer::start().await;
    let args = with(
        base_args(&postmark),
        &["--server-token", "t", "--skip-verify", "--toolsets", "read"],
    );
    let client = DiscoveryClient
        .serve_with_lifecycle(
            TokioChildProcess::new(binary(&refs(&args)))?,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;

    let names: HashSet<String> = client
        .list_all_tools()
        .await?
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    client.cancel().await?;

    assert_eq!(names, set(&[&READ_TOOLS]));
    Ok(())
}
