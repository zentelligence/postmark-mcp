# Documentation

- [`installation.md`](installation.md): installing, updating, and removing, and building from source
- [`configuration.md`](configuration.md): environment variable reference
- [`cli-reference.md`](cli-reference.md): the `postmark-mcp` and `postmark-mcp-launcher` command lines
- [`mcp-tools.md`](mcp-tools.md): the tool catalogue, and its mapping to the official server
- [`acceptance.md`](acceptance.md): checking the server against a real Postmark account
- [`roadmap.md`](roadmap.md): what is delivered, what comes next, and open decisions
- [`upstream-postmark-mcp.md`](upstream-postmark-mcp.md): what the official Node.js server does, as a reference when building the same tools here

Rustdoc comments are the source of truth for implementation detail:

```sh
cargo doc --workspace --no-deps --open
```
