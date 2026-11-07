# Configuration

All configuration comes from environment variables, normally set in the `env` block of your MCP host's entry for this server. There is no configuration file, and `.env` files are not loaded. An empty value counts as unset.

Names match the official `postmark-mcp` server, so an existing host entry works unchanged.

## Required

| Variable | Description |
| --- | --- |
| `POSTMARK_SERVER_TOKEN` | Server API token, from your server's API Tokens tab. Scopes every tool to that one server. |
| `DEFAULT_SENDER_EMAIL` | Sender used when a tool call does not supply one, as `sender@example.com` or `Name <sender@example.com>`. Must have a verified Sender Signature. |
| `DEFAULT_MESSAGE_STREAM` | Message stream ID used when a tool call does not supply one, typically `outbound`. Applies to message, bounce, and suppression tools. Statistics and webhooks cover all streams unless one is requested. |

## Optional

| Variable | Default | Description |
| --- | --- | --- |
| `AGENT_LABEL` | unset | Label sent as `X-Agent-Label` on every request, to identify this instance. At most 128 visible ASCII characters. |
| `POSTMARK_API_BASE_URL` | `https://api.postmarkapp.com/` | API origin. Must be a bare origin using `https`, or `http` on a loopback address for local testing. The server token is sent here. |
| `POSTMARK_SKIP_VERIFY` | `false` | When `true`, skips checking the token against Postmark at startup. Must be `true` or `false`. |

### Sending

| Variable | Default | Description |
| --- | --- | --- |
| `POSTMARK_READ_ONLY` | `false` | When `true`, the four send tools are left out of the catalogue entirely, so they cannot be listed or called. Must be `true` or `false`. |
| `POSTMARK_RECIPIENT_ALLOWLIST` | unset | Comma-separated addresses, such as `name@example.com`, and whole domains, such as `@example.com`, that mail may be sent to. Every `to`, `cc`, and `bcc` recipient must match, or nothing is sent. A domain entry does not cover its subdomains. Matching ignores case. |

The allowlist is for deployments whose recipients are known in advance: a staging or development server that must only reach your team, an agent that only sends within your own domain, or a demonstration. It cannot protect sending to the public, such as customers or workshop attendees, since those addresses cannot be listed ahead of time. Leave it unset there.

Unset or empty, the allowlist places no restriction. Check it is set as you intend: like every variable here, an empty value counts as unset, so a host that templates `POSTMARK_RECIPIENT_ALLOWLIST=` with no value leaves sending unrestricted.

A malformed entry stops the server at startup, rather than being skipped.

### Logging

| Variable | Default | Description |
| --- | --- | --- |
| `LOG_FILE` | unset | File that each tool call is appended to, as one JSON line, as well as stderr. It is created with owner-only permissions on Unix. The server will not start if it cannot open the file. Never point it at standard output, which carries MCP traffic. |
| `LOG_EMAIL_FULL` | `false` | When `true`, addresses are logged in full rather than masked. Must be `true` or `false`. |

Each entry records the tool, `ok` or `error`, the failure code if any, the duration, and the arguments. Only arguments that describe a request, such as `tag`, `count`, or `template_id`, are logged as given. Addresses are masked as `u**r@example.com`, with any display name dropped, and every other argument, including subjects, bodies, template models, and anything under a name the server does not expect, is reduced to its size.

## Not yet supported

These `postmark-mcp` variables arrive with the tools that need them. See the [roadmap](roadmap.md).

| Variable | Arrives with |
| --- | --- |
| `WEBHOOK_URL_ALLOWLIST` | `create_webhook` |

## Startup checks

The server validates every variable, then calls Postmark to confirm the token, and exits with a non-zero status and an explanation on stderr if either fails. `postmark-mcp doctor` runs the same checks without serving.

## Logging

Diagnostics go to stderr, at the level set by `--log-level`. Keep the default of `info` in production: at `debug` and below, the MCP SDK logs protocol messages, which include tool arguments such as email content, without the masking applied to tool call entries.
