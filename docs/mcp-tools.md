# MCP Tools

Tools are named in `snake_case`, as the direct translation of the official server's names. The server speaks MCP over stdio, using `rmcp` 3.9.0, which supports protocol revisions up to 2026-07-28.

## Results and errors

Every tool returns structured content. A failure is a tool result with `isError` set, and this shape:

| Field | Description |
| --- | --- |
| `code` | Stable, machine-readable code. |
| `message` | What went wrong. |
| `remediation` | What to do next. |

| Code | Meaning |
| --- | --- |
| `postmark/unauthorised` | Postmark did not accept the server token. |
| `postmark/rate-limited` | Postmark's rate limit was exceeded. |
| `postmark/rejected` | Postmark refused the request. The message carries its explanation. |
| `postmark/unavailable` | Postmark reported a failure on its side. |
| `postmark/timeout` | The request did not complete in time. |
| `postmark/unreachable` | No response was received. |
| `postmark/redirect-refused` | The API redirected, which is never followed. |
| `postmark/response-too-large` | The response exceeded the size limit. |
| `postmark/invalid-response` | A successful response could not be understood. |
| `server/invalid-request` | The request could not be built, because of a defect in this server. |

## Available

### `get_server_info`

Reports the configuration of the Postmark server the token belongs to, and the default sender and message stream. Read-only, and takes no arguments. Optional fields are omitted when unset.

| Result field | Description |
| --- | --- |
| `server_id` | Postmark's numeric server identifier. |
| `name` | Display name of the server. |
| `color` | Colour label in Postmark's interface. Optional. |
| `smtp_api_activated` | Whether the SMTP API is activated. Optional. |
| `inbound_address` | Address that receives inbound mail. Optional. |
| `inbound_domain` | Custom domain for inbound mail. Optional. |
| `track_opens` | Whether open tracking is on by default. Optional. |
| `track_links` | Default link tracking mode. Optional. |
| `post_first_open_only` | Whether only the first open triggers the open webhook. Optional. |
| `bounce_hook_url`, `open_hook_url`, `delivery_hook_url`, `click_hook_url`, `inbound_hook_url` | Legacy server-level webhooks. Optional. |
| `default_sender` | Sender used when a tool call does not supply one. |
| `default_message_stream` | Message stream used when a tool call does not supply one. |

Unlike the official server, this tool removes basic auth credentials from hook URLs, and withholds a hook URL it cannot parse.

## Planned

| Tool | Official name | Stage |
| --- | --- | --- |
| `list_templates` | `listTemplates` | 1 |
| `get_template` | `getTemplate` | 1 |
| `validate_template` | `validateTemplate` | 1 |
| `search_outbound_messages` | `searchOutboundMessages` | 1 |
| `get_message_details` | `getMessageDetails` | 1 |
| `search_bounces` | `searchBounces` | 1 |
| `get_bounce_dump` | `getBounceDump` | 1 |
| `list_suppressions` | `listSuppressions` | 1 |
| `get_delivery_stats` | `getDeliveryStats` | 1 |
| `list_webhooks` | `listWebhooks` | 1 |
| `diagnose_delivery` | `diagnoseDelivery` | 2 |
| `send_email` | `sendEmail` | 3 |
| `send_email_with_template` | `sendEmailWithTemplate` | 3 |
| `send_batch` | `sendBatch` | 3 |
| `send_batch_with_template` | `sendBatchWithTemplate` | 3 |
| `create_template` | `createTemplate` | 4 |
| `edit_template` | `editTemplate` | 4 |
| `delete_template` | `deleteTemplate` | 4 |
| `activate_bounce` | `activateBounce` | 4 |
| `create_suppressions` | `createSuppressions` | 4 |
| `delete_suppressions` | `deleteSuppressions` | 4 |
| `create_webhook` | `createWebhook` | 4 |
| `delete_webhook` | `deleteWebhook` | 4 |
