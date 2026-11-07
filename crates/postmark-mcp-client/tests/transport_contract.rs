//! Behaviour of the Postmark adapter against a real local HTTP server: what is
//! sent, what is refused, and how each failure surfaces through the domain port.

use std::error::Error;
use std::time::Duration;

use postmark_mcp_client::{ApiBaseUrl, HardenedTransport, PostmarkApi, TransportConfig};
use postmark_mcp_core::{AgentLabel, GatewayError, ReadsServer, ServerSummary, ServerToken};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

const TOKEN: &str = "test-server-token";

fn config(server: &MockServer) -> Result<TransportConfig, Box<dyn Error>> {
    Ok(TransportConfig::new(
        ApiBaseUrl::try_from(server.uri().as_str())?,
        ServerToken::try_from(TOKEN.to_owned())?,
    ))
}

fn api(config: TransportConfig) -> Result<PostmarkApi, Box<dyn Error>> {
    Ok(HardenedTransport::try_from(config)?.into())
}

fn server_body() -> serde_json::Value {
    json!({
        "ID": 42,
        "Name": "Production",
        "ApiTokens": ["secret-from-postmark"],
        "SmtpApiActivated": true,
    })
}

async fn respond_to_server_request(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/server"))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn failure_for(response: ResponseTemplate) -> Result<Option<GatewayError>, Box<dyn Error>> {
    let server = MockServer::start().await;
    respond_to_server_request(&server, response).await;
    Ok(api(config(&server)?)?.current_server().await.err())
}

#[tokio::test]
async fn current_server_authenticates_and_identifies_this_client() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/server"))
        .and(header("X-Postmark-Server-Token", TOKEN))
        .and(header("Accept", "application/json"))
        .and(header("X-Postmark-Client", "zentelligence-postmark-mcp"))
        .and(header("X-Postmark-Client-Version", env!("CARGO_PKG_VERSION")))
        .and(header("X-Agent-Label", "ci runner"))
        .respond_with(ResponseTemplate::new(200).set_body_json(server_body()))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = config(&server)?;
    config.agent_label = Some(AgentLabel::try_from("ci runner".to_owned())?);

    let summary = api(config)?.current_server().await?;

    assert_eq!(
        summary,
        ServerSummary {
            id: 42,
            name: "Production".to_owned(),
            smtp_api_activated: Some(true),
            ..ServerSummary::default()
        }
    );
    Ok(())
}

#[tokio::test]
async fn api_tokens_in_the_response_do_not_survive_into_the_domain() -> TestResult {
    let server = MockServer::start().await;
    respond_to_server_request(&server, ResponseTemplate::new(200).set_body_json(server_body())).await;

    let summary = api(config(&server)?)?.current_server().await?;

    assert!(!format!("{summary:?}").contains("secret-from-postmark"));
    Ok(())
}

async fn summary_of(body: serde_json::Value) -> Result<ServerSummary, Box<dyn Error>> {
    let server = MockServer::start().await;
    respond_to_server_request(&server, ResponseTemplate::new(200).set_body_json(body)).await;
    Ok(api(config(&server)?)?.current_server().await?)
}

#[tokio::test]
async fn server_configuration_is_mapped_in_full() -> TestResult {
    let summary = summary_of(json!({
        "ID": 42,
        "Name": "Production",
        "Color": "blue",
        "SmtpApiActivated": true,
        "InboundAddress": "abc123@inbound.postmarkapp.com",
        "InboundDomain": "inbound.example.com",
        "TrackOpens": true,
        "TrackLinks": "HtmlAndText",
        "PostFirstOpenOnly": false,
        "BounceHookUrl": "https://hooks.example.com/bounce",
        "OpenHookUrl": "https://hooks.example.com/open",
        "DeliveryHookUrl": "https://hooks.example.com/delivery",
        "ClickHookUrl": "https://hooks.example.com/click",
        "InboundHookUrl": "https://hooks.example.com/inbound",
    }))
    .await?;

    assert_eq!(
        summary,
        ServerSummary {
            id: 42,
            name: "Production".to_owned(),
            color: Some("blue".to_owned()),
            smtp_api_activated: Some(true),
            inbound_address: Some("abc123@inbound.postmarkapp.com".to_owned()),
            inbound_domain: Some("inbound.example.com".to_owned()),
            track_opens: Some(true),
            track_links: Some("HtmlAndText".to_owned()),
            post_first_open_only: Some(false),
            bounce_hook_url: Some("https://hooks.example.com/bounce".to_owned()),
            open_hook_url: Some("https://hooks.example.com/open".to_owned()),
            delivery_hook_url: Some("https://hooks.example.com/delivery".to_owned()),
            click_hook_url: Some("https://hooks.example.com/click".to_owned()),
            inbound_hook_url: Some("https://hooks.example.com/inbound".to_owned()),
        }
    );
    Ok(())
}

#[tokio::test]
async fn blank_and_absent_values_are_reported_as_unset() -> TestResult {
    let summary = summary_of(json!({
        "ID": 42,
        "Name": "Production",
        "Color": "",
        "InboundDomain": null,
        "BounceHookUrl": "",
    }))
    .await?;

    assert_eq!(
        summary,
        ServerSummary {
            id: 42,
            name: "Production".to_owned(),
            ..ServerSummary::default()
        }
    );
    Ok(())
}

#[tokio::test]
async fn credentials_embedded_in_hook_urls_are_removed() -> TestResult {
    let summary = summary_of(json!({
        "ID": 42,
        "Name": "Production",
        "BounceHookUrl": "https://hookuser:hookpass@hooks.example.com/bounce?source=postmark",
        "InboundHookUrl": "https://onlyuser@hooks.example.com/inbound",
    }))
    .await?;

    assert_eq!(
        summary.bounce_hook_url.as_deref(),
        Some("https://hooks.example.com/bounce?source=postmark")
    );
    assert_eq!(
        summary.inbound_hook_url.as_deref(),
        Some("https://hooks.example.com/inbound")
    );
    Ok(())
}

#[tokio::test]
async fn hook_urls_that_cannot_be_parsed_are_withheld_rather_than_relayed() -> TestResult {
    let summary = summary_of(json!({
        "ID": 42,
        "Name": "Production",
        "ClickHookUrl": "//hookuser:hookpass@hooks.example.com",
    }))
    .await?;

    assert_eq!(summary.click_hook_url, None);
    Ok(())
}

#[tokio::test]
async fn redirects_are_refused_and_the_token_never_reaches_the_target() -> TestResult {
    let server = MockServer::start().await;
    let attacker = MockServer::start().await;
    respond_to_server_request(
        &server,
        ResponseTemplate::new(302).insert_header("Location", format!("{}/server", attacker.uri())),
    )
    .await;

    let result = api(config(&server)?)?.current_server().await;

    assert_eq!(result, Err(GatewayError::RedirectRefused { status: 302 }));
    let received = attacker.received_requests().await.unwrap_or_default();
    assert!(received.is_empty(), "the redirect target received a request");
    Ok(())
}

#[tokio::test]
async fn slow_responses_time_out() -> TestResult {
    let server = MockServer::start().await;
    respond_to_server_request(&server, ResponseTemplate::new(200).set_delay(Duration::from_secs(30))).await;
    let mut config = config(&server)?;
    config.timeout = Duration::from_millis(50);

    assert_eq!(api(config)?.current_server().await, Err(GatewayError::Timeout));
    Ok(())
}

#[tokio::test]
async fn oversized_responses_are_refused() -> TestResult {
    let server = MockServer::start().await;
    respond_to_server_request(&server, ResponseTemplate::new(200).set_body_string("x".repeat(2048))).await;
    let mut config = config(&server)?;
    config.max_response_bytes = 1024;

    assert_eq!(
        api(config)?.current_server().await,
        Err(GatewayError::ResponseTooLarge { limit_bytes: 1024 })
    );
    Ok(())
}

#[tokio::test]
async fn oversized_responses_that_declare_no_length_are_refused() -> TestResult {
    // A raw socket, since a mock server always declares a `Content-Length`.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await?;
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request).await?;
        let chunk = "x".repeat(1024);
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n400\r\n{chunk}\r\n400\r\n{chunk}\r\n0\r\n\r\n"
        );
        socket.write_all(response.as_bytes()).await
    });
    let mut config = TransportConfig::new(
        ApiBaseUrl::try_from(format!("http://{address}").as_str())?,
        ServerToken::try_from(TOKEN.to_owned())?,
    );
    config.max_response_bytes = 1500;

    let result = api(config)?.current_server().await;

    assert_eq!(result, Err(GatewayError::ResponseTooLarge { limit_bytes: 1500 }));
    peer.await??;
    Ok(())
}

#[tokio::test]
async fn unreachable_hosts_are_reported_without_the_token() -> TestResult {
    // Bound then released, so the port is closed. Dropping a `MockServer` would
    // not do, because wiremock returns it to a pool still listening.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    drop(listener);
    let config = TransportConfig::new(
        ApiBaseUrl::try_from(format!("http://{address}").as_str())?,
        ServerToken::try_from(TOKEN.to_owned())?,
    );

    let error = api(config)?.current_server().await.err();

    assert!(matches!(error, Some(GatewayError::Unreachable { .. })), "{error:?}");
    assert!(!format!("{error:?}").contains(TOKEN));
    Ok(())
}

#[tokio::test]
async fn unauthorised_responses_are_classified_by_status() -> TestResult {
    let response = ResponseTemplate::new(401).set_body_json(json!({"ErrorCode": 10, "Message": "Bad token"}));

    assert_eq!(failure_for(response).await?, Some(GatewayError::Unauthorised));
    Ok(())
}

#[tokio::test]
async fn bad_token_error_code_is_classified_as_unauthorised_on_any_status() -> TestResult {
    let response = ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 10, "Message": "Bad token"}));

    assert_eq!(failure_for(response).await?, Some(GatewayError::Unauthorised));
    Ok(())
}

#[tokio::test]
async fn rate_limit_responses_are_classified() -> TestResult {
    assert_eq!(
        failure_for(ResponseTemplate::new(429)).await?,
        Some(GatewayError::RateLimited)
    );
    Ok(())
}

#[tokio::test]
async fn server_errors_are_classified_as_unavailable() -> TestResult {
    assert_eq!(
        failure_for(ResponseTemplate::new(503)).await?,
        Some(GatewayError::Unavailable { status: 503 })
    );
    Ok(())
}

#[tokio::test]
async fn rejections_preserve_postmarks_code_and_message() -> TestResult {
    let response = ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 300, "Message": "Invalid request"}));

    assert_eq!(
        failure_for(response).await?,
        Some(GatewayError::Rejected {
            status: 422,
            error_code: Some(300),
            message: "Invalid request".to_owned(),
        })
    );
    Ok(())
}

#[tokio::test]
async fn relayed_rejection_messages_are_bounded() -> TestResult {
    let response = ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 300, "Message": "x".repeat(5000)}));

    let error = failure_for(response).await?;

    assert!(matches!(error, Some(GatewayError::Rejected { message, .. }) if message.chars().count() == 500));
    Ok(())
}

#[tokio::test]
async fn undecodable_success_responses_are_reported() -> TestResult {
    let error = failure_for(ResponseTemplate::new(200).set_body_string("not json")).await?;

    assert!(matches!(error, Some(GatewayError::InvalidResponse { .. })), "{error:?}");
    Ok(())
}
