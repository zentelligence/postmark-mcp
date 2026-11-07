//! MCP contract tests: the advertised catalogue, result shapes, and stable
//! errors, exercised through a real MCP client over an in-process transport.

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use postmark_mcp::{PostmarkMcpServer, ServerDefaults};
use postmark_mcp_core::{GatewayError, ReadsServer, ServerSummary};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use serde_json::{Map, Value, json};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

struct FakeGateway(Result<ServerSummary, GatewayError>);

#[async_trait]
impl ReadsServer for FakeGateway {
    async fn current_server(&self) -> Result<ServerSummary, GatewayError> {
        self.0.clone()
    }
}

fn server(gateway: FakeGateway) -> Result<PostmarkMcpServer, Box<dyn Error + Send + Sync>> {
    let defaults = ServerDefaults {
        sender: "sender@example.com".to_owned().try_into()?,
        message_stream: "outbound".to_owned().try_into()?,
    };
    Ok(PostmarkMcpServer::new(Arc::new(gateway), defaults))
}

fn healthy() -> FakeGateway {
    FakeGateway(Ok(ServerSummary {
        id: 42,
        name: "Production".to_owned(),
        smtp_api_activated: Some(true),
        track_links: Some("HtmlAndText".to_owned()),
        bounce_hook_url: Some("https://hooks.example.com/bounce".to_owned()),
        ..ServerSummary::default()
    }))
}

async fn list_tools(server: PostmarkMcpServer) -> Result<Vec<Tool>, Box<dyn Error + Send + Sync>> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    });
    let mut client = ().serve(client_transport).await?;
    let tools = client.list_all_tools().await;
    client.close().await?;
    handle.await??;
    Ok(tools?)
}

async fn call_tool(
    server: PostmarkMcpServer,
    name: &'static str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, Box<dyn Error + Send + Sync>> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let handle = tokio::spawn(async move {
        server.serve(server_transport).await?.waiting().await?;
        Ok::<(), Box<dyn Error + Send + Sync>>(())
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

#[tokio::test]
async fn get_server_info_is_advertised_as_read_only_with_a_closed_input_schema() -> TestResult {
    let tools = list_tools(server(healthy())?).await?;

    let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(names, ["get_server_info"]);

    let tool = tools.first().ok_or("catalogue is empty")?;
    assert!(tool.description.as_deref().is_some_and(|text| !text.is_empty()));
    let annotations = tool.annotations.as_ref().ok_or("tool has no annotations")?;
    assert_eq!(annotations.read_only_hint, Some(true));
    assert_eq!(annotations.destructive_hint, Some(false));
    assert_eq!(tool.input_schema.get("additionalProperties"), Some(&json!(false)));

    let output = tool.output_schema.as_ref().ok_or("tool has no output schema")?;
    let properties = output.get("properties").and_then(Value::as_object);
    let properties = properties.ok_or("output schema has no properties")?;
    for field in [
        "server_id",
        "name",
        "color",
        "bounce_hook_url",
        "default_sender",
        "code",
        "message",
        "remediation",
    ] {
        assert!(properties.contains_key(field), "output schema omits {field}");
    }
    Ok(())
}

#[tokio::test]
async fn output_schema_advertises_each_field_with_a_single_type() -> TestResult {
    let tools = list_tools(server(healthy())?).await?;

    let tool = tools.first().ok_or("catalogue is empty")?;
    let output = tool.output_schema.as_ref().ok_or("tool has no output schema")?;
    let properties = output.get("properties").and_then(Value::as_object);
    for (name, schema) in properties.ok_or("output schema has no properties")? {
        assert!(
            schema.get("type").is_some_and(Value::is_string),
            "{name} has no single type, which several MCP hosts mishandle: {schema}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn get_server_info_returns_the_server_and_effective_defaults() -> TestResult {
    let result = call_tool(server(healthy())?, "get_server_info", Some(Map::new())).await?;

    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        result.structured_content,
        Some(json!({
            "server_id": 42,
            "name": "Production",
            "smtp_api_activated": true,
            "track_links": "HtmlAndText",
            "bounce_hook_url": "https://hooks.example.com/bounce",
            "default_sender": "sender@example.com",
            "default_message_stream": "outbound",
        }))
    );
    Ok(())
}

#[tokio::test]
async fn get_server_info_accepts_a_call_with_no_arguments_object() -> TestResult {
    let result = call_tool(server(healthy())?, "get_server_info", None).await?;

    assert_eq!(result.is_error, Some(false));
    Ok(())
}

#[tokio::test]
async fn get_server_info_rejects_unknown_arguments() -> TestResult {
    let mut arguments = Map::new();
    arguments.insert("unexpected".to_owned(), json!(true));

    let result = call_tool(server(healthy())?, "get_server_info", Some(arguments)).await?;

    assert_eq!(result.is_error, Some(true), "unknown argument was accepted: {result:?}");
    assert!(serde_json::to_string(&result)?.contains("unknown field `unexpected`"));
    Ok(())
}

#[tokio::test]
async fn gateway_failures_become_structured_tool_errors() -> TestResult {
    let gateway = FakeGateway(Err(GatewayError::Unauthorised));

    let result = call_tool(server(gateway)?, "get_server_info", Some(Map::new())).await?;

    assert_eq!(result.is_error, Some(true));
    let failure = result.structured_content.ok_or("failure has no structured content")?;
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
