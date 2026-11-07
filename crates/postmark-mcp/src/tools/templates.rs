//! Tools for email templates.

use postmark_mcp_core::{
    Page, PageRequest, ResourceId, Template, TemplateList, TemplateQuery, TemplateType, TemplateTypeFilter,
    TemplateValidation, ValidationResult, ValueError,
};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::JsonObject;
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use super::filter;
use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::ToolFailure;

/// Page size when a caller does not choose one.
const DEFAULT_COUNT: u32 = 100;

/// Arguments of `list_templates`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListTemplatesArgs {
    /// Number of templates to return, from 1 to 500. Defaults to 100.
    count: Option<u32>,
    /// Number of templates to skip. count plus offset cannot exceed 10,000.
    offset: Option<u32>,
    /// Restricts results to one kind of template. Defaults to `All`.
    template_type: Option<TemplateTypeFilter>,
    /// Restricts results to templates using this layout alias.
    layout_template: Option<String>,
}

impl TryFrom<ListTemplatesArgs> for TemplateQuery {
    type Error = ValueError;

    fn try_from(args: ListTemplatesArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            page: Page::try_from(PageRequest {
                count: args.count,
                offset: args.offset,
                default_count: DEFAULT_COUNT,
            })?,
            template_type: args.template_type,
            layout_template: filter("layout_template", args.layout_template)?,
        })
    }
}

/// A template's numeric ID or string alias, as a caller may send either.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IdOrAlias {
    Id(u64),
    Alias(String),
}

/// Arguments of `get_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GetTemplateArgs {
    /// Template ID or alias. A numeric ID may be sent as a number or a string.
    #[schemars(with = "String")]
    template_id_or_alias: IdOrAlias,
}

impl TryFrom<GetTemplateArgs> for ResourceId {
    type Error = ValueError;

    fn try_from(args: GetTemplateArgs) -> Result<Self, Self::Error> {
        let value = match args.template_id_or_alias {
            IdOrAlias::Id(id) => id.to_string(),
            IdOrAlias::Alias(alias) => alias,
        };
        Self::try_from(("template_id_or_alias", value))
    }
}

/// Arguments of `validate_template`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ValidateTemplateArgs {
    /// Subject to validate.
    subject: Option<String>,
    /// HTML body to validate.
    html_body: Option<String>,
    /// Plain text body to validate.
    text_body: Option<String>,
    /// Data to render the content with.
    test_render_model: Option<JsonObject>,
    /// Kind of template to validate as. Defaults to `Standard`.
    template_type: Option<TemplateType>,
    /// Alias of the layout to validate against.
    layout_template: Option<String>,
}

impl TryFrom<ValidateTemplateArgs> for TemplateValidation {
    type Error = ValueError;

    fn try_from(args: ValidateTemplateArgs) -> Result<Self, Self::Error> {
        let supplied = |text: &Option<String>| text.as_deref().is_some_and(|value| !value.is_empty());
        if !supplied(&args.subject) && !supplied(&args.html_body) && !supplied(&args.text_body) {
            return Err(ValueError::NothingToValidate);
        }
        Ok(Self {
            subject: args.subject,
            html_body: args.html_body,
            text_body: args.text_body,
            test_render_model: args.test_render_model,
            template_type: args.template_type,
            layout_template: args.layout_template,
        })
    }
}

#[tool_router(router = template_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "list_templates",
        description = "List email templates on this Postmark server, with name, ID, alias, type, and layout. Use total_count with count and offset to page through them, and get_template for a template's full content.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<ListTemplatesArgs>(),
        output_schema = fallible_output_schema_for::<TemplateList>()
    )]
    async fn list_templates(
        &self,
        Parameters(args): Parameters<ListTemplatesArgs>,
    ) -> Result<Json<TemplateList>, ToolFailure> {
        let query = TemplateQuery::try_from(args)?;
        Ok(Json(self.gateway.list_templates(&query).await?))
    }

    #[tool(
        name = "get_template",
        description = "Retrieve one Postmark template's full content, including subject, HTML body, and text body, by numeric ID or string alias.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<GetTemplateArgs>(),
        output_schema = fallible_output_schema_for::<Template>()
    )]
    async fn get_template(&self, Parameters(args): Parameters<GetTemplateArgs>) -> Result<Json<Template>, ToolFailure> {
        let id = ResourceId::try_from(args)?;
        Ok(Json(self.gateway.get_template(&id).await?))
    }

    #[tool(
        name = "validate_template",
        description = "Check template syntax without saving anything, and optionally render against test data. Supply at least one of subject, html_body, or text_body. Reports errors by line and position, and suggests a model from the variables found.",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<ValidateTemplateArgs>(),
        output_schema = fallible_output_schema_for::<ValidationResult>()
    )]
    async fn validate_template(
        &self,
        Parameters(args): Parameters<ValidateTemplateArgs>,
    ) -> Result<Json<ValidationResult>, ToolFailure> {
        let content = TemplateValidation::try_from(args)?;
        Ok(Json(self.gateway.validate_template(&content).await?))
    }
}
