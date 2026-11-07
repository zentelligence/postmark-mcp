# Postmark MCP

A Rust [Model Context Protocol](https://modelcontextprotocol.io) server for the [Postmark](https://postmarkapp.com) email API. It gives AI assistants structured, safety-conscious access to sending, templates, delivery diagnostics, and more, as a single self-contained binary.

This is an independent project, not affiliated with ActiveCampaign or Postmark. Its tool surface and configuration follow ActiveCampaign's official [`postmark-mcp`](https://github.com/ActiveCampaign/postmark-mcp).

## Status

Early development. The server runs end to end over stdio, but exposes only one tool so far, `get_server_info`. See [`docs/roadmap.md`](docs/roadmap.md) for what comes next, and [`docs/mcp-tools.md`](docs/mcp-tools.md) for the tool catalogue.

## Installation

Requires Rust 1.96 or later.

```sh
cargo install --path crates/postmark-mcp
```

## Quick start

Set the three required variables, then check them against Postmark:

```sh
export POSTMARK_SERVER_TOKEN=your-server-token
export DEFAULT_SENDER_EMAIL=sender@example.com
export DEFAULT_MESSAGE_STREAM=outbound
postmark-mcp doctor
```

To use it from an MCP host, add an entry like this to the host's configuration:

```json
{
  "mcpServers": {
    "postmark": {
      "command": "postmark-mcp",
      "env": {
        "POSTMARK_SERVER_TOKEN": "your-server-token",
        "DEFAULT_SENDER_EMAIL": "sender@example.com",
        "DEFAULT_MESSAGE_STREAM": "outbound"
      }
    }
  }
}
```

See [`docs/configuration.md`](docs/configuration.md) for every variable.

## Security

- The server token is sent only to the configured API origin, and redirects are always refused.
- Every request has a timeout and a response size limit.
- Tokens never appear in logs, errors, or tool results, including the API tokens Postmark returns in its own responses.
- Credentials embedded in webhook URLs are removed before they reach a tool result.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `postmark-mcp-core` | Validated values, domain results, and ports. No adapter dependencies. |
| `postmark-mcp-client` | Hardened HTTP transport, Postmark wire types, and the port implementations. |
| `postmark-mcp` | Configuration, the MCP server, the tool catalogue, and the binary. |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). [`AGENTS.md`](AGENTS.md) is the binding engineering contract.

## Licence

Licensed under the Apache License, Version 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
