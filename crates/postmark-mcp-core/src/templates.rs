//! Email templates.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::wire::{JsonObject, blank_as_none, null_as_default, vocabulary};
use crate::{FilterText, GatewayError, Page, ResourceId};

vocabulary! {
    /// Kind of template.
    TemplateType { Standard => "Standard", Layout => "Layout" }
}

vocabulary! {
    /// Which kinds of template to list.
    TemplateTypeFilter { All => "All", Standard => "Standard", Layout => "Layout" }
}

/// Which templates to list.
#[derive(Clone, Debug)]
pub struct TemplateQuery {
    /// The page of results to return.
    pub page: Page,
    /// Restricts results to one kind of template.
    pub template_type: Option<TemplateTypeFilter>,
    /// Restricts results to templates using this layout alias.
    pub layout_template: Option<FilterText>,
}

/// A page of templates.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct TemplateList {
    /// Number of templates on the server, across all pages.
    pub total_count: i64,
    /// Templates on this page.
    #[serde(default, deserialize_with = "null_as_default")]
    pub templates: Vec<TemplateSummary>,
}

/// A template in a list. Postmark documents no subject here, but one is passed
/// through if it is present. Use `get_template` for the full content.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct TemplateSummary {
    /// Numeric template identifier.
    pub template_id: i64,
    /// Name of the template.
    pub name: String,
    /// Optional string identifier.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub alias: Option<String>,
    /// Subject line, when Postmark includes it.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub subject: Option<String>,
    /// Whether the template can be used for sending.
    pub active: Option<bool>,
    /// `Standard` or `Layout`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub template_type: Option<String>,
    /// Alias of the layout this template uses.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub layout_template: Option<String>,
}

/// A template with its full content.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct Template {
    /// Numeric template identifier.
    pub template_id: i64,
    /// Name of the template.
    pub name: String,
    /// Subject line used when sending.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub subject: Option<String>,
    /// HTML content used when sending.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub html_body: Option<String>,
    /// Plain text content used when sending.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub text_body: Option<String>,
    /// Identifier of the server the template belongs to.
    pub associated_server_id: Option<i64>,
    /// Whether the template can be used for sending.
    pub active: Option<bool>,
    /// Optional string identifier.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub alias: Option<String>,
    /// `Standard` or `Layout`.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub template_type: Option<String>,
    /// Alias of the layout this template uses.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub layout_template: Option<String>,
}

/// Content to validate, in the shape Postmark takes it.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TemplateValidation {
    /// Subject to validate.
    pub subject: Option<String>,
    /// HTML body to validate.
    pub html_body: Option<String>,
    /// Plain text body to validate.
    pub text_body: Option<String>,
    /// Model to render the content with.
    pub test_render_model: Option<JsonObject>,
    /// Kind of template to validate as.
    pub template_type: Option<TemplateType>,
    /// Alias of the layout to validate against.
    pub layout_template: Option<String>,
}

/// Outcome of validating template content.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct ValidationResult {
    /// Whether all submitted content is parseable.
    pub all_content_is_valid: bool,
    /// Outcome for the subject, when one was submitted.
    pub subject: Option<ContentValidation>,
    /// Outcome for the HTML body, when one was submitted.
    pub html_body: Option<ContentValidation>,
    /// Outcome for the plain text body, when one was submitted.
    pub text_body: Option<ContentValidation>,
    /// Variables found in the content, merged with any test render model.
    pub suggested_template_model: Option<JsonObject>,
}

/// Outcome of validating one part of a template.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct ContentValidation {
    /// Whether this part is parseable and renderable.
    pub content_is_valid: bool,
    /// Problems found, if any.
    #[serde(default, deserialize_with = "null_as_default")]
    pub validation_errors: Vec<ValidationError>,
    /// This part rendered with the model.
    #[serde(default, deserialize_with = "blank_as_none")]
    pub rendered_content: Option<String>,
}

/// A problem in template content.
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all(serialize = "snake_case", deserialize = "PascalCase"))]
pub struct ValidationError {
    /// Description of the problem.
    pub message: String,
    /// Line of the problem, counting from 1.
    pub line: Option<i64>,
    /// Character position on the line, counting from 1.
    pub character_position: Option<i64>,
}

/// Reads and validates templates.
#[async_trait]
pub trait ReadsTemplates: Send + Sync {
    /// Lists templates on the server.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn list_templates(&self, query: &TemplateQuery) -> Result<TemplateList, GatewayError>;

    /// Fetches one template by numeric ID or alias.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn get_template(&self, id_or_alias: &ResourceId) -> Result<Template, GatewayError>;

    /// Validates template content without saving anything.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn validate_template(&self, content: &TemplateValidation) -> Result<ValidationResult, GatewayError>;
}
