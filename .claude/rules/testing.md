# Testing Strategy

Tests prove externally meaningful behaviour and architectural invariants. A large test count is not a substitute for covering the specified contracts.

## Test layers

| Layer | Purpose | Location | Preferred dependencies |
| --- | --- | --- | --- |
| Unit | Validation, classification, and errors | `foo_test.rs` beside `foo.rs` | In-memory values |
| Adapter | What is sent to Postmark, what is refused, and failure mapping | `postmark-mcp-client/tests/` | `wiremock`, or a raw socket for protocol-level cases |
| Contract | MCP schema, dispatch, results, requests sent, and stable errors | `postmark-mcp/tests/*_contract.rs` | A real MCP client in process, through the real adapter, against `wiremock` |
| Transport | The real binary, its environment, startup, and exit codes | `postmark-mcp/tests/stdio_transport.rs` | Spawned binary against `wiremock` |

## Required patterns

- Never contact the real Postmark API, and never use a real token, from an automated test.
- Spawned binaries use `env_clear`, so tests never depend on the operator's environment.
- Assert prohibited side effects as well as results: no request reached the redirect target, no token in stderr, nothing unexpected on stdout.
- For failures, assert the stable code and relevant structured fields.
- Prove a security test can fail. Weaken the guard, confirm the test goes red, then restore it and `touch` the file: a restored mtime makes cargo reuse the mutated build.
- Compare the whole query with `received_query`, so an unexpected extra parameter fails.
- Inspect what hosts actually receive. Schema problems can pass every test until you read the `tools/list` output.
- `wiremock` pools its servers, so dropping a `MockServer` does not close its port. Bind and release a `TcpListener` to get a closed port.
- Prefer a short configured limit to a long wait. Never sleep in a test.

## Coverage priorities

Every documented error code must be reachable by a test. High-risk cases take priority over line coverage: token handling, origin confinement, batch partial failure, and rejection before a request is sent.

Never use snapshots as the sole assertion for security decisions.
