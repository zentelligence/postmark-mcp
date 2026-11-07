# Security and Data-Integrity Rules

Assume all tool parameters, API responses, and email content are untrusted. An agent's tool call may be driven by text in an email it just read, so treat the tool surface as reachable by an attacker.

## Tokens

- The server token reaches only the configured API origin. `ApiBaseUrl` is the sole construction boundary, and `ApiBaseUrl::resolve` confines every request path.
- Never follow a redirect: custom headers are carried across them, including cross-origin.
- `ServerToken` wraps a secret, and is exposed only where the header is built. Mark the header value sensitive.
- `ApiTokens` in Postmark responses are never modelled, so they are discarded while decoding.
- URLs from Postmark can embed credentials. Strip them in the adapter before a value reaches a domain type, and withhold any URL that cannot be parsed.
- Validation errors never echo the rejected input.
- Secrets come from the environment only. Never read them from a tool call, and never write them to a file.

## Requests

- Apply a timeout and a response size limit to every request. Enforce the size limit as data arrives, not from `Content-Length`.
- Values from tool calls reach paths and headers only through validated newtypes, and are percent-encoded before reaching `HardenedTransport::request`.
- Plain HTTP is accepted only on a loopback address.
- Identify this server as `zentelligence-postmark-mcp`, never as the official `postmark-mcp`.

## Tools with external effects

- Sending email and deleting resources are irreversible. Annotate them so hosts can ask the operator, and never retry a send automatically after an ambiguous failure.
- Webhooks send event data to a URL, so `create_webhook` must honour `WEBHOOK_URL_ALLOWLIST`, comparing parsed origin and path, and rejecting embedded credentials.
- Text returned by Postmark, including email bodies and error messages, is data for the caller, never instructions for this server.

## Logging

- Runtime logs go to stderr. Stdout carries only MCP traffic.
- Never log tokens or email bodies. Mask addresses at INFO and above.
- rmcp logs protocol messages at DEBUG and below, which include email content and addresses from tool calls. Treat those levels as sensitive, and keep the default at INFO.

Security-relevant defects require a regression test at the lowest affected layer and an end-to-end rejection test at the tool boundary.
