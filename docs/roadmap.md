# Roadmap

The goal is parity with the 24 tools of the official `postmark-mcp` server, delivered from least to most consequential, so the patterns are proven on read-only tools before anything can send mail. [`mcp-tools.md`](mcp-tools.md) lists each tool by stage.

## Delivered

- **Stage 0, foundation.** Workspace, CI, hardened transport, environment configuration, stdio server, `doctor`, and `get_server_info`.
- **Stage 1, read-only tools.** Ten tools across templates, messages, bounces, suppressions, stats, and webhooks.
- **Stage 2, diagnostics.** `diagnose_delivery`, which finds the message, checks suppressions and bounces on its stream, and reports what it could not establish rather than treating it as good news.

## Planned

- **Stage 3, sending.** The four send tools, plus tool call logging with address masking (`LOG_FILE`, `LOG_EMAIL_FULL`), which must land first.
- **Stage 4, mutation.** Template, bounce, suppression, and webhook changes, including `WEBHOOK_URL_ALLOWLIST`.
- **Stage 5, beyond parity.** Candidates only: the asynchronous bulk API, account-level tools (domains, sender signatures) using `POSTMARK_ACCOUNT_TOKEN`, and streamable HTTP, which would need authentication.

## Decided

- **No Postmark SDK.** `postmark-rs` was assessed and removed. Its endpoints are declarative models, and 6 of the 12 read-side tools needed something they lacked, including `MessageEvents`, half the outbound search filters, and bounce result fields.

- **Bounce source is returned.** `get_bounce_dump` returns raw bounce source, which can quote the original message, including its body. This matches the official server, and the tool's description warns callers.
- **URL paths are kept.** Credentials, query strings, and fragments are removed from webhook URLs, but paths are not, even though some services put a secret there. A path is useful for diagnosis, and a secret in one cannot be told from an ordinary path.

## Verification still owed

Result types were written from the Postmark API documentation and the official server's code, then tested against responses written by hand. A read-only pass with a real server token is needed to confirm them, in particular:

- `open_read_times`, which the official server uses, but the API reference does not list.
- `+` versus `%20` for spaces in query values. This server uses the former, the official server the latter.
- The shape of `Metadata`, which the documentation describes inconsistently.
- Whether `to_date` covers the whole of its final day. Postmark's examples for messages and bounces show date-times, so a bare date may mean midnight.
- `MessageStream` on `list_webhooks`, where this server follows the documentation and the official server sends `messagestream`.
- Which fields appear in message event `Details`, since only the allowlisted ones are returned.
- Whether `Recipients` includes Cc and Bcc addresses, and whether it ever holds display names. A recipient missing from it reads as `no_recent_message`.
- Whether `Details.BounceID` on a `Bounced` event links to the bounce record. If it does not, an out of office reply logged as a `Bounced` event reads as `bounced`.
- The order of message search results. `diagnose_delivery` takes the first match as the most recent, and neither the search nor the bounce documentation states a sort order.
- Whether `ManualSuppression` with origin `Recipient` can be removed. The documentation mentions an authority failure without saying when it applies, so the recommendation treats it as the recipient's to reverse.

## Open decisions

Each needs settling before its stage begins.


- **Sending guardrails (stage 3).** Whether to go beyond tool annotations with a recipient allowlist, or a read-only mode that leaves send and mutation tools out of the catalogue. An agent that reads email can be steered by that email's content, so this deserves a deliberate answer.
- **Tracking defaults (stage 3).** The official server always enables open and link tracking. Tracking has privacy implications, so whether to match that or leave it to the server's settings is a real choice.
