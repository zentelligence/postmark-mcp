# Postmark MCP

A Rust [Model Context Protocol](https://modelcontextprotocol.io) server for the [Postmark](https://postmarkapp.com) email API. It gives AI assistants structured, safety-conscious access to sending, templates, delivery diagnostics, and more, as a single self-contained binary.

This is an independent project, not affiliated with ActiveCampaign or Postmark. Its tool surface and configuration follow ActiveCampaign's official [`postmark-mcp`](https://github.com/ActiveCampaign/postmark-mcp).

## Status

Early development. The server runs over stdio, supports MCP protocol revisions from 2024-11-05 to 2026-07-28, and offers all 24 tools of the official server, in three groups: twelve that read, including delivery diagnosis, four that send email, and eight that change templates, suppressions, bounces, and webhooks. See [`docs/mcp-tools.md`](docs/mcp-tools.md) for the tool catalogue, and [`docs/roadmap.md`](docs/roadmap.md) for what comes next.

## Installation

Download the archive for your platform from [Releases](https://github.com/zentelligence/postmark-mcp/releases), unpack it, and from the unpacked folder run:

```sh
./bin/postmark-mcp-launcher install
```

On Windows, run `bin\postmark-mcp-launcher.exe install`. This puts `postmark-mcp` on your `PATH`, and `postmark-mcp-launcher update` and `postmark-mcp-launcher uninstall` later update and remove it. To build from source instead, with Rust 1.96 or later:

```sh
cargo install --path crates/postmark-mcp
```

See [`docs/installation.md`](docs/installation.md) for details.

## Quick start

Check your settings against Postmark:

```sh
postmark-mcp --server-token-file ~/.config/postmark-mcp/token \
  --default-sender sender@example.com --default-message-stream outbound doctor
```

Then add an entry like this to your MCP host's configuration:

```json
{
  "mcpServers": {
    "postmark": {
      "command": "postmark-mcp",
      "args": [
        "--server-token-file", "/home/you/.config/postmark-mcp/token",
        "--default-sender", "sender@example.com",
        "--default-message-stream", "outbound"
      ]
    }
  }
}
```

Every setting is a flag, with an environment variable fallback where a host can set one. See [`docs/configuration.md`](docs/configuration.md) for every setting, including `--toolsets`, which chooses whether the server can send and change account state.

## Security

- `--toolsets` chooses which groups of tools exist at all, so an agent that only sends cannot change templates or lift suppressions.
- Recipients can be restricted with `--recipient-allowlist`, for staging and internal-only use, and webhook destinations with `--webhook-url-allowlist`.
- A send that may or may not have gone out says so, rather than inviting a retry that could send twice.
- The server token is sent only to the configured API origin, and redirects are always refused.
- Every request has a timeout and a response size limit.
- Tokens never appear in logs, errors, or tool results, including the API tokens Postmark returns in its own responses.
- Webhook passwords and custom headers never reach a tool result, and credentials, query strings, and fragments are removed from webhook URLs. A secret in a URL's path cannot be detected, and is not removed.
- Releases are installed only after their signed manifest and checksum are verified, and an update never replaces a running server's files.
- Message bodies and clicked link URLs are not returned. The exception is `get_bounce_dump`, which returns a bounce's raw source, and that can quote the original message.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `postmark-mcp-core` | Validated values, result types, and ports. No adapter dependencies. |
| `postmark-mcp-client` | Hardened HTTP transport, and the port implementations. |
| `postmark-mcp` | Configuration, the MCP server, the tool catalogue, and the binary. |
| `postmark-mcp-launcher` | Starts the active release, and installs, updates, and removes it. Never contacts Postmark. |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). [`AGENTS.md`](AGENTS.md) is the binding engineering contract.

## Licence

Licensed under the Apache License, Version 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
