# MCP Contract Rules

The MCP surface is a public compatibility boundary. Treat tool names, input schemas, result shapes, error codes, and transport behaviour as versioned contracts.

## Tool implementation

- Name tools in `snake_case`, as the direct translation of the upstream `postmark-mcp` name (`sendEmail` becomes `send_email`). `docs/mcp-tools.md` maps every tool.
- Keep one domain operation behind all transports.
- Deserialise into argument types with `deny_unknown_fields`, then convert to validated domain queries with `TryFrom`, so bad input gets the stable `invalid/argument` code. Never use ad hoc conversion functions at this boundary. Values rmcp itself rejects while deserialising, such as an unknown enum value, get `isError` without a code.
- Return structured results. Do not make clients parse prose to obtain fields.
- Set all four annotations (`read_only_hint`, `destructive_hint`, `idempotent_hint`, `open_world_hint`) honestly, because hosts use them to decide when to ask the operator. Sending email is never idempotent.
- Declare `input_schema = input_schema_for::<Args>()` and `output_schema = fallible_output_schema_for::<T>()`, and return `Result<Json<T>, ToolFailure>`. These keep schemas free of nullable types and unions, which several hosts mishandle, and `catalogue_contract.rs` enforces it.
- A stream filter falls back to `DEFAULT_MESSAGE_STREAM` where Postmark would otherwise assume one stream. Where omitting it means all streams, as with statistics and webhooks, leave it omitted.
- Preserve stable machine-readable error codes, and provide an actionable message and remediation hint.
- Enforce documented limits, such as Postmark's 500 messages per batch, before calling the API.
- Batch tools report per-message results, and isolate per-item failures.

## Contract tests

For every tool, cover:

1. advertised name, description, annotations, and JSON schema;
2. smallest valid request and representative complete request;
3. response schema and content-block encoding;
4. every documented error code;
5. unknown fields, missing required fields, and incompatible options; and
6. limits, and rejection before any request reaches Postmark.

Tests invoke the MCP adapter through a real MCP client rather than calling only the underlying service. Keep separate adapter tests for HTTP behaviour.

Unknown arguments are rejected by rmcp as a tool result with `isError` set, not as a protocol error.

Protocol revisions and SDK versions are temporally sensitive. `rmcp` is pinned exactly. Before changing it, consult the official MCP specification and the SDK changelog, record the protocol revision in the change, and run the stdio tests. Keep SDK types out of the lower crates so SDK churn stops at `postmark-mcp`.
