//! Contract tests that hold for every tool in the catalogue.

mod support;

use serde_json::{Value, json};
use support::{TestResult, failure, list_tools};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DELIVERED_TOOLS: [&str; 24] = [
    "activate_bounce",
    "create_suppressions",
    "create_template",
    "create_webhook",
    "delete_suppressions",
    "delete_template",
    "delete_webhook",
    "diagnose_delivery",
    "edit_template",
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
    "send_batch",
    "send_batch_with_template",
    "send_email",
    "send_email_with_template",
    "validate_template",
];

/// Collects every place a schema allows `null`, or uses array-form `type` or a
/// union, which several MCP hosts mishandle.
fn awkward_schemas(path: &str, schema: &Value, found: &mut Vec<String>) {
    match schema {
        Value::Object(object) => {
            if object.contains_key("oneOf") || object.contains_key("anyOf") {
                found.push(path.to_owned());
            }
            if object.get("type").is_some_and(Value::is_array) || object.get("type") == Some(&json!("null")) {
                found.push(path.to_owned());
            }
            if object
                .get("enum")
                .and_then(Value::as_array)
                .is_some_and(|values| values.contains(&Value::Null))
            {
                found.push(path.to_owned());
            }
            for (key, value) in object {
                awkward_schemas(&format!("{path}/{key}"), value, found);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                awkward_schemas(&format!("{path}/{index}"), value, found);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn catalogue_holds_exactly_the_delivered_tools() -> TestResult {
    let tools = list_tools(&MockServer::start().await).await?;

    let mut names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, DELIVERED_TOOLS);
    Ok(())
}

#[tokio::test]
async fn every_tool_is_honestly_annotated_documented_and_closed_to_unknown_arguments() -> TestResult {
    for tool in list_tools(&MockServer::start().await).await? {
        let name = &tool.name;
        assert!(
            tool.description.as_deref().is_some_and(|text| !text.is_empty()),
            "{name}"
        );
        let annotations = tool.annotations.as_ref().ok_or("tool has no annotations")?;
        // (read only, destructive, idempotent), by the MCP specification's
        // definitions. Destructive means deleting or overwriting, so sending and
        // creating are not. Idempotent means repeating has no further effect, so
        // sending and creating are not, while deleting and overwriting are.
        let expected = match name.as_ref() {
            "send_email"
            | "send_email_with_template"
            | "send_batch"
            | "send_batch_with_template"
            | "create_template"
            | "create_webhook" => (false, false, false),
            "create_suppressions" => (false, false, true),
            // Reactivating changes an existing record, as lifting a hard bounce
            // suppression does, rather than adding one.
            "activate_bounce" | "edit_template" | "delete_template" | "delete_suppressions" | "delete_webhook" => {
                (false, true, true)
            }
            _ => (true, false, true),
        };
        let actual = (
            annotations.read_only_hint,
            annotations.destructive_hint,
            annotations.idempotent_hint,
        );
        assert_eq!(actual, (Some(expected.0), Some(expected.1), Some(expected.2)), "{name}");
        assert_eq!(annotations.open_world_hint, Some(true), "{name}");
        assert_eq!(
            tool.input_schema.get("additionalProperties"),
            Some(&json!(false)),
            "{name}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_output_schema_describes_results_and_failures() -> TestResult {
    for tool in list_tools(&MockServer::start().await).await? {
        let output = tool.output_schema.as_ref().ok_or("tool has no output schema")?;
        let properties = output.get("properties").and_then(Value::as_object);
        let properties = properties.ok_or("output schema has no properties")?;
        for field in ["code", "message", "remediation"] {
            assert!(properties.contains_key(field), "{} omits {field}", tool.name);
        }
        assert!(properties.len() > 3, "{} describes no result fields", tool.name);
        assert_eq!(
            output.get("required"),
            None,
            "{} would reject its own failures",
            tool.name
        );
    }
    Ok(())
}

#[tokio::test]
async fn no_schema_uses_a_form_that_mcp_hosts_mishandle() -> TestResult {
    for tool in list_tools(&MockServer::start().await).await? {
        let mut found = Vec::new();
        awkward_schemas("input", &Value::Object(tool.input_schema.as_ref().clone()), &mut found);
        if let Some(output) = &tool.output_schema {
            awkward_schemas("output", &Value::Object(output.as_ref().clone()), &mut found);
        }
        assert!(found.is_empty(), "{} has awkward schemas at {found:?}", tool.name);
    }
    Ok(())
}

#[tokio::test]
async fn every_output_definition_is_used() -> TestResult {
    for tool in list_tools(&MockServer::start().await).await? {
        let output = tool.output_schema.as_ref().ok_or("tool has no output schema")?;
        let Some(definitions) = output.get("$defs").and_then(Value::as_object) else {
            continue;
        };
        let serialised = serde_json::to_string(output.as_ref())?;
        for name in definitions.keys() {
            assert!(
                serialised.contains(&format!("#/$defs/{name}")),
                "{} leaves {name} unreferenced, so a field was overwritten when merging",
                tool.name
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn input_schemas_do_not_expose_internal_type_names() -> TestResult {
    for tool in list_tools(&MockServer::start().await).await? {
        assert_eq!(tool.input_schema.get("title"), None, "{}", tool.name);
        assert_eq!(tool.input_schema.get("description"), None, "{}", tool.name);
    }
    Ok(())
}

#[tokio::test]
async fn fixed_vocabularies_are_advertised_as_plain_enums() -> TestResult {
    let tools = list_tools(&MockServer::start().await).await?;
    let tool = tools
        .iter()
        .find(|tool| tool.name == "search_outbound_messages")
        .ok_or("tool is missing")?;

    let status = tool
        .input_schema
        .get("properties")
        .and_then(|fields| fields.get("status"));

    assert_eq!(
        status.and_then(|schema| schema.get("enum")),
        Some(&json!(["queued", "sent", "processed"]))
    );
    assert_eq!(status.and_then(|schema| schema.get("type")), Some(&json!("string")));
    Ok(())
}

#[tokio::test]
async fn postmark_failures_reach_every_kind_of_tool_as_structured_errors() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"ErrorCode": 701, "Message": "This message was not found."})),
        )
        .mount(&postmark)
        .await;

    for (tool, arguments) in [
        ("get_message_details", json!({"message_id": "missing"})),
        ("get_template", json!({"template_id_or_alias": "missing"})),
        ("search_bounces", json!({})),
        ("validate_template", json!({"subject": "x"})),
    ] {
        let result = failure(&postmark, tool, arguments).await?;

        assert_eq!(result.get("code"), Some(&json!("postmark/rejected")), "{tool}");
        assert!(
            result
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains("This message was not found.")),
            "{tool}"
        );
    }
    Ok(())
}
