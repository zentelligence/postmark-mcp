//! End-to-end tests of the real binary: spawned over stdio, configured only
//! through its environment, and talking to a local stand-in for Postmark.

use std::error::Error;
use std::process::Output;

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ProtocolVersion};
use rmcp::transport::TokioChildProcess;
use serde_json::{Map, json};
use tokio::process::Command;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

const TOKEN: &str = "stdio-test-token";

fn binary(base_url: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_postmark-mcp"));
    command
        .env_clear()
        .env("POSTMARK_SERVER_TOKEN", TOKEN)
        .env("DEFAULT_SENDER_EMAIL", "sender@example.com")
        .env("DEFAULT_MESSAGE_STREAM", "outbound")
        .env("POSTMARK_API_BASE_URL", base_url);
    command
}

async fn postmark(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/server"))
        .and(header("X-Postmark-Server-Token", TOKEN))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn healthy_postmark() -> MockServer {
    postmark(
        200,
        json!({"ID": 42, "Name": "Production", "ApiTokens": ["leaked-api-token"], "SmtpApiActivated": true}),
    )
    .await
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[tokio::test]
async fn serves_tools_over_stdio_without_exposing_api_tokens() -> TestResult {
    let postmark = healthy_postmark().await;
    let transport = TokioChildProcess::new(binary(&postmark.uri()))?;

    let client = ().serve(transport).await?;

    let info = client.peer_info().ok_or("server sent no initialise result")?;
    let implementation = info
        .server_info
        .as_ref()
        .ok_or("server sent no implementation details")?;
    assert_eq!(implementation.name, "postmark-mcp");
    assert_eq!(implementation.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(info.protocol_version, ProtocolVersion::LATEST_WITH_INITIALIZE);

    let result = client
        .call_tool(CallToolRequestParams::new("get_server_info").with_arguments(Map::new()))
        .await?;
    client.cancel().await?;

    assert_eq!(result.is_error, Some(false));
    let serialised = serde_json::to_string(&result)?;
    assert!(serialised.contains("Production"));
    assert!(!serialised.contains("leaked-api-token"));
    Ok(())
}

#[tokio::test]
async fn refuses_to_start_when_postmark_rejects_the_token() -> TestResult {
    let postmark = postmark(401, json!({"ErrorCode": 10, "Message": "Bad token"})).await;

    let output = binary(&postmark.uri()).output().await?;

    assert!(!output.status.success());
    assert!(stderr(&output).contains("Postmark rejected the server token"));
    assert!(!stderr(&output).contains(TOKEN));
    assert!(stdout(&output).is_empty(), "stdout is reserved for MCP traffic");
    Ok(())
}

#[tokio::test]
async fn skip_verify_starts_without_contacting_postmark() -> TestResult {
    let postmark = MockServer::start().await;
    let mut command = binary(&postmark.uri());
    command.env("POSTMARK_SKIP_VERIFY", "true");

    let client = ().serve(TokioChildProcess::new(command)?).await?;
    client.cancel().await?;

    assert!(postmark.received_requests().await.unwrap_or_default().is_empty());
    Ok(())
}

#[tokio::test]
async fn refuses_to_start_with_missing_configuration() -> TestResult {
    let output = Command::new(env!("CARGO_BIN_EXE_postmark-mcp"))
        .env_clear()
        .output()
        .await?;

    assert!(!output.status.success());
    assert!(stderr(&output).contains("POSTMARK_SERVER_TOKEN is not set"));
    Ok(())
}

#[tokio::test]
async fn doctor_reports_the_server_a_valid_token_belongs_to() -> TestResult {
    let postmark = healthy_postmark().await;

    let output = binary(&postmark.uri()).arg("doctor").output().await?;

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Token accepted for Postmark server Production (42)"));
    assert!(!stdout(&output).contains(TOKEN));
    Ok(())
}

#[tokio::test]
async fn doctor_fails_with_remediation_when_the_token_is_rejected() -> TestResult {
    let postmark = postmark(401, json!({"ErrorCode": 10, "Message": "Bad token"})).await;

    let output = binary(&postmark.uri()).arg("doctor").output().await?;

    assert!(!output.status.success());
    assert!(stderr(&output).contains("POSTMARK_SERVER_TOKEN"));
    assert!(!stderr(&output).contains(TOKEN));
    Ok(())
}
