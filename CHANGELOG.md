# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once a first public release is tagged.

## [Unreleased]

## [0.4.0] - 2026-11-07

### Added

- `send_email`, `send_email_with_template`, `send_batch`, and `send_batch_with_template`. Every message is validated, and every recipient checked against policy, before any request is made, so a batch is sent whole or not at all.
- `send/outcome-unknown`, for a send that failed in a way that leaves it unclear whether the email went out. Its remediation says to check before resending, never to retry.
- `POSTMARK_READ_ONLY`, which removes the send tools from the catalogue.
- `POSTMARK_RECIPIENT_ALLOWLIST`, which restricts recipients to listed addresses and domains, and `policy/recipient-not-allowed` for a refusal.
- Tool call logging to stderr, and to `LOG_FILE` if set, with message content reduced to its size and addresses masked unless `LOG_EMAIL_FULL` is `true`.
- Send tools are annotated as not idempotent, and, as MCP defines it, not destructive, since sending adds and overwrites nothing. A host's own settings decide whether it asks before a send.
- Tests proving MCP protocol revision 2026-07-28, through `server/discover`, and every earlier revision, through `initialize`.

### Security

- Open and link tracking are never set, so each Postmark server's own settings decide. The official server always turns both on.
- Addresses with commas, semicolons, or quotes, or with address characters such as `@` or `:` in a display name, are refused, since each can hide a recipient from the allowlist.
- The tool log keeps only arguments known to describe a request, and reduces everything else to its size, so content under an unexpected name, such as `textBody`, stays out.

## [0.3.0] - 2026-11-07

### Added

- `diagnose_delivery`, which checks the most recent message and its events, then suppressions and bounce history, for one exact address, and gives a verdict and recommendation. What could not be established is reported in `incomplete`, and never read as an empty result.
- `docs/upstream-postmark-mcp.md`, a reference for what the official server does.

## [0.2.0] - 2026-11-07

### Added

- Ten read-only tools: `list_templates`, `get_template`, `validate_template`, `search_outbound_messages`, `get_message_details`, `search_bounces`, `get_bounce_dump`, `list_suppressions`, `get_delivery_stats`, and `list_webhooks`.
- `invalid/argument`, the error code for arguments refused before any request is made.

### Changed

- `get_server_info` reports `smtp_api_activated` as optional, and omits unset fields.

### Security

- Webhook passwords, custom headers, message bodies, and `Content` on bounce records are never decoded, so they cannot reach a tool result. `get_bounce_dump` returns a bounce's raw source by design, which can quote the original message.
- Credentials, query strings, and fragments are removed from webhook URLs.
- Message event details are limited to an allowlist, with any value holding a URL dropped, because clicked links can hold live tokens.
- Decoding failures no longer quote values from the response.

## [0.1.0] - 2026-11-07

### Added

- Cargo workspace with `postmark-mcp-core`, `postmark-mcp-client`, and `postmark-mcp`.
- `HardenedTransport`, an HTTP client for the Postmark API that confines requests to one origin, refuses redirects, and bounds request time and response size.
- Environment configuration compatible with the official `postmark-mcp` server.
- MCP server over stdio, with the `get_server_info` tool, which removes credentials embedded in hook URLs.
- `postmark-mcp doctor`, which validates configuration and checks the token against Postmark.
