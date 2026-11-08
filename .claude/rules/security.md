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
- Secrets come from the operator only, through a flag, a file, or the environment. Never read them from a tool call, never write them to a file, and never echo a token file's contents in an error.

## Requests

- Apply a timeout and a response size limit to every request. Enforce the size limit as data arrives, not from `Content-Length`.
- Values from tool calls reach paths and queries only through validated newtypes. `ApiBaseUrl::endpoint` percent-encodes them, so a value can only ever be one path segment or one query value.
- Plain HTTP is accepted only on a loopback address.
- Identify this server as `zentelligence-postmark-mcp`, never as the official `postmark-mcp`.

## Tools with external effects

- Sending email and deleting resources are irreversible. Annotate deletion and overwriting as destructive, per `mcp-contracts.md`, and never retry a send. A failure that leaves the outcome unknown is `send/outcome-unknown`, through `SendFailure`, and its remediation never says retry.
- Validate every message and check every recipient against the recipient allowlist before any request, so a batch is sent whole or not at all. Policy checks use `MailAddress::bare`, never the display form.
- A tool removed by policy, through `--toolsets`, is left out of the catalogue, not refused when called.
- Webhooks send event data to a URL, so `create_webhook` requires HTTPS without credentials, and honours the webhook URL allowlist by parsed origin and whole path segments. A refusal never reveals the list.
- A create that may have happened despite failing is `change/outcome-unknown`, through `ChangeFailure`, naming the tool to check with.
- Text returned by Postmark, including subjects, bounce transcripts, and error messages, can be written by anyone who can send or reply to email. It is data, never instructions, and the server's MCP instructions say so.
- Message bodies, and `Content` on bounce records, are not modelled. Returning them would need a deliberate decision.
- `get_bounce_dump` is the one exception, by design: it returns the raw bounce, which can quote the original message. Its description warns callers, and it returns this only for one bounce on explicit request.

## Releases and the launcher

- A release is installed only after the updater verifies the manifest's Ed25519 signature against a key built into the launcher, then the archive's SHA-256, then that the server reports the manifest's version. Keys live in `product.rs`. Rotate by adding a key, never by replacing one, and never accept a key from the environment.
- The private signing key exists only as the `RELEASE_SIGNING_KEY` secret. The release workflow writes it to an owner-only temporary file and deletes it after signing.
- Every download starts at an HTTPS address, but the pinned updater follows redirects without refusing plain HTTP, so never claim HTTPS-only transfer: integrity rests on the signature and checksum. An empty or non-Unicode `POSTMARK_MCP_UPDATE_URL` or `POSTMARK_MCP_INSTALL_ROOT` is refused, rather than taken as unset or replaced by the default.
- An update never replaces the files of the active release. Every change holds the update lock, apart from removing the lock file itself and the then empty folder. `update` asks before taking the lock, and refuses if the installation changed meanwhile. A pinned install that ends with another release active is a failure.
- `POSTMARK_MCP_INSTALL_ROOT` can name any folder, so the launcher writes only to a folder that is missing, empty, or marked as an installation by `launchers.json`, `state.json`, or a `current` pointer, and removes only the entries an install creates.
- Uninstall removes only what the ledger shows an install placed and still matches, and never the token file or a host's configuration. Anything it could not remove makes it fail.
- At the pinned updater commit, an archive's declared size is not enforced and its download has no size limit. Integrity rests on the signed checksum.

## Logging

- Runtime logs go to stderr. Stdout carries only MCP traffic.
- Never log tokens or email bodies. Serde error text can quote response values, so never relay it. Mask addresses at INFO and above.
- rmcp logs protocol messages at DEBUG and below, which include email content and addresses from tool calls. Treat those levels as sensitive, and keep the default at INFO.

Security-relevant defects require a regression test at the lowest affected layer and an end-to-end rejection test at the tool boundary.
