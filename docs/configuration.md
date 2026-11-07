# Configuration

All configuration comes from environment variables, normally set in the `env` block of your MCP host's entry for this server. There is no configuration file, and `.env` files are not loaded. An empty value counts as unset.

Names match the official `postmark-mcp` server, so an existing host entry works unchanged.

## Required

| Variable | Description |
| --- | --- |
| `POSTMARK_SERVER_TOKEN` | Server API token, from your server's API Tokens tab. Scopes every tool to that one server. |
| `DEFAULT_SENDER_EMAIL` | Sender used when a tool call does not supply one, as `sender@example.com` or `Name <sender@example.com>`. Must have a verified Sender Signature. |
| `DEFAULT_MESSAGE_STREAM` | Message stream ID used when a tool call does not supply one, typically `outbound`. |

## Optional

| Variable | Default | Description |
| --- | --- | --- |
| `AGENT_LABEL` | unset | Label sent as `X-Agent-Label` on every request, to identify this instance. At most 128 visible ASCII characters. |
| `POSTMARK_API_BASE_URL` | `https://api.postmarkapp.com/` | API origin. Must be a bare origin using `https`, or `http` on a loopback address for local testing. The server token is sent here. |
| `POSTMARK_SKIP_VERIFY` | `false` | When `true`, skips checking the token against Postmark at startup. Must be `true` or `false`. |

## Not yet supported

These `postmark-mcp` variables arrive with the tools that need them. See the [roadmap](roadmap.md).

| Variable | Arrives with |
| --- | --- |
| `WEBHOOK_URL_ALLOWLIST` | `create_webhook` |
| `LOG_FILE`, `LOG_EMAIL_FULL` | Tool call logging |

## Startup checks

The server validates every variable, then calls Postmark to confirm the token, and exits with a non-zero status and an explanation on stderr if either fails. `postmark-mcp doctor` runs the same checks without serving.

## Logging

Diagnostics go to stderr, at the level set by `--log-level`. Keep the default of `info` in production: at `debug` and below, the MCP SDK logs protocol messages, which can include tool arguments such as email content.
