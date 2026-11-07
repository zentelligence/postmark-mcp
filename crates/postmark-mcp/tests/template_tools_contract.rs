//! Contract tests for `list_templates`, `get_template`, and `validate_template`.

mod support;

use serde_json::json;
use support::{TestResult, assert_refused, failure, postmark_answering, query, received_query, success};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn list_templates_defaults_to_the_first_hundred() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/templates",
        json!({
            "TotalCount": 2,
            "Templates": [
                {"Active": true, "TemplateId": 1, "Name": "Welcome", "Alias": "welcome", "TemplateType": "Standard", "LayoutTemplate": "base"},
                {"Active": true, "TemplateId": 2, "Name": "Base", "Alias": null, "TemplateType": "Layout", "LayoutTemplate": null},
            ],
        }),
    )
    .await;

    let result = success(&postmark, "list_templates", json!({})).await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[("count", "100"), ("offset", "0")])
    );
    assert_eq!(
        result,
        json!({
            "total_count": 2,
            "templates": [
                {"template_id": 1, "name": "Welcome", "alias": "welcome", "active": true, "template_type": "Standard", "layout_template": "base"},
                {"template_id": 2, "name": "Base", "active": true, "template_type": "Layout"},
            ],
        })
    );
    Ok(())
}

#[tokio::test]
async fn list_templates_supports_paging_and_filters() -> TestResult {
    let postmark = postmark_answering("GET", "/templates", json!({"TotalCount": 0, "Templates": []})).await;

    success(
        &postmark,
        "list_templates",
        json!({"count": 10, "offset": 20, "template_type": "Layout", "layout_template": "base"}),
    )
    .await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[
            ("count", "10"),
            ("offset", "20"),
            ("TemplateType", "Layout"),
            ("LayoutTemplate", "base"),
        ])
    );
    Ok(())
}

#[tokio::test]
async fn list_templates_refuses_out_of_range_paging() -> TestResult {
    assert_refused("list_templates", json!({"count": 0})).await?;
    assert_refused("list_templates", json!({"count": 501})).await?;
    assert_refused("list_templates", json!({"count": 500, "offset": 9501})).await
}

fn template_body() -> serde_json::Value {
    json!({
        "TemplateId": 7,
        "Name": "Welcome",
        "Subject": "Hello {{name}}",
        "HtmlBody": "<p>Hello</p>",
        "TextBody": "Hello",
        "AssociatedServerId": 42,
        "Active": true,
        "Alias": "welcome",
        "TemplateType": "Standard",
        "LayoutTemplate": null,
    })
}

#[tokio::test]
async fn get_template_accepts_a_numeric_id() -> TestResult {
    let postmark = postmark_answering("GET", "/templates/7", template_body()).await;

    let result = success(&postmark, "get_template", json!({"template_id_or_alias": 7})).await?;

    assert_eq!(
        result,
        json!({
            "template_id": 7,
            "name": "Welcome",
            "subject": "Hello {{name}}",
            "html_body": "<p>Hello</p>",
            "text_body": "Hello",
            "associated_server_id": 42,
            "active": true,
            "alias": "welcome",
            "template_type": "Standard",
        })
    );
    Ok(())
}

#[tokio::test]
async fn get_template_accepts_an_alias() -> TestResult {
    let postmark = postmark_answering("GET", "/templates/welcome", template_body()).await;

    let result = success(&postmark, "get_template", json!({"template_id_or_alias": "welcome"})).await?;

    assert_eq!(result.get("template_id"), Some(&json!(7)));
    Ok(())
}

#[tokio::test]
async fn get_template_encodes_an_alias_so_it_stays_one_path_segment() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(template_body()))
        .mount(&postmark)
        .await;

    success(&postmark, "get_template", json!({"template_id_or_alias": "a/b?c=d"})).await?;

    let requests = postmark.received_requests().await.unwrap_or_default();
    let request = requests.first().ok_or("no request received")?;
    assert_eq!(request.url.path(), "/templates/a%2Fb%3Fc=d");
    assert_eq!(request.url.query(), None);
    Ok(())
}

#[tokio::test]
async fn get_template_refuses_values_that_would_not_name_a_template() -> TestResult {
    for value in [json!(""), json!(".."), json!("."), json!(true), json!(1.5), json!(-1)] {
        assert_refused("get_template", json!({"template_id_or_alias": value})).await?;
    }
    assert_refused("get_template", json!({})).await
}

#[tokio::test]
async fn validate_template_sends_the_content_and_reports_each_part() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/templates/validate"))
        .and(body_json(json!({
            "Subject": "Hi {{name}}",
            "HtmlBody": "<p>{{#broken}}</p>",
            "TestRenderModel": {"name": "Ada"},
            "TemplateType": "Standard",
            "LayoutTemplate": "base",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "AllContentIsValid": false,
            "Subject": {"ContentIsValid": true, "ValidationErrors": [], "RenderedContent": "Hi Ada"},
            "HtmlBody": {
                "ContentIsValid": false,
                "ValidationErrors": [{"Message": "Unclosed block", "Line": 1, "CharacterPosition": 4}],
                "RenderedContent": null,
            },
            "TextBody": null,
            "SuggestedTemplateModel": {"name": "Ada"},
        })))
        .expect(1)
        .mount(&postmark)
        .await;

    let result = success(
        &postmark,
        "validate_template",
        json!({
            "subject": "Hi {{name}}",
            "html_body": "<p>{{#broken}}</p>",
            "test_render_model": {"name": "Ada"},
            "template_type": "Standard",
            "layout_template": "base",
        }),
    )
    .await?;

    assert_eq!(
        result,
        json!({
            "all_content_is_valid": false,
            "subject": {"content_is_valid": true, "validation_errors": [], "rendered_content": "Hi Ada"},
            "html_body": {
                "content_is_valid": false,
                "validation_errors": [{"message": "Unclosed block", "line": 1, "character_position": 4}],
            },
            "suggested_template_model": {"name": "Ada"},
        })
    );
    Ok(())
}

#[tokio::test]
async fn validate_template_accepts_each_part_on_its_own() -> TestResult {
    for field in ["subject", "html_body", "text_body"] {
        let postmark = postmark_answering("POST", "/templates/validate", json!({"AllContentIsValid": true})).await;

        let result = success(&postmark, "validate_template", json!({field: "content"})).await?;

        assert_eq!(result.get("all_content_is_valid"), Some(&json!(true)), "{field}");
    }
    Ok(())
}

#[tokio::test]
async fn validate_template_needs_something_to_validate() -> TestResult {
    assert_refused("validate_template", json!({})).await?;
    assert_refused("validate_template", json!({"template_type": "Standard"})).await?;
    assert_refused("validate_template", json!({"subject": ""})).await
}

#[tokio::test]
async fn invalid_arguments_are_reported_with_a_stable_code_and_remediation() -> TestResult {
    let postmark = MockServer::start().await;

    let result = failure(&postmark, "validate_template", json!({})).await?;

    assert_eq!(result.get("code"), Some(&json!("invalid/argument")));
    assert!(result.get("remediation").is_some());
    Ok(())
}
