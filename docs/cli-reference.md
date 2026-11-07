# CLI Reference

```text
postmark-mcp [OPTIONS] [COMMAND]
```

With no command, the server validates its configuration, confirms the token with Postmark, and serves MCP over stdio until the host closes the connection.

Every setting is a flag. [`configuration.md`](configuration.md) describes each one, and the environment variable it falls back to. `postmark-mcp --help` lists them.

| Option | Description |
| --- | --- |
| `--version` | Print the version. |
| `--help` | Print help. |

## `doctor`

Validates configuration and checks the token against Postmark, then exits. On success it prints the name and ID of the server the token belongs to. On failure it explains the problem on stderr and exits with a non-zero status.

This always contacts Postmark, even with `--skip-verify`. Flags work the same before or after `doctor`.
