# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once a first public release is tagged.

## [Unreleased]

### Changed

- `rmcp` 3.10.0, from 3.9.0. Protocol revisions are unchanged: 2026-07-28 through `server/discover`, and 2024-11-05 to 2025-11-25 through `initialize`. The release adds an optional skills feature this server does not enable, fixes header annotations on HTTP transports, and clarifies documentation, so the tool catalogue and stdio behaviour are unchanged.

## [0.6.1] - 2026-11-13

### Fixed

- A client that closes the connection without using the server, either at once or after only `server/discover`, now ends it with status 0 and an INFO line, rather than `ERROR connection closed: initialize request` and status 1. Hosts such as contextos-web start every configured server and close those a session did not use, so this was reported as a failure on every run.

## [0.6.0] - 2026-11-08

### Added

- `postmark-mcp-launcher`, which installs, updates, rolls back, and removes the server, built on `zentelligence/auto-updater`. It keeps each release in its own folder, verifies a signed manifest and checksum before activating one, and goes on `PATH` as `postmark-mcp`, so an MCP host's entry follows every update. `update check`, `update cleanup`, and `update diagnostics` report on and tidy the installation.
- Release archives now hold `bin/postmark-mcp`, `bin/postmark-mcp-launcher`, and `release.json`, with a signed `latest.json`. Each release is installed on every platform before it becomes the latest.

### Changed

- `postmark-mcp --version` prints the bare version, such as `postmark-mcp 0.6.0`, without a `v`, as the updater requires.
- Release tags are the bare version, such as `0.6.0`, with no `v`.

## [0.5.0] - 2026-11-07

### Added

- `create_template`, `edit_template`, `delete_template`, `activate_bounce`, `create_suppressions`, `delete_suppressions`, `create_webhook`, and `delete_webhook`, completing the official server's 24 tools.
- A command-line flag for every setting, such as `--default-sender`, since not every MCP host can set environment variables. Each still falls back to its environment variable, and a flag wins over its variable.
- `--server-token-file`, which reads the token from a file, keeping it out of the process list and the host's configuration.
- `--toolsets`, choosing which of the `read`, `send`, and `manage` tool groups exist. All three by default.
- `--webhook-url-allowlist`, with `WEBHOOK_URL_ALLOWLIST` as its fallback, matching path prefixes by whole segments.
- `change/outcome-unknown`, for a create that may have happened, and `policy/webhook-url-not-allowed`.

### Changed

- `activate_bounce` is annotated destructive, since reactivating changes an existing record.
- `edit_template` reads the template first and sends it back whole, with only the requested changes applied, rather than relying on how Postmark treats fields an edit leaves out.
- `create_webhook` uses the configured default stream when none is given, as every other tool does, where the official server leaves Postmark to choose `outbound`.
- A delete that may have happened despite failing is `change/outcome-unknown`, naming the tool to check with.
- Server token and API base URL remediations name the flags as well as the variables.

### Removed

- `POSTMARK_READ_ONLY`, from 0.4.0, replaced by `--toolsets read`. The server refuses to start while it is set, rather than ignore it and offer every tool.

### Security

- A flag given an empty value is refused, rather than hiding its environment variable, which for an allowlist would silently lift the restriction.
- A command line the parser cannot read, such as a flag and its value given as one argument, is reported without repeating its text whenever the token flag appears.
- Webhook URLs with an encoded slash or backslash in the path are refused, since the receiving server could decode them into a path the allowlist never saw.

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
