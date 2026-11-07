//! Shared harness for contract tests: a real MCP client, over an in-process
//! transport, talking to the real adapter and a local stand-in for Postmark.

// Each test binary compiles this module separately and uses only part of it.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use postmark_mcp::{PostmarkMcpServer, ServerDefaults};
use postmark_mcp_client::{ApiBaseUrl, HardenedTransport, PostmarkApi, TransportConfig};
use postmark_mcp_core::ServerToken;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use serde_json::{Map, Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub type TestError = Box<dyn Error + Send + Sync>;
pub type TestResult = Result<(), TestError>;

/// Default stream configured for every test, deliberately not `outbound`, so a
/// test can tell it from the stream Postmark itself would default to.
pub const DEFAULT_STREAM: &str = "default-stream";

pub fn server(postmark: &MockServer) -> Result<PostmarkMcpServer, TestError> {
    let transport = TransportConfig::new(
        ApiBaseUrl::try_from(postmark.uri().as_str())?,
        ServerToken::try_from("contract-test-token".to_owned())?,
    );
    let api = PostmarkApi::from(HardenedTransport::try_from(transport)?);
    let defaults = ServerDefaults {
        sender: "sender@example.com".to_owned().try_into()?,
        message_stream: DEFAULT_STREAM.to_owned().try_into()?,
    };
    Ok(PostmarkMcpServer::new(Arc::new(api), defaults))
}

pub async fn list_tools(postmark: &MockServer) -> Result<Vec<Tool>, TestError> {
    let server = server(postmark)?;
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        Ok::<(), TestError>(())
    });
    let mut client = ().serve(client_transport).await?;
    let tools = client.list_all_tools().await;
    client.close().await?;
    handle.await??;
    Ok(tools?)
}

pub async fn call_raw(
    postmark: &MockServer,
    name: &'static str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, TestError> {
    let server = server(postmark)?;
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        Ok::<(), TestError>(())
    });
    let mut client = ().serve(client_transport).await?;
    let mut params = CallToolRequestParams::new(name);
    if let Some(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    let result = client.call_tool(params).await;
    client.close().await?;
    handle.await??;
    Ok(result?)
}

pub async fn call(postmark: &MockServer, name: &'static str, arguments: Value) -> Result<CallToolResult, TestError> {
    let Value::Object(arguments) = arguments else {
        return Err("tool arguments must be an object".into());
    };
    call_raw(postmark, name, Some(arguments)).await
}

/// Calls a tool expecting success, and returns its structured content.
pub async fn success(postmark: &MockServer, name: &'static str, arguments: Value) -> Result<Value, TestError> {
    let result = call(postmark, name, arguments).await?;
    assert_eq!(result.is_error, Some(false), "{name} failed: {result:?}");
    let content = result
        .structured_content
        .ok_or_else(|| format!("{name} returned no structured content"))?;
    assert_advertised(postmark, name, &content).await?;
    Ok(content)
}

/// Asserts every top-level result field appears in the tool's output schema.
async fn assert_advertised(postmark: &MockServer, name: &str, content: &Value) -> TestResult {
    let tools = list_tools(postmark).await?;
    let tool = tools
        .iter()
        .find(|tool| tool.name == name)
        .ok_or("tool is not listed")?;
    let schema = tool.output_schema.as_ref().ok_or("tool has no output schema")?;
    let properties = schema.get("properties").and_then(Value::as_object);
    let properties = properties.ok_or("output schema has no properties")?;
    for field in content.as_object().ok_or("result is not an object")?.keys() {
        assert!(
            properties.contains_key(field),
            "{name} returned unadvertised field {field}"
        );
    }
    Ok(())
}

/// Calls a tool expecting a structured failure, and returns its content.
pub async fn failure(postmark: &MockServer, name: &'static str, arguments: Value) -> Result<Value, TestError> {
    let result = call(postmark, name, arguments).await?;
    assert_eq!(result.is_error, Some(true), "{name} succeeded: {result:?}");
    result
        .structured_content
        .ok_or_else(|| format!("{name} returned no structured failure").into())
}

/// Asserts a tool refuses these arguments without any request reaching Postmark.
pub async fn assert_refused(name: &'static str, arguments: Value) -> TestResult {
    let postmark = MockServer::start().await;

    let result = call(&postmark, name, arguments.clone()).await?;

    assert_eq!(result.is_error, Some(true), "{name} accepted {arguments}: {result:?}");
    // Refused by validation with a stable code, or by the SDK while decoding.
    match &result.structured_content {
        Some(failure) => assert_eq!(
            failure.get("code"),
            Some(&json!("invalid/argument")),
            "{name}: {arguments}"
        ),
        None => assert!(
            serde_json::to_string(&result.content)?.contains("failed to deserialize parameters"),
            "{name}: {arguments}: {result:?}"
        ),
    }
    let requests = postmark.received_requests().await.unwrap_or_default();
    assert!(requests.is_empty(), "{name} contacted Postmark for {arguments}");
    Ok(())
}

/// Starts a stand-in for Postmark answering one route.
pub async fn postmark_answering(verb: &str, route: &str, body: Value) -> MockServer {
    let postmark = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&postmark)
        .await;
    postmark
}

/// The query string of the only request received, as a sorted map, so that an
/// unexpected extra parameter fails the comparison.
pub async fn received_query(postmark: &MockServer) -> Result<BTreeMap<String, String>, TestError> {
    let requests = postmark.received_requests().await.unwrap_or_default();
    let [request] = requests.as_slice() else {
        return Err(format!("expected one request, received {}", requests.len()).into());
    };
    Ok(request.url.query_pairs().into_owned().collect())
}

pub fn query(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}
