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

## `postmark-mcp-launcher`

```text
postmark-mcp-launcher <COMMAND>
```

Installs, updates, and removes the server. It is also placed on your `PATH` as `postmark-mcp`, where it starts the active release, passing every argument and its standard input and output to the server unchanged. Under any other name it is the command below. [`installation.md`](installation.md) describes what each command places and removes.

| Command | Description |
| --- | --- |
| `install [--version <version>]` | Installs the current release and puts the commands on your `PATH`. `--version` must name the current release, such as `0.6.0`, and installs it even over a newer one. Running it again sets the commands up again without reinstalling, and keeps a newer installed release. |
| `update [--yes]` | Installs a newer release, if there is one, asking first unless given `--yes`, and replaces the launcher's copies with the new release's. |
| `update check` | Reports whether a newer release is available, changing nothing. |
| `update cleanup` | Deletes releases other than the active one and the one before it, keeping any still in use. |
| `update diagnostics` | Describes the installation: its folder, platform, update address, and active and previous releases. |
| `rollback` | Makes the release before the last update active again. |
| `uninstall [--yes]` | Removes the commands, the `PATH` changes, and what an install created in the installation folder, asking first unless given `--yes`. Your token file is kept. |

Outcomes print on stdout. A failure prints `error:` and an explanation on stderr, and exits with status 1, including an uninstall that could not remove everything, and an install or update whose release is active but whose commands could not be set up. An unknown command or flag exits with status 2. Each change waits for any other launcher's change to finish first, and every command refuses an installation folder that holds anything but an installation.
