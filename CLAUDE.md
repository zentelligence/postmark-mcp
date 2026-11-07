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

- `postmark-mcp-core`: validated values, result types, capability ports (`ReadsTemplates` and so on, each beside its types), the composite `PostmarkGateway`, and `GatewayError`.
- `postmark-mcp-client`: `HardenedTransport` (every request, and failure classification) and `PostmarkApi` (implements the ports, naming each route and its parameters).
- `postmark-mcp`: environment configuration, the MCP server, `tools/*` routers, `schema.rs`, `ToolFailure`, and the binary.

A tool call flows: rmcp, arguments in `tools/<area>.rs`, `TryFrom` to a validated query, a port, `PostmarkApi`, `HardenedTransport`, Postmark. A result type decodes from Postmark's `PascalCase` and serialises as `snake_case`, so one struct serves both ends. Failures return as `GatewayError` or `ValueError`, then `ToolFailure`, a tool result with `isError` set.

## Adding a tool

1. Write the contract test first, in `crates/postmark-mcp/tests/`. It calls the tool by name, so it compiles before the tool exists, and fails with `tool not found`.
2. Add the query and result types to the area's module in `postmark-mcp-core`, modelling only fields worth returning, checked against the Postmark API documentation. Use the decoders in `wire.rs`, and `sanitised_url` for any URL.
3. Implement the port in `postmark-mcp-client/src/api.rs`.
4. Add the arguments and handler to `postmark-mcp/src/tools/<area>.rs`, with `input_schema = input_schema_for::<Args>()` and `output_schema = fallible_output_schema_for::<Result>()`. Merge any new router in `PostmarkMcpServer::new`.
5. Add the name to `catalogue_contract.rs`, then update `docs/mcp-tools.md`, `docs/roadmap.md`, and `CHANGELOG.md`.

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
- The orphan rule shapes two conversions: `transport.rs` wraps `reqwest::Error` in `RequestFailure`, and tools wrap arguments in `WithDefaultStream`.
- Result types need `#[skip_serializing_none]` above their derives. `schema.rs` removes nullability from schemas, so a `null` in a result would contradict the advertised schema.
- A result type's top-level field cannot be called `code`, `message`, or `remediation`, because `fallible_output_schema_for` merges it with `ToolFailure`. `every_output_definition_is_used` and the type check in `success` catch this.
- `PostmarkMcpServer` overrides `call_tool` to log every call, so `#[tool_handler]` generates only `list_tools` and `get_tool`.
- `send_policy_contract.rs` drives the binary, because the send policy and logging come from the environment.
- Edit scripts must assert their patterns match, because `cargo fmt` rewraps code.
- Wiremock's path matcher is case-sensitive, but Postmark's API is not.
- `vocabulary!` variants must stay undocumented: a doc comment turns the advertised `enum` into a `oneOf` of consts.
- `.claude/scripts/check.sh` validates every relative Markdown link and rejects trailing whitespace in guidance files.
- `#[tool_handler]` output trips `clippy::unused_async_trait_impl`, which is why `server.rs` carries a narrow allow.
