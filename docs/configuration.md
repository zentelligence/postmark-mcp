# Configuration

Every setting is a command-line flag, given in the `args` of your MCP host's entry for this server. Every host can pass arguments to the server it launches, while not every host can set environment variables, so flags are the reliable route.

Each setting also falls back to an environment variable, used only when its flag is absent. Variable names match the official `postmark-mcp` server where one exists there, so an existing host entry that sets them works unchanged. There is no configuration file, and `.env` files are not loaded.

An empty variable counts as unset, since hosts often template an `env` block and leave entries blank. An empty flag, such as `--recipient-allowlist ""`, is refused instead, because it would otherwise hide its variable, which for an allowlist would silently lift the restriction.

`POSTMARK_READ_ONLY`, from version 0.4.0, is no longer read. The server refuses to start while it is set, rather than ignore it and offer every tool. Use `--toolsets read` instead.

## Example

```json
{
  "mcpServers": {
    "postmark": {
      "command": "postmark-mcp",
      "args": [
        "--server-token-file", "/home/you/.config/postmark-mcp/token",
        "--default-sender", "support@example.com",
        "--default-message-stream", "outbound",
        "--toolsets", "read,send"
      ]
    }
  }
}
```

If you installed with `postmark-mcp-launcher`, give the full path it printed, such as `/home/you/.local/bin/postmark-mcp`, as the `command`, since not every host searches your `PATH`. That path follows every update and rollback.

## Required

| Flag | Variable | Description |
| --- | --- | --- |
| `--server-token` or `--server-token-file` | `POSTMARK_SERVER_TOKEN` | Server API token, from your server's API Tokens tab, which scopes every tool to that one server. See [The token](#the-token). |
| `--default-sender` | `DEFAULT_SENDER_EMAIL` | Sender used when a tool call does not supply one, as `sender@example.com` or `Name <sender@example.com>`. Must have a verified Sender Signature. |
| `--default-message-stream` | `DEFAULT_MESSAGE_STREAM` | Message stream ID used when a tool call does not supply one, typically `outbound`. Applies to the message, bounce, send, and suppression tools. Statistics and the webhook list cover all streams unless one is requested. |

## The token

There are three ways to give it, in order of preference:

- `--server-token-file <path>` reads it from a file, trimming surrounding whitespace. Restrict the file to your own user, for example with `chmod 600`. The token then appears in neither the process list nor the host's configuration.
- `POSTMARK_SERVER_TOKEN`, where the host can set environment variables.
- `--server-token <token>` gives it directly. This is the simplest, but the token is visible to other users of the machine through the process list, and stored in plain text in the host's configuration.

`--server-token` and `--server-token-file` cannot be combined. A token file must be non-empty and at most 4 KB.

Give `--server-token` and its value as two separate arguments in the host's `args`. If the command line cannot be read and the token flag appears in it, the server names the flag at fault but withholds the rest, since it may include the token.

## Toolsets

| Flag | Variable | Default |
| --- | --- | --- |
| `--toolsets` | `POSTMARK_TOOLSETS` | `read,send,manage` |

A comma-separated list of the tool groups to offer:

| Group | Tools |
| --- | --- |
| `read` | The twelve tools that only read, including `diagnose_delivery`. |
| `send` | The four send tools. |
| `manage` | The eight tools that change templates, suppressions, bounces, and webhooks. |

A tool outside the chosen groups is left out of the catalogue entirely, so it cannot be listed or called. An unknown group, or an empty flag, stops the server rather than being ignored.

For example, an agent that sends automated sequences might use `read,send`, so it can send and check delivery but cannot change templates or lift suppressions. A host's own tool permissions can narrow this further, but can never add a tool the server leaves out.

## Optional

| Flag | Variable | Default | Description |
| --- | --- | --- | --- |
| `--agent-label` | `AGENT_LABEL` | unset | Label sent as `X-Agent-Label` on every request, to identify this instance. At most 128 visible ASCII characters. |
| `--api-base-url` | `POSTMARK_API_BASE_URL` | `https://api.postmarkapp.com/` | API origin. Must be a bare origin using `https`, or `http` on a loopback address for local testing. The server token is sent here. |
| `--skip-verify` | `POSTMARK_SKIP_VERIFY` | off | Skips checking the token against Postmark at startup. The variable must be `true` or `false`. |

### Recipient allowlist

| Flag | Variable | Default |
| --- | --- | --- |
| `--recipient-allowlist` | `POSTMARK_RECIPIENT_ALLOWLIST` | unset |

Comma-separated addresses, such as `name@example.com`, and whole domains, such as `@example.com`, that mail may be sent to. Every `to`, `cc`, and `bcc` recipient must match, or nothing is sent. A domain entry does not cover its subdomains. Matching ignores case.

The allowlist is for deployments whose recipients are known in advance: a staging or development server that must only reach your team, an agent that only sends within your own domain, or a demonstration. It cannot protect sending to the public, such as customers or workshop attendees, since those addresses cannot be listed ahead of time. Leave it unset there.

Unset or empty, it places no restriction. A malformed entry stops the server at startup, rather than being skipped.

### Webhook URL allowlist

| Flag | Variable | Default |
| --- | --- | --- |
| `--webhook-url-allowlist` | `WEBHOOK_URL_ALLOWLIST` | unset |

Comma-separated HTTPS URLs that `create_webhook` may register under. A URL is permitted if its origin matches an entry's and its path is the entry's path or below it, by whole segments: `https://hooks.example.com/postmark` permits `https://hooks.example.com/postmark/events`, but not `https://hooks.example.com/postmark-other`. An entry with no path permits the whole origin. A refusal does not reveal the list.

Unset, any HTTPS URL without credentials may be registered. A malformed entry stops the server at startup.

### Logging

| Flag | Variable | Default | Description |
| --- | --- | --- | --- |
| `--log-file` | `LOG_FILE` | unset | File that each tool call is appended to, as one JSON line, as well as stderr. It is created with owner-only permissions on Unix. The server will not start if it cannot open the file. Never point it at standard output, which carries MCP traffic. |
| `--log-email-full` | `LOG_EMAIL_FULL` | off | Logs addresses in full rather than masked. The variable must be `true` or `false`. |
| `--log-level` | none | `info` | One of `error`, `warn`, `info`, `debug`, or `trace`. |

Each tool call entry records the tool, `ok` or `error`, the failure code if any, the duration, and the arguments. Only arguments that describe a request, such as `tag`, `count`, or `template_id`, are logged as given. Addresses are masked as `u**r@example.com`, with any display name dropped, and every other argument, including subjects, bodies, template models, and anything under a name the server does not expect, is reduced to its size.

Keep `--log-level` at `info` in production: at `debug` and below, the MCP SDK logs protocol messages, which include tool arguments such as email content, without that masking.

## Startup checks

The server validates every setting, then calls Postmark to confirm the token, and exits with a non-zero status and an explanation on stderr if either fails. Each error names both the flag and the variable, and never repeats a token. `postmark-mcp doctor` runs the same checks without serving.

The server exits with status 0 when the client closes the connection, including when it closes without using the server, either at once or after only `server/discover`. Some hosts start every configured server and close those a session did not use, so this is a normal shutdown, logged at INFO.
