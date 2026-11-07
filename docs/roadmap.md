# Roadmap

The goal is parity with the 24 tools of the official `postmark-mcp` server, delivered from least to most consequential, so the patterns are proven on read-only tools before anything can send mail. [`mcp-tools.md`](mcp-tools.md) lists each tool by stage.

## Delivered

- **Stage 0, foundation.** Workspace, CI, hardened transport, environment configuration, stdio server, `doctor`, and `get_server_info`.
- **Stage 1, read-only tools.** Ten tools across templates, messages, bounces, suppressions, stats, and webhooks.
- **Stage 2, diagnostics.** `diagnose_delivery`, which finds the message, checks suppressions and bounces on its stream, and reports what it could not establish rather than treating it as good news.
- **Stage 3, sending.** The four send tools, a read-only mode, a recipient allowlist, and tool call logging with address masking.

## Planned

- **Stage 4, mutation.** Template, bounce, suppression, and webhook changes, including `WEBHOOK_URL_ALLOWLIST`.
- **Stage 5, beyond parity.** Candidates only: the asynchronous bulk API, account-level tools (domains, sender signatures) using `POSTMARK_ACCOUNT_TOKEN`, and streamable HTTP, which would need authentication.

## Decided

- **No Postmark SDK.** `postmark-rs` was assessed and removed. Its endpoints are declarative models, and 6 of the 12 read-side tools needed something they lacked, including `MessageEvents`, half the outbound search filters, and bounce result fields.

- **Bounce source is returned.** `get_bounce_dump` returns raw bounce source, which can quote the original message, including its body. This matches the official server, and the tool's description warns callers.
- **URL paths are kept.** Credentials, query strings, and fragments are removed from webhook URLs, but paths are not, even though some services put a secret there. A path is useful for diagnosis, and a secret in one cannot be told from an ordinary path.

- **No per-send confirmation.** Asking a person to approve each send, for example through MCP elicitation, was considered and rejected: the intended use is automated sending, such as email sequences before and after a workshop, where nobody is there to approve each one. Interactive hosts can still be set to ask before a send, through their own tool permissions.
- **Sending guardrails.** `POSTMARK_READ_ONLY` removes the send tools, and `POSTMARK_RECIPIENT_ALLOWLIST` restricts recipients. Both are off by default, matching the official server, so existing host entries behave as before.
- **Tracking.** This server never sets open or link tracking, so each Postmark server's settings decide, unlike the official server, which always turns both on.
- **No automatic retries.** Postmark's documentation says nothing about idempotency, so a send that may have reached it is reported as `send/outcome-unknown`, never retried.

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
- A live send, ideally from a sandbox server or to `test@blackhole.postmarkapp.com`, confirming the request and response shapes, and that leaving tracking unset defers to the server's settings.
- The order of message search results. `diagnose_delivery` takes the first match as the most recent, and neither the search nor the bounce documentation states a sort order.
- Whether `ManualSuppression` with origin `Recipient` can be removed. The documentation mentions an authority failure without saying when it applies, so the recommendation treats it as the recipient's to reverse.

## Open decisions

- **Guardrails for automated sending.** Candidates that limit what an automated run can do without a person approving each send: a mode allowing only template sends, so an agent can fill in a template but not write free text; a cap on recipients per call or per run; and a `message_stream` argument, so sequences can use a broadcast stream rather than `DEFAULT_MESSAGE_STREAM`.

Beyond that, none outstanding. Stage 4 will need decisions on whether template, suppression, and webhook changes fall under `POSTMARK_READ_ONLY`, and whether deletions need a separate switch.

