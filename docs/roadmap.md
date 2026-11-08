# Roadmap

The goal is parity with the 24 tools of the official `postmark-mcp` server, delivered from least to most consequential, so the patterns are proven on read-only tools before anything can send mail. [`mcp-tools.md`](mcp-tools.md) lists each tool by stage.

## Delivered

- **Stage 0, foundation.** Workspace, CI, hardened transport, environment configuration, stdio server, `doctor`, and `get_server_info`.
- **Stage 1, read-only tools.** Ten tools across templates, messages, bounces, suppressions, stats, and webhooks.
- **Stage 2, diagnostics.** `diagnose_delivery`, which finds the message, checks suppressions and bounces on its stream, and reports what it could not establish rather than treating it as good news.
- **Stage 3, sending.** The four send tools, a read-only mode, a recipient allowlist, and tool call logging with address masking.
- **Stage 4, managing.** The eight tools that change templates, suppressions, bounces, and webhooks, completing the official server's 24. With it, every setting became a command-line flag, with environment variables as a fallback, and `--toolsets` replaced the read-only switch.

- **Distribution.** `postmark-mcp-launcher` installs, updates, rolls back, and removes the server as versioned release sets, on `zentelligence/auto-updater`. Releases are signed, installed on every platform before they become the latest, and started through a stable launcher on `PATH`, so an MCP host's entry survives every update.

## Planned

- **Stage 5, beyond parity.** Candidates only: the asynchronous bulk API, account-level tools (domains, sender signatures) using `POSTMARK_ACCOUNT_TOKEN`, and streamable HTTP, which would need authentication.

## Decided

- **No Postmark SDK.** `postmark-rs` was assessed and removed. Its endpoints are declarative models, and 6 of the 12 read-side tools needed something they lacked, including `MessageEvents`, half the outbound search filters, and bounce result fields.

- **Bounce source is returned.** `get_bounce_dump` returns raw bounce source, which can quote the original message, including its body. This matches the official server, and the tool's description warns callers.
- **URL paths are kept.** Credentials, query strings, and fragments are removed from webhook URLs, but paths are not, even though some services put a secret there. A path is useful for diagnosis, and a secret in one cannot be told from an ordinary path.

- **No per-send confirmation.** Asking a person to approve each send, for example through MCP elicitation, was considered and rejected: the intended use is automated sending, such as email sequences before and after a workshop, where nobody is there to approve each one. Interactive hosts can still be set to ask before a send, through their own tool permissions.
- **Configuration by flags.** Not every host can set environment variables, so every setting is a flag, with its variable as a fallback. The token can come from a file, a variable, or a flag, the last being simplest but visible in the process list.
- **Toolsets.** `--toolsets` chooses among `read`, `send`, and `manage`, all on by default to match the official server. Deleting and overwriting are not a separate group: they are annotated destructive, and a host's tool permissions can deny any single tool.
- **Recipient allowlist.** `--recipient-allowlist` restricts recipients, for staging, internal-only, and demonstration deployments only.
- **Tracking.** This server never sets open or link tracking, so each Postmark server's settings decide, unlike the official server, which always turns both on.
- **One launcher, no automatic updates.** Installing, updating, and removing belong to `postmark-mcp-launcher`, not to the server, which stays free of updater code. Updates happen only when asked for, since a server an MCP host has running cannot be switched under it.
- **No automatic retries.** Postmark's documentation says nothing about idempotency, so a send that may have reached it is reported as `send/outcome-unknown`, never retried.

## Verified live

On 2026-11-07, every tool was run against a real Postmark server, through the real binary over stdio, at version 0.4.0. This confirmed:

- Every read-only tool's result type decodes real responses, including `get_server_info`, message search and details, bounces, the bounce dump, suppressions, all fourteen statistics, and `validate_template`.
- `open_read_times` works, although the API reference does not list it.
- `+` for spaces in query values works: a search by a subject containing spaces finds its message.
- `to_date` covers the whole of its final day.
- `Metadata` is an object.
- Message search results are newest first, which `diagnose_delivery` relies on.
- `Details.BounceID` on a `Bounced` event matches the bounce record's ID, so a linked record can overrule the event as designed.
- `send_email` sends, returns its receipt, and is reported as `Delivered` within seconds. With tracking left unset, the message follows the server's own settings.
- `diagnose_delivery` finds and correctly diagnoses a message just sent.
- The token appeared nowhere in stderr, and the tool log masked addresses and reduced content to its size.

At version 0.5.0, a throwaway layout and template were taken through their whole lifecycle on the same server, confirming that:

- `create_template`, `get_template`, and `list_templates` handle layouts, and templates that use them.
- `edit_template` changes only the field asked for, keeping the name, bodies, alias, layout, and type.
- `send_email_with_template` sends, rendering the edited subject, and is reported `Delivered`.
- `detach_layout` removes a layout and keeps everything else.
- `delete_template` deletes, and a deleted template then reads as `postmark/rejected`.

## Verification still owed

The launcher's first real install, update, and uninstall happen in the release workflow's `verify-install` job, which needs `RELEASE_SIGNING_KEY` set. Until a release has passed it, these remain unconfirmed: the signed manifest and archives, adding to `PATH` on each platform, an update replacing a running launcher on Windows, and `update` and `rollback` between two published releases.

The live server had no templates, no webhooks, and only single-recipient messages, so these remain unconfirmed:

- `send_batch` and `send_batch_with_template`.
- `activate_bounce`, both suppression tools, and both webhook tools, which the live server had nothing safe to exercise. `create_webhook` also needs a URL that answers Postmark's verification.
- `MessageStream` on `list_webhooks`, where this server follows the documentation and the official server sends `messagestream`.
- Whether `Recipients` includes Cc and Bcc addresses, and whether it ever holds display names. A recipient missing from it reads as `no_recent_message`.
- Which fields appear in message event `Details` for events other than `Delivered` and `Bounced`, since only the allowlisted ones are returned.
- Whether `ManualSuppression` with origin `Recipient` can be removed. The documentation mentions an authority failure without saying when it applies, so the recommendation treats it as the recipient's to reverse.

## Known risks

- With the `manage` toolset on and no webhook URL allowlist, an agent steered by content it has read could register a webhook that sends events, including clicked links, which can hold live tokens, to an address of the attacker's choosing. This matches the official server. Deployments that do not need to manage webhooks can leave `manage` out of `--toolsets`, or set `--webhook-url-allowlist`.

## Open decisions

- **Guardrails for automated sending.** Candidates that limit what an automated run can do without a person approving each send: a mode allowing only template sends, so an agent can fill in a template but not write free text; a cap on recipients per call or per run; and a `message_stream` argument, so sequences can use a broadcast stream rather than `DEFAULT_MESSAGE_STREAM`.

