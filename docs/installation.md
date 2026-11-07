# Installation

Requires Rust 1.96 or later. Building also needs a C compiler for the TLS library, and CMake on some platforms.

```sh
cargo install --path crates/postmark-mcp
```

To build from a checkout without installing:

```sh
cargo build --workspace --release
```

The binary is at `target/release/postmark-mcp`. Tagged releases also publish checksummed archives for Linux, Windows, and Apple Silicon macOS.

After installing, set the [required variables](configuration.md) and run `postmark-mcp doctor`.
