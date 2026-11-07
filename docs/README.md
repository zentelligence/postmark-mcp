# Documentation

- [`installation.md`](installation.md): building and installing
- [`configuration.md`](configuration.md): environment variable reference
- [`cli-reference.md`](cli-reference.md): the `postmark-mcp` command line
- [`mcp-tools.md`](mcp-tools.md): the tool catalogue, and its mapping to the official server
- [`roadmap.md`](roadmap.md): what is delivered, what comes next, and open decisions

Rustdoc comments are the source of truth for implementation detail:

```sh
cargo doc --workspace --no-deps --open
```
