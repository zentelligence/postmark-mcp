# Live Acceptance

Automated tests never contact Postmark. This check does: it runs the real binary against a real Postmark server, to confirm that the result types match what Postmark actually returns. Run it after changing a result type, a route, or the MCP SDK, and record the outcome in [`roadmap.md`](roadmap.md) under "Verified live".

It prints only field names, counts, error codes, and yes or no answers, never addresses, subjects, bodies, or other account content, so its output is safe to share. The token comes from your environment and is never printed.

## Running it

```sh
cargo build -p postmark-mcp
export POSTMARK_SERVER_TOKEN=...          # a server token
export DEFAULT_SENDER_EMAIL=you@example.com  # a verified sender on that server
python3 -I scripts/acceptance/live_probe.py "$PWD/target/debug/postmark-mcp"
```

That run is read-only. Two optional arguments extend it:

- `send` sends exactly one email, to Postmark's black hole address, `test@blackhole.postmarkapp.com`, tagged `postmark-mcp-acceptance`.
- `templates` creates a throwaway layout and template with timestamped aliases, reads, lists, and edits them, sends one email from the template to the black hole address, detaches the layout, and deletes both. Deletion is attempted even if an earlier step fails. It prints `PASS` or `FAIL` for each check.

Each email counts towards the server's sending volume.

Set `ACCEPTANCE_STDERR` to a file path to keep the server's stderr, which includes the tool log, for checking by hand. Afterwards, confirm the token is absent from it without printing either:

```sh
grep -cF "$POSTMARK_SERVER_TOKEN" "$ACCEPTANCE_STDERR"   # must print 0
```

## Coverage

A server without templates, webhooks, or multi-recipient messages leaves those parts unchecked; the output says which steps had nothing to work with. A sandbox server suits the send step, since it never delivers mail.
