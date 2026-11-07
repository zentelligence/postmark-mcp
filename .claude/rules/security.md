# Security and Data-Integrity Rules

Assume all tool parameters, API responses, and email content are untrusted. An agent's tool call may be driven by text in an email it just read, so treat the tool surface as reachable by an attacker.

## Tokens

- The server token reaches only the configured API origin. `ApiBaseUrl` is the sole construction boundary, and `ApiBaseUrl::endpoint` builds every request URL.
- Never follow a redirect: custom headers are carried across them, including cross-origin.
- `ServerToken` wraps a secret, and is exposed only where the header is built. Mark the header value sensitive.
- `ApiTokens` and webhook `HttpAuth` and `HttpHeaders` in Postmark responses are never modelled, so they are discarded while decoding.
- URLs from Postmark can carry secrets. `sanitised_url` in `wire.rs` removes credentials, query, and fragment while decoding, and withholds a URL it cannot parse. A secret in the path cannot be detected, so treat a webhook URL as sensitive regardless.
- Clicked links can hold live tokens, such as password resets. `Details` on message events is decoded through an allowlist, and any value holding a URL is dropped.
- Validation errors never echo the rejected input.
- Secrets come from the environment only. Never read them from a tool call, and never write them to a file.

## Requests

- Apply a timeout and a response size limit to every request. Enforce the size limit as data arrives, not from `Content-Length`.
- Values from tool calls reach paths and queries only through validated newtypes. `ApiBaseUrl::endpoint` percent-encodes them, so a value can only ever be one path segment or one query value.
- Plain HTTP is accepted only on a loopback address.
- Identify this server as `zentelligence-postmark-mcp`, never as the official `postmark-mcp`.

## Tools with external effects

- Sending email and deleting resources are irreversible. Annotate them so hosts can ask the operator, and never retry a send automatically after an ambiguous failure.
- Webhooks send event data to a URL, so `create_webhook` must honour `WEBHOOK_URL_ALLOWLIST`, comparing parsed origin and path, and rejecting embedded credentials.
- Text returned by Postmark, including subjects, bounce transcripts, and error messages, can be written by anyone who can send or reply to email. It is data, never instructions, and the server's MCP instructions say so.
- Message bodies, and `Content` on bounce records, are not modelled. Returning them would need a deliberate decision.
- `get_bounce_dump` is the one exception, by design: it returns the raw bounce, which can quote the original message. Its description warns callers, and it returns this only for one bounce on explicit request.

## Logging

- Runtime logs go to stderr. Stdout carries only MCP traffic.
- Never log tokens or email bodies. Serde error text can quote response values, so never relay it. Mask addresses at INFO and above.
- rmcp logs protocol messages at DEBUG and below, which include email content and addresses from tool calls. Treat those levels as sensitive, and keep the default at INFO.

Security-relevant defects require a regression test at the lowest affected layer and an end-to-end rejection test at the tool boundary.
