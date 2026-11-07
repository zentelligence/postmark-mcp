# MCP Tools

Tools are named in `snake_case`, as the direct translation of the official server's names. The server speaks MCP over stdio, using `rmcp` 3.9.0. It supports protocol revision 2026-07-28, where a client starts with `server/discover`, and every earlier revision from 2024-11-05, where it starts with `initialize`. Tests prove each one through the real binary.

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
| `send/outcome-unknown` | A send failed in a way that leaves it unclear whether the email went out. Check with `search_outbound_messages` before resending. |
| `policy/recipient-not-allowed` | A recipient is outside `POSTMARK_RECIPIENT_ALLOWLIST`, so nothing was sent. |

An argument of the wrong type, an unknown argument, or a value outside a fixed list is rejected by the MCP SDK, as a tool result with `isError` set and no `code`.

Unset result fields are omitted. All dates are `YYYY-MM-DD`, inclusive, and Postmark interprets them in Eastern Time.

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

### `diagnose_delivery`

Investigates why an email may not have reached `recipient`, which must be one bare address. It covers one message: the most recent one found, or the one named by `message_id`.

| Argument | Description |
| --- | --- |
| `recipient` | The address to investigate. |
| `message_id` | A specific message. Cannot be combined with dates. |
| `message_stream` | Defaults to `DEFAULT_MESSAGE_STREAM`. A given message's own stream takes precedence, and the result's `message_stream` shows which was used. |
| `from_date`, `to_date` | Restrict the message search and the bounce history, not the suppression check. With no dates, all retained history is searched. |

| Result field | Description |
| --- | --- |
| `recipient`, `message_stream` | What was investigated. |
| `verdict` | The conclusion, listed below. |
| `recommendation` | What to do next. |
| `suppression` | The suppression on this address on this stream. |
| `investigated_message` | The message investigated, with its events for this recipient. |
| `recent_bounces` | Up to ten bounce records of this address, the investigated message's first, including ones that are not delivery failures. |
| `reactivatable_bounce_id` | For a hard bounce suppression, the bounce that deactivated the address, if it can be reactivated. |
| `incomplete` | What could not be established, each with `lookup`, `code`, `message`, and `remediation`. If this is not empty, treat absent data as unknown. |

`lookup` is `message`, `message_details`, `suppressions`, `bounces`, or `message_bounces`. As well as the codes above, `code` can be:

| Code | Meaning |
| --- | --- |
| `diagnosis/recipient-mismatch` | The given message was not sent to this recipient, so it is not reported. |
| `diagnosis/recipient-unconfirmed` | Postmark did not say who a message was sent to. |
| `diagnosis/results-truncated` | More messages or bounces matched than were read, and those read were for other addresses. |
| `diagnosis/stream-unconfirmed` | Postmark gave no usable stream for a message named by `message_id`, so the requested stream was checked instead. |

| `verdict` | Meaning |
| --- | --- |
| `suppressed_spam_complaint`, `suppressed_hard_bounce`, `suppressed_manual`, `suppressed_other` | The address is suppressed on this stream. This takes priority, because it blocks future mail. |
| `sandboxed` | The message was sandboxed, so it never left Postmark. |
| `bounced` | A bounce event, or a bounce record for this message, shows delivery failed. |
| `delivered` | A delivery, open, or click shows the receiving server accepted the message for this recipient. |
| `delayed` | Delivery hit a temporary failure, and nothing shows it was delivered since. |
| `queued` | The message has not been sent yet. |
| `sent_no_delivery_event` | The message was sent, with no delivery recorded for this recipient, and every lookup succeeded. |
| `bounce_history_only` | No message was found, but this address has bounce records. |
| `no_recent_message` | No message to this address was found, and every lookup succeeded. |
| `inconclusive` | The answer could not be established. See `incomplete`. |

Postmark's bounce log holds more than failures, so its record types are classified. `HardBounce`, `SoftBounce`, `BadEmailAddress`, `Blocked`, `SMTPApiError`, `DMARCPolicy`, and `TemplateRenderingFailed` are failures. `Transient` and `DnsError` are delays. Out of office replies logged as `AutoResponder`, and the other notices that do not mean delivery failed, are not failures, and can overrule a `Bounced` event linked to them. Types that do not say either way, such as `Unknown`, are not evidence on their own, and do not overrule a `Bounced` event.

A verdict resting on evidence that was found stands even if another lookup failed, so one other than `inconclusive` can still come with entries in `incomplete`. A verdict resting on nothing being found needs those lookups to succeed: `delayed` and `queued` need the bounce lookups, and `sent_no_delivery_event` and `no_recent_message` need every lookup.

This differs from the official server in several ways, all to avoid a confident wrong answer:

- A failed lookup is listed in `incomplete`, and never read as an empty result. If every lookup fails, the tool fails.
- Postmark's address filters may match partially, so results are checked against the exact address, ignoring case.
- Only events for this recipient count, so delivery to another recipient of the same message does not. An event naming nobody counts only on a message with one recipient.
- Bounce records are classified, and attributed to a message only by its ID.
- The stream applies to every lookup, and a given message's own stream takes precedence.
- There is no default date window, so an older message is still found.
- Recommendations name only tools this server has.

## Sending

These four tools are left out entirely when `POSTMARK_READ_ONLY` is `true`. They are annotated as not read-only and not idempotent, since calling one twice sends twice. They are not marked destructive, because in MCP's terms sending is additive: it deletes and overwrites nothing. Whether a host asks before a send is up to its own settings, which an automated deployment can set to allow the send tools outright.

Common to all four:

- Every message is validated, and every recipient checked against `POSTMARK_RECIPIENT_ALLOWLIST`, before any request is made. In a batch, one problem refuses the whole batch, nothing is sent, and the failure names the message, counting from 0.
- `to`, `cc`, and `bcc` are advertised as arrays of addresses. A single address given as a string is also accepted. An address is `name@example.com` or `Name <name@example.com>`. Commas, semicolons, and quotes are refused, as are characters with meaning in an address header, such as `@`, `:`, and brackets, within a display name, because each can hide a recipient.
- A message has at most 50 recipients across `to`, `cc`, and `bcc`. A subject is at most 2,000 characters, and a tag at most 1,000.
- `from` defaults to `DEFAULT_SENDER_EMAIL`, and the stream is always `DEFAULT_MESSAGE_STREAM`.
- Open and link tracking are not set, so the Postmark server's own settings decide. The official server always turns both on.
- A send is never retried. If Postmark may have received it, the failure is `send/outcome-unknown`.

### `send_email`

Takes `to`, `subject`, at least one of `text_body` and `html_body`, and optionally `from`, `cc`, `bcc`, `reply_to`, and `tag`. Returns `message_id`, `to`, and `submitted_at`.

### `send_email_with_template`

Takes `to`, exactly one of `template_id` and `template_alias`, `template_model`, and optionally `from`, `cc`, `bcc`, `reply_to`, and `tag`. Returns the same as `send_email`.

### `send_batch`

Takes `messages`, from 1 to 500, each with the arguments of `send_email`.

### `send_batch_with_template`

Takes exactly one of `template_id` and `template_alias`, optional shared `from` and `tag`, and `recipients`, from 1 to 500, each with `to`, `template_model`, and optionally `from`, `cc`, `bcc`, `reply_to`, and `tag`. A recipient's `from` or `tag` overrides the shared one.

### Batch results

Postmark accepts or refuses each message in a batch on its own, so a batch succeeds as a call even if some messages fail.

| Field | Description |
| --- | --- |
| `total` | Messages in the request. |
| `succeeded`, `failed` | Messages Postmark accepted and refused. |
| `unaccounted` | Messages Postmark gave no clear answer for. Treat these as unknown, and check before resending. |
| `results` | Each answer in request order, with `index`, `to`, `error_code`, `message`, and, if accepted, `message_id` and `submitted_at`. |

## Planned


| Tool | Official name | Stage |
| --- | --- | --- |
| `create_template` | `createTemplate` | 4 |
| `edit_template` | `editTemplate` | 4 |
| `delete_template` | `deleteTemplate` | 4 |
| `activate_bounce` | `activateBounce` | 4 |
| `create_suppressions` | `createSuppressions` | 4 |
| `delete_suppressions` | `deleteSuppressions` | 4 |
| `create_webhook` | `createWebhook` | 4 |
| `delete_webhook` | `deleteWebhook` | 4 |
