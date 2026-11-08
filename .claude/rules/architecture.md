# Architecture Rules

The architecture is hexagonal. Domain contracts remain independent of MCP, Tokio, and HTTP.

## Dependency boundaries

```text
postmark-mcp (composition root, MCP adapter, binary)
    |-- postmark-mcp-client (Postmark adapter)
    |       |-- postmark-mcp-core
    |-- postmark-mcp-core (domain types and ports)

postmark-mcp-launcher (installer, updater, and stable launcher binary)
    |-- updater-core, updater-engine, updater-launcher (zentelligence/auto-updater)
```

- `postmark-mcp-core` owns validated values, result types, ports, and `GatewayError`. It has no adapter dependencies. `serde`, `serde_json`, `schemars`, and `url` are allowed there because they describe data, not transport.
- `postmark-mcp-client` implements the ports. It is the only crate that names `reqwest` types or Postmark routes, and it never imports `postmark-mcp`.
- `postmark-mcp` owns configuration, the tool catalogue, and error translation. Tool handlers translate requests to domain types, call a port, and translate results.
- `postmark-mcp-launcher` depends on no crate of this workspace, and none depends on it. It never contacts Postmark and never sees a token, so the server binary carries no updater code, and the launcher no Postmark code. The updater does the work, and the launcher decides what should happen, names the product, validates its own settings, guards the installation folder, takes the update lock around every change, and words the outcome. Every external collaborator reaches the lifecycle through `Host`, so `test_support.rs` drives it with signed fixtures and no network.
- The only contract between the two binaries is that `postmark-mcp --version` prints `postmark-mcp <version>` with a bare version, which the updater reads before activating a release.
- Keep transport types out of port signatures. Convert at the adapter edge with `From` or `TryFrom`, using a local wrapper where the orphan rule requires one.

## Reaching Postmark

- Every request goes through `HardenedTransport`. Never construct another HTTP client for Postmark.
- Postmark is reached directly, not through an SDK. `postmark-rs` was assessed and removed: its value is declarative models, which were incomplete for half the read-side tools, and its client lacks a timeout and follows redirects. It remains a useful reference for field names.
- One struct serves as wire type and tool result: it decodes from Postmark's `PascalCase` and serialises as `snake_case`. A separate layer would add mapping code and no safety.
- Result types model only fields worth returning, checked against the Postmark API documentation. Unmodelled fields are discarded while decoding, which is how `ApiTokens` stays out of the process.
- Sanitise while decoding, in `wire.rs`, so an unsanitised value never exists as a result type.
- Required fields are those a tool cannot work without; anything else is `Option`, so one missing field does not fail a whole call. Where Postmark's documentation contradicts itself about a shape, decode tolerantly.
- A secret has no place in a result type, so it cannot reach a tool result.

## Ports and testability

- One small capability trait per area of the Postmark API, such as `ReadsServer`. Do not create a single wide client trait or a service locator.
- Keep ports object-safe. The server holds one `Arc<dyn PostmarkGateway>`, which every capability trait composes into.
- Inject environment, clocks, and any other external state. Tests never read the process environment.

Any proposed boundary change must state the dependency direction, failure policy, and how it will be tested before implementation.
