@AGENTS.md

# Postmark MCP

Rust MCP server for the Postmark email API. Cargo workspace of library crates behind the `postmark-mcp` binary.

**`AGENTS.md` is the binding contract for every substantive task in this repository.** The `.claude/` directory holds the detailed rules, workflows, review lens, and templates it references.

## Commands

| Purpose | Command |
| --- | --- |
| Completion gate | `.claude/scripts/check.sh` |
| Rust gate only | `just ci` |
| Targeted red/green test | `cargo test -p <crate> <test_name> -- --exact` |
| Check configuration and token | `cargo run -p postmark-mcp -- doctor` |
| Regenerate licence report | `just licences` |

## Architecture in one screen

- `postmark-mcp-core`: validated values, domain results, capability ports (`ReadsServer`), and `GatewayError`.
- `postmark-mcp-client`: `HardenedTransport` (every request, and failure classification) and `PostmarkApi` (wire types and the port implementations).
- `postmark-mcp`: environment configuration, the MCP server, `tools/*` routers, `ToolFailure`, and the binary.

A tool call flows: rmcp, `tools/<area>.rs`, a port, `PostmarkApi`, `HardenedTransport`, Postmark. Failures return as `GatewayError`, then `ToolFailure`, a tool result with `isError` set.

## Adding a tool

1. Add or extend a capability port in `postmark-mcp-core`, with domain types that omit anything secret.
2. Implement it in `postmark-mcp-client/src/api.rs`, with a wire type modelling only the fields used, checked against the Postmark API documentation, and adapter tests in `tests/transport_contract.rs`.
3. Add the handler to `postmark-mcp/src/tools/<area>.rs`, following `tools/server.rs`, and merge any new router in `PostmarkMcpServer::new`.
4. Add contract tests, then update `docs/mcp-tools.md`, `docs/roadmap.md`, and `CHANGELOG.md`.

## Rule routing

| When the task touches | Read |
| --- | --- |
| Any production behaviour | `.claude/rules/testing.md`, `.claude/workflows/tdd.md`, `.claude/workflows/change-loop.md` |
| Crate boundaries, traits, or services | `.claude/rules/architecture.md` |
| MCP tools, schemas, or transports | `.claude/rules/mcp-contracts.md` |
| Tokens, HTTP, email content, or external input | `.claude/rules/security.md` |
| Rust code, dependencies, errors, async, or logging | `.claude/rules/rust-quality.md` |
| Completion or CI | `.claude/workflows/quality-gate.md` |
| Session memory or knowledge stores | `.claude/rules/memory.md` |

## Repository gotchas

- The binary name is coupled to `env!("CARGO_BIN_EXE_postmark-mcp")` in `tests/stdio_transport.rs`.
- The orphan rule blocks `From<reqwest::Error>` for `GatewayError`, so `transport.rs` uses a local wrapper (`RequestFailure`).
- Optional result fields need `#[schemars(schema_with = "plain::<T>")]` and `skip_serializing_if`, because several MCP hosts mishandle `["string", "null"]`.
- `.claude/scripts/check.sh` validates every relative Markdown link and rejects trailing whitespace in guidance files.
- `#[tool_handler]` output trips `clippy::unused_async_trait_impl`, which is why `server.rs` carries a narrow allow.
