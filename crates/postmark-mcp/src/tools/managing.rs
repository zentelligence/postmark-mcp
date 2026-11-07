//! Tools that change account state: templates, bounces, suppressions, and
//! webhooks. They exist only when the `manage` toolset is on.

use postmark_mcp_core::{
    Bounce, BounceId, Deletion, MessageStreamId, NewTemplate, NewTemplateRequest, NewWebhook, ResourceId,
    SuppressionChange, SuppressionResults, TemplateChange, TemplateChangeRequest, TemplateSummary, TemplateType,
    ValueError, Webhook, WebhookId, WebhookTrigger, WebhookUrl,
};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::Deserialize;

use super::stream_or_default;
use super::templates::IdOrAlias;
use crate::schema::{fallible_output_schema_for, input_schema_for};
use crate::server::PostmarkMcpServer;
use crate::tool_error::{ChangeFailure, ToolFailure};

/// Arguments of `create_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTemplateArgs {
    /// Name of the template.
    name: String,
    /// Subject line. Required for a standard template, and not allowed for a layout.
    subject: Option<String>,
    /// HTML body. Supply this, `text_body`, or both.
    html_body: Option<String>,
    /// Plain text body. Supply this, `html_body`, or both.
    text_body: Option<String>,
    /// String identifier: letters, digits, `.`, `-`, and `_`, starting with a letter.
    alias: Option<String>,
    /// `Standard` or `Layout`. Defaults to `Standard`, and cannot be changed later.
    template_type: Option<TemplateType>,
    /// Alias of a layout for a standard template to use.
    layout_template: Option<String>,
}

impl From<CreateTemplateArgs> for NewTemplateRequest {
    fn from(args: CreateTemplateArgs) -> Self {
        Self {
            name: args.name,
            subject: args.subject,
            html_body: args.html_body,
            text_body: args.text_body,
            alias: args.alias,
            template_type: args.template_type,
            layout_template: args.layout_template,
        }
    }
}

/// Arguments of `edit_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditTemplateArgs {
    /// Template ID or alias. A numeric ID may be sent as a number or a string.
    #[schemars(with = "String")]
    template_id_or_alias: IdOrAlias,
    /// New name.
    name: Option<String>,
    /// New subject line.
    subject: Option<String>,
    /// New HTML body.
    html_body: Option<String>,
    /// New plain text body.
    text_body: Option<String>,
    /// New alias.
    alias: Option<String>,
    /// Alias of a layout to use instead.
    layout_template: Option<String>,
    /// Set to true to stop the template using any layout.
    detach_layout: Option<bool>,
}

/// Arguments of `delete_template`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteTemplateArgs {
    /// Template ID or alias. A numeric ID may be sent as a number or a string.
    #[schemars(with = "String")]
    template_id_or_alias: IdOrAlias,
}

/// Arguments of `activate_bounce`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivateBounceArgs {
    /// The bounce's ID, from `search_bounces` or `diagnose_delivery`.
    bounce_id: u64,
}

/// Arguments of `create_suppressions` and `delete_suppressions`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SuppressionArgs {
    /// Bare addresses, such as `name@example.com`, from 1 to 50.
    email_addresses: Vec<String>,
    /// Stream to change. Defaults to the configured default stream.
    message_stream: Option<String>,
}

/// Arguments of `create_webhook`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateWebhookArgs {
    /// HTTPS URL to send events to, with no username or password in it.
    url: String,
    /// Stream to report on. Defaults to the configured default stream.
    message_stream: Option<String>,
    /// Events to send: `Open`, `Click`, `Delivery`, `Bounce`, `SpamComplaint`, and `SubscriptionChange`.
    triggers: Vec<WebhookTrigger>,
}

/// Arguments of `delete_webhook`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteWebhookArgs {
    /// The webhook's ID, from `list_webhooks`.
    webhook_id: u64,
}

impl PostmarkMcpServer {
    fn suppression_change(&self, args: SuppressionArgs) -> Result<SuppressionChange, ValueError> {
        let stream: MessageStreamId = stream_or_default(args.message_stream, &self.defaults.message_stream)?;
        SuppressionChange::try_from((stream, args.email_addresses))
    }
}

#[tool_router(router = managing_tool_router, vis = "pub(crate)")]
impl PostmarkMcpServer {
    #[tool(
        name = "create_template",
        description = "Create an email template. A standard template needs a subject, and a layout must not have one. Supply at least one of html_body and text_body. If this fails with change/outcome-unknown, the template may exist, so check with list_templates before trying again.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        input_schema = input_schema_for::<CreateTemplateArgs>(),
        output_schema = fallible_output_schema_for::<TemplateSummary>()
    )]
    async fn create_template(
        &self,
        Parameters(args): Parameters<CreateTemplateArgs>,
    ) -> Result<Json<TemplateSummary>, ToolFailure> {
        let template = NewTemplate::try_from(NewTemplateRequest::from(args))?;
        let created = self
            .gateway
            .create_template(&template)
            .await
            .map_err(|error| ChangeFailure {
                error,
                check_with: "list_templates",
            })?;
        Ok(Json(created))
    }

    #[tool(
        name = "edit_template",
        description = "Change an existing template's name, subject, bodies, alias, or layout. The template is read first, then sent back whole with only the fields given changed, overwriting what is there, which cannot be undone. Use detach_layout to stop it using a layout.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<EditTemplateArgs>(),
        output_schema = fallible_output_schema_for::<TemplateSummary>()
    )]
    async fn edit_template(
        &self,
        Parameters(args): Parameters<EditTemplateArgs>,
    ) -> Result<Json<TemplateSummary>, ToolFailure> {
        let id = ResourceId::try_from(args.template_id_or_alias)?;
        let change = TemplateChange::try_from(TemplateChangeRequest {
            name: args.name,
            subject: args.subject,
            html_body: args.html_body,
            text_body: args.text_body,
            alias: args.alias,
            layout_template: args.layout_template,
            detach_layout: args.detach_layout,
        })?;
        Ok(Json(self.gateway.edit_template(&id, &change).await?))
    }

    #[tool(
        name = "delete_template",
        description = "Permanently delete a template by ID or alias. This cannot be undone. Postmark refuses to delete a layout that templates still use.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<DeleteTemplateArgs>(),
        output_schema = fallible_output_schema_for::<Deletion>()
    )]
    async fn delete_template(
        &self,
        Parameters(args): Parameters<DeleteTemplateArgs>,
    ) -> Result<Json<Deletion>, ToolFailure> {
        let id = ResourceId::try_from(args.template_id_or_alias)?;
        let deleted = self.gateway.delete_template(&id).await.map_err(|error| ChangeFailure {
            error,
            check_with: "list_templates",
        })?;
        Ok(Json(deleted))
    }

    #[tool(
        name = "activate_bounce",
        description = "Reactivate an address that a bounce deactivated, so it can receive mail again. Works only where the bounce's can_activate is true. Confirm the address is valid first, as sending to a bad address harms the server's reputation.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<ActivateBounceArgs>(),
        output_schema = fallible_output_schema_for::<Bounce>()
    )]
    async fn activate_bounce(
        &self,
        Parameters(args): Parameters<ActivateBounceArgs>,
    ) -> Result<Json<Bounce>, ToolFailure> {
        let id = BounceId::try_from(args.bounce_id)?;
        Ok(Json(self.gateway.activate_bounce(id).await?))
    }

    #[tool(
        name = "create_suppressions",
        description = "Suppress up to 50 addresses on a message stream, so they receive no mail on it. Postmark reports each address as Suppressed or Failed.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<SuppressionArgs>(),
        output_schema = fallible_output_schema_for::<SuppressionResults>()
    )]
    async fn create_suppressions(
        &self,
        Parameters(args): Parameters<SuppressionArgs>,
    ) -> Result<Json<SuppressionResults>, ToolFailure> {
        let change = self.suppression_change(args)?;
        Ok(Json(self.gateway.create_suppressions(&change).await?))
    }

    #[tool(
        name = "delete_suppressions",
        description = "Lift suppressions on up to 50 addresses on a message stream, so they can receive mail again. Postmark refuses to lift a spam complaint, and lifting a hard bounce reactivates it. Postmark reports each address as Deleted or Failed.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<SuppressionArgs>(),
        output_schema = fallible_output_schema_for::<SuppressionResults>()
    )]
    async fn delete_suppressions(
        &self,
        Parameters(args): Parameters<SuppressionArgs>,
    ) -> Result<Json<SuppressionResults>, ToolFailure> {
        let change = self.suppression_change(args)?;
        Ok(Json(self.gateway.delete_suppressions(&change).await?))
    }

    #[tool(
        name = "create_webhook",
        description = "Register a webhook that Postmark will send event data to until it is deleted. Only use a URL you control. Postmark checks the URL responds before saving it. If this fails with change/outcome-unknown, it may exist, so check with list_webhooks before trying again.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        input_schema = input_schema_for::<CreateWebhookArgs>(),
        output_schema = fallible_output_schema_for::<Webhook>()
    )]
    async fn create_webhook(
        &self,
        Parameters(args): Parameters<CreateWebhookArgs>,
    ) -> Result<Json<Webhook>, ToolFailure> {
        let destination = WebhookUrl::try_from(args.url)?;
        let stream = stream_or_default(args.message_stream, &self.defaults.message_stream)?;
        let webhook = NewWebhook::try_from((destination, Some(stream), args.triggers))?;
        if let Some(allowlist) = &self.policy.webhook_url_allowlist
            && !allowlist.permits(webhook.destination())
        {
            return Err(ToolFailure::webhook_url_not_allowed());
        }
        let created = self
            .gateway
            .create_webhook(&webhook)
            .await
            .map_err(|error| ChangeFailure {
                error,
                check_with: "list_webhooks",
            })?;
        Ok(Json(created))
    }

    #[tool(
        name = "delete_webhook",
        description = "Permanently delete a webhook by ID, so Postmark stops sending events to its URL. This cannot be undone.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true),
        input_schema = input_schema_for::<DeleteWebhookArgs>(),
        output_schema = fallible_output_schema_for::<Deletion>()
    )]
    async fn delete_webhook(
        &self,
        Parameters(args): Parameters<DeleteWebhookArgs>,
    ) -> Result<Json<Deletion>, ToolFailure> {
        let id = WebhookId::try_from(args.webhook_id)?;
        let deleted = self.gateway.delete_webhook(id).await.map_err(|error| ChangeFailure {
            error,
            check_with: "list_webhooks",
        })?;
        Ok(Json(deleted))
    }
}
