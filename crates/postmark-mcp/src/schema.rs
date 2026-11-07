//! JSON Schema for tools, in a form MCP hosts handle reliably.
//!
//! An `Option` field normally produces `"type": ["string", "null"]`. Several
//! hosts do not recognise that array form: some reject the tool, and some send
//! the argument as a string. Optional fields are expressed by omission instead,
//! so the schemas here state only the field's own type.

use std::sync::Arc;

use rmcp::handler::server::tool::{schema_for_output, schema_for_type};
use rmcp::model::JsonObject;
use rmcp::schemars;
use serde_json::{Value, json};

use crate::tool_error::ToolFailure;

/// Removes nullability from a schema and everything nested in it.
fn drop_nullability(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            if let Some(Value::Array(types)) = object.get_mut("type") {
                types.retain(|kind| kind != "null");
                if let [only] = types.as_slice() {
                    let only = only.clone();
                    object.insert("type".to_owned(), only);
                }
            }
            if let Some(Value::Array(values)) = object.get_mut("enum") {
                values.retain(|value| !value.is_null());
            }
            if let Some(Value::Array(variants)) = object.get_mut("anyOf") {
                variants.retain(|variant| variant.get("type").is_none_or(|kind| kind != "null"));
                if let [only] = variants.as_slice()
                    && let Value::Object(only) = only.clone()
                {
                    object.remove("anyOf");
                    object.extend(only);
                }
            }
            object.values_mut().for_each(drop_nullability);
        }
        Value::Array(items) => items.iter_mut().for_each(drop_nullability),
        _ => {}
    }
}

/// Input schema for a tool taking `T`.
pub(crate) fn input_schema_for<T: schemars::JsonSchema + std::any::Any>() -> Arc<JsonObject> {
    let mut schema = schema_for_type::<T>().as_ref().clone();
    // These describe the Rust type, not the tool, whose description is its own.
    schema.remove("title");
    schema.remove("description");
    let mut schema = Value::Object(schema);
    drop_nullability(&mut schema);
    match schema {
        Value::Object(object) => Arc::new(object),
        _ => Arc::default(),
    }
}

/// Output schema for a tool returning `T` on success and [`ToolFailure`]
/// otherwise.
///
/// Both shapes are merged into one flat object, with `required` dropped, rather
/// than expressed as `oneOf`: a result of either shape must validate, and
/// several MCP hosts reject a tool whose output schema has no top-level
/// `properties`.
pub(crate) fn fallible_output_schema_for<T: schemars::JsonSchema + std::any::Any>() -> Arc<JsonObject> {
    let mut properties = JsonObject::new();
    let mut definitions = JsonObject::new();
    for schema in [schema_for_output::<T>(), schema_for_output::<ToolFailure>()] {
        if let Some(fields) = schema.get("properties").and_then(Value::as_object) {
            properties.extend(fields.clone());
        }
        if let Some(fields) = schema.get("$defs").and_then(Value::as_object) {
            definitions.extend(fields.clone());
        }
    }

    let mut merged = JsonObject::new();
    merged.insert(
        "$schema".to_owned(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    if !definitions.is_empty() {
        merged.insert("$defs".to_owned(), Value::Object(definitions));
    }
    merged.insert("type".to_owned(), json!("object"));
    merged.insert("properties".to_owned(), Value::Object(properties));

    let mut schema = Value::Object(merged);
    drop_nullability(&mut schema);
    match schema {
        Value::Object(object) => Arc::new(object),
        _ => Arc::default(),
    }
}
