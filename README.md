# Postmark MCP

A Rust [Model Context Protocol](https://modelcontextprotocol.io) server for the [Postmark](https://postmarkapp.com) email API. It gives AI assistants structured, safety-conscious access to sending, templates, delivery diagnostics, and more, as a single self-contained binary.

This is an independent project, not affiliated with ActiveCampaign or Postmark. Its tool surface and configuration follow ActiveCampaign's official [`postmark-mcp`](https://github.com/ActiveCampaign/postmark-mcp).

## Status

Early development. The server runs over stdio and offers twelve read-only tools covering templates, messages, bounces, suppressions, statistics, webhooks, and delivery diagnosis. It cannot send mail or change anything yet. See [`docs/mcp-tools.md`](docs/mcp-tools.md) for the tool catalogue, and [`docs/roadmap.md`](docs/roadmap.md) for what comes next.

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
- Webhook passwords and custom headers never reach a tool result, and credentials, query strings, and fragments are removed from webhook URLs. A secret in a URL's path cannot be detected, and is not removed.
- Message bodies and clicked link URLs are not returned. The exception is `get_bounce_dump`, which returns a bounce's raw source, and that can quote the original message.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `postmark-mcp-core` | Validated values, result types, and ports. No adapter dependencies. |
| `postmark-mcp-client` | Hardened HTTP transport, and the port implementations. |
| `postmark-mcp` | Configuration, the MCP server, the tool catalogue, and the binary. |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). [`AGENTS.md`](AGENTS.md) is the binding engineering contract.

## Licence

Licensed under the Apache License, Version 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
