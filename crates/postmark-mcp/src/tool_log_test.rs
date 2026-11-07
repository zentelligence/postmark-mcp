use super::*;

fn arguments(value: Value) -> Option<JsonObject> {
    match value {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

fn masked(value: Value) -> Value {
    sanitised_arguments(arguments(value).as_ref(), false)
}

#[test]
fn addresses_are_masked_and_display_names_dropped() {
    assert_eq!(masked_address("user@example.com").as_deref(), Some("u**r@example.com"));
    assert_eq!(masked_address("ab@example.com").as_deref(), Some("a*b@example.com"));
    assert_eq!(masked_address("a@example.com").as_deref(), Some("a*@example.com"));
    assert_eq!(
        masked_address("Jo Bloggs <user@example.com>").as_deref(),
        Some("u**r@example.com")
    );
    assert_eq!(masked_address("not an address"), None);
}

#[test]
fn message_content_is_reduced_to_its_size() {
    assert_eq!(
        masked(json!({"subject": "Secret", "html_body": "<p>", "text_body": "ab"})),
        json!({"subject": "[6 chars]", "html_body": "[3 chars]", "text_body": "[2 chars]"})
    );
}

#[test]
fn content_under_any_other_name_is_reduced_to_its_size() {
    assert_eq!(
        masked(json!({
            "textBody": "secret",
            "HtmlBody": "<p>secret</p>",
            "templateModel": {"link": "secret"},
            "template_model": "secret",
            "subject": {"nested": "secret"},
            "attachments": [1, 2],
            "code": 123_456,
        })),
        json!({
            "textBody": "[6 chars]",
            "HtmlBody": "[13 chars]",
            "templateModel": "[1 keys]",
            "template_model": "[6 chars]",
            "subject": "[1 keys]",
            "attachments": "[2 items]",
            "code": "[number]",
        })
    );
}

#[test]
fn caller_chosen_names_are_cut_short() {
    let long = "x".repeat(100);
    let result = masked(json!({long.clone(): "value"}));

    assert!(
        result
            .as_object()
            .is_some_and(|object| object.keys().all(|key| key.chars().count() == 64))
    );
    assert_eq!(bounded_name(&long).chars().count(), 64);
}

#[test]
fn caller_data_is_reduced_to_a_count() {
    assert_eq!(
        masked(json!({"template_model": {"name": "Ada", "code": "123"}})),
        json!({"template_model": "[2 keys]"})
    );
}

#[test]
fn nested_messages_are_sanitised_too() {
    assert_eq!(
        masked(json!({"messages": [{"to": "user@example.com", "text_body": "Secret"}]})),
        json!({"messages": [{"to": "u**r@example.com", "text_body": "[6 chars]"}]})
    );
}

#[test]
fn partial_addresses_in_address_fields_are_hidden() {
    assert_eq!(
        masked(json!({"email_filter": "alice"})),
        json!({"email_filter": "[5 chars]"})
    );
}

#[test]
fn an_address_in_an_unexpected_field_is_still_masked() {
    assert_eq!(
        masked(json!({"tag": "user@example.com"})),
        json!({"tag": "u**r@example.com"})
    );
}

#[test]
fn secret_shaped_fields_are_reduced_to_their_size() {
    assert_eq!(
        masked(json!({"api_token": "abc", "PassWord": "def"})),
        json!({"api_token": "[3 chars]", "PassWord": "[3 chars]"})
    );
}

#[test]
fn long_strings_and_long_lists_are_cut_down() {
    let many: Vec<_> = (0..25).map(|index| format!("r{index}@example.com")).collect();
    let result = masked(json!({"tag": "x".repeat(301), "to": many}));

    assert_eq!(result.get("tag"), Some(&json!("[301 chars]")));
    assert_eq!(result.pointer("/to/0"), Some(&json!("r*0@example.com")));
    assert_eq!(result.pointer("/to/20"), Some(&json!("+5 more")));
    assert_eq!(result.pointer("/to/21"), None);
}

#[test]
fn ordinary_values_are_kept() {
    assert_eq!(
        masked(json!({"tag": "welcome", "count": 10, "inactive": false})),
        json!({"tag": "welcome", "count": 10, "inactive": false})
    );
}

#[test]
fn full_addresses_are_kept_only_on_request_and_content_stays_hidden() {
    let call = arguments(json!({"to": "user@example.com", "subject": "Secret"}));

    assert_eq!(
        sanitised_arguments(call.as_ref(), true),
        json!({"to": "user@example.com", "subject": "[6 chars]"})
    );
}
