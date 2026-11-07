# Roadmap

The goal is parity with the 24 tools of the official `postmark-mcp` server, delivered from least to most consequential, so the patterns are proven on read-only tools before anything can send mail. [`mcp-tools.md`](mcp-tools.md) lists each tool by stage.

## Delivered

- **Stage 0, foundation.** Workspace, CI, hardened transport, environment configuration, stdio server, `doctor`, and `get_server_info`.

## Planned

- **Stage 1, read-only tools.** Ten tools across templates, messages, bounces, suppressions, stats, and webhooks.
- **Stage 2, diagnostics.** `diagnose_delivery`, a composite that runs message, suppression, and bounce lookups concurrently, tolerates individual failures, and ends with a recommendation.
- **Stage 3, sending.** The four send tools, plus tool call logging with address masking (`LOG_FILE`, `LOG_EMAIL_FULL`), which must land first.
- **Stage 4, mutation.** Template, bounce, suppression, and webhook changes, including `WEBHOOK_URL_ALLOWLIST`.
- **Stage 5, beyond parity.** Candidates only: the asynchronous bulk API, account-level tools (domains, sender signatures) using `POSTMARK_ACCOUNT_TOKEN`, and streamable HTTP, which would need authentication.

## Decided

- **No Postmark SDK.** `postmark-rs` was assessed and removed. Its endpoints are declarative models, and 8 of the 12 read-side tools needed something they lacked, including `MessageEvents`, half the outbound search filters, and bounce result fields. Wire types live in `postmark-mcp-client` instead, and must be checked against the Postmark API documentation, ideally with a read-only pass using a real token.

## Open decisions

Each needs settling before its stage begins.

- **Sending guardrails (stage 3).** Whether to go beyond tool annotations with a recipient allowlist, or a read-only mode that leaves send and mutation tools out of the catalogue. An agent that reads email can be steered by that email's content, so this deserves a deliberate answer.
- **Tracking defaults (stage 3).** The official server always enables open and link tracking. Tracking has privacy implications, so whether to match that or leave it to the server's settings is a real choice.
