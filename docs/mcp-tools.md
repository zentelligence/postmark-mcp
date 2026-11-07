# MCP Tools

Tools are named in `snake_case`, as the direct translation of the official server's names. The server speaks MCP over stdio, using `rmcp` 3.9.0.

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
| `invalid/argument` | An argument was refused before any request was made. |

An argument of the wrong type, an unknown argument, or a value outside a fixed list is rejected by the MCP SDK, as a tool result with `isError` set and no `code`.

Every tool is read-only, and unset result fields are omitted. All dates are `YYYY-MM-DD`, inclusive, and Postmark interprets them in Eastern Time.

## Available

### `get_server_info`

Reports the configuration of the Postmark server the token belongs to, and the default sender and message stream. Takes no arguments.

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

Unlike the official server, this tool removes credentials, query strings, and fragments from hook URLs, and withholds a hook URL it cannot parse. A secret in a URL's path cannot be detected, and is not removed.

### `list_templates`

Lists templates, with `total_count` for paging. Unlike the official tool, it can page past the first 100.

| Argument | Description |
| --- | --- |
| `count` | From 1 to 500. Defaults to 100. Postmark documents these limits for messages and bounces, and this server applies them here too. |
| `offset` | Results to skip. `count` plus `offset` cannot exceed 10,000. |
| `template_type` | `All`, `Standard`, or `Layout`. |
| `layout_template` | Layout alias. |

### `get_template`

Returns a template's full content, by `template_id_or_alias`. This is advertised as a string, and a numeric ID may be sent as a number or a string.

### `validate_template`

Checks template syntax without saving anything, and reports errors by line and position. Supply at least one of `subject`, `html_body`, or `text_body`, plus optionally `test_render_model`, `template_type`, and `layout_template`.

### `search_outbound_messages`

| Argument | Description |
| --- | --- |
| `recipient`, `from_email`, `tag`, `subject` | Text filters. |
| `status` | `queued`, `sent`, or `processed`. |
| `message_stream` | Defaults to `DEFAULT_MESSAGE_STREAM`. |
| `from_date`, `to_date` | Date range. |
| `count` | From 1 to 500. Defaults to 50. |
| `offset` | Results to skip. `count` plus `offset` cannot exceed 10,000. |

### `get_message_details`

Returns a message's metadata and events, by `message_id`, in the order Postmark reports them, which is not always chronological. Message bodies are not included. Event `details` are limited to delivery, bounce, and subscription fields: clicked link URLs can hold live tokens, and geolocation and client addresses are personal data, so neither is returned, which differs from the official server.

### `search_bounces`

Takes `type`, `inactive`, `email_filter`, `tag`, `message_id`, `message_stream`, `from_date`, `to_date`, `count`, and `offset`, with the same paging and stream defaults as `search_outbound_messages`. Original message content is not included.

### `get_bounce_dump`

Returns the raw source of a bounce, by `bounce_id`. This is what the remote server sent back, which can quote the original message, including its body. `available` is false once Postmark has discarded it, after 30 days.

### `list_suppressions`

Takes `message_stream`, which defaults to `DEFAULT_MESSAGE_STREAM`, plus `suppression_reason`, `origin`, `email_address`, `from_date`, and `to_date`. Postmark's API offers no paging here. The list is eventually consistent.

### `get_delivery_stats`

Takes `stat`, plus `tag`, `message_stream`, `from_date`, and `to_date`. With no `message_stream`, all streams are covered, and with no dates, all time.

| `stat` | Reports |
| --- | --- |
| `summary` | The default. Overview totals, plus `rates`. |
| `overview` | Overview totals. |
| `sent`, `bounces`, `spam`, `tracked`, `opens`, `clicks` | Counts of each. |
| `open_platforms`, `open_clients`, `open_read_times` | Opens by platform, client, and read time. |
| `click_browsers`, `click_platforms`, `click_location` | Clicks by browser, platform, and location. |

`totals` keeps Postmark's names, since many are dynamic. `rates` are percentages to one decimal place, and a rate missing a figure, or with a denominator of zero, is omitted rather than reported as zero. Open rate uses emails with open tracking as its denominator, not all tracked emails as the official server does, because an email with only link tracking cannot register an open. The per-day series Postmark returns is not included.

`open_read_times` is not in Postmark's API reference. The official server uses it, so it is included, but it is unverified.

### `list_webhooks`

Lists webhooks, optionally for one `message_stream`, with each one's `id`, `url`, `message_stream`, `status`, and enabled `triggers`. HTTP auth and custom headers are never included, and credentials, query strings, and fragments are removed from URLs. A URL's path is kept, so treat these URLs as sensitive: some services put the secret there.

## Planned

| Tool | Official name | Stage |
| --- | --- | --- |
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
