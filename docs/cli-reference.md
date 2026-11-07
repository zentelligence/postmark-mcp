# CLI Reference

```text
postmark-mcp [--log-level <LEVEL>] [COMMAND]
```

With no command, the server validates its configuration, confirms the token with Postmark, and serves MCP over stdio until the host closes the connection.

| Option | Description |
| --- | --- |
| `--log-level <LEVEL>` | One of `error`, `warn`, `info`, `debug`, or `trace`. Defaults to `info`. |
| `--version` | Print the version. |
| `--help` | Print help. |

## `doctor`

Validates configuration and checks the token against Postmark, then exits. On success it prints the name and ID of the server the token belongs to. On failure it explains the problem on stderr and exits with a non-zero status.

This always contacts Postmark, even when `POSTMARK_SKIP_VERIFY` is `true`.
