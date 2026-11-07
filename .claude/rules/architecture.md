# Architecture Rules

The architecture is hexagonal. Domain contracts remain independent of MCP, Tokio, and HTTP.

## Dependency boundaries

```text
postmark-mcp (composition root, MCP adapter, binary)
    |-- postmark-mcp-client (Postmark adapter)
    |       |-- postmark-mcp-core
    |-- postmark-mcp-core (domain types and ports)
```

- `postmark-mcp-core` owns validated values, domain results, ports, and `GatewayError`. It has no adapter dependencies.
- `postmark-mcp-client` implements the ports. It is the only crate that names `reqwest` types or Postmark wire types, and it never imports `postmark-mcp`.
- `postmark-mcp` owns configuration, the tool catalogue, and error translation. Tool handlers translate requests to domain types, call a port, and translate results.
- Keep transport types out of port signatures. Convert at the adapter edge with `From` or `TryFrom`, using a local wrapper where the orphan rule requires one.

## Reaching Postmark

- Every request goes through `HardenedTransport::request`. Never construct another HTTP client for Postmark.
- Postmark is reached directly, not through an SDK. `postmark-rs` was assessed and removed: its value is declarative models, which were incomplete for most read-side tools, and its client lacks a timeout and follows redirects. It remains a useful reference for field names.
- Wire types live in `postmark-mcp-client` and model only the fields used, checked against the Postmark API documentation. Unmodelled fields are discarded while decoding, which is how `ApiTokens` stays out of the process.
- Required wire fields are those a tool cannot work without; anything else is `Option`, so one missing field does not fail a whole call.
- Domain results hold only what callers need. A secret has no place in a domain type, so it cannot reach a tool result.

## Ports and testability

- One small capability trait per area of the Postmark API, such as `ReadsServer`. Do not create a single wide client trait or a service locator.
- Keep ports object-safe, so tests can substitute a domain-specific fake.
- Inject environment, clocks, and any other external state. Tests never read the process environment.

Any proposed boundary change must state the dependency direction, failure policy, and how it will be tested before implementation.
