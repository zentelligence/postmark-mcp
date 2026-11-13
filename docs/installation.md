# Installation

`postmark-mcp` ships with `postmark-mcp-launcher`, which installs, updates, and removes it, and which goes on your `PATH` under the server's name to start the active release. No administrator rights are needed on any platform.

## From a release

Download the archive for your platform from [Releases](https://github.com/zentelligence/postmark-mcp/releases) and unpack it. From the unpacked folder:

```sh
./bin/postmark-mcp-launcher install
```

On Windows, run `bin\postmark-mcp-launcher.exe install`. Then open a new terminal.

`install` downloads the current release, checks it against a signed manifest and its checksum, confirms that the server it contains reports the expected version, and only then makes it active. Releases are kept in `~/.local/share/postmark-mcp` on Linux and macOS, or `%LOCALAPPDATA%\postmark-mcp` on Windows. Running `install` again sets the commands up again without reinstalling, and keeps an installed release that is newer than the current one.

The commands are copies of the launcher named `postmark-mcp` and `postmark-mcp-launcher`, in `~/.local/bin` on Linux and macOS, or `%LOCALAPPDATA%\postmark-mcp\bin` on Windows. The launcher behaves according to its file name: as `postmark-mcp` it becomes the active release's server, passing on every argument, and under any other name it manages the installation, so a downloaded copy saved as `postmark-mcp-launcher (1)` still works.

If that folder is not on your `PATH`, `install` adds it: in `~/.zshrc` for zsh, in `~/.bashrc` and `~/.bash_profile` or `~/.profile` for bash, and in the user `Path` on Windows. For any other shell it tells you which folder to add. Everything placed outside the installation folder is recorded in `launchers.json` there, so it can be removed later. A file with a command's name that an install did not place is left alone and reported.

Then [configure the server](configuration.md), check it with `postmark-mcp doctor`, and give your MCP host the full path `install` printed, such as `/home/you/.local/bin/postmark-mcp`, as the `command`. Some hosts do not search your `PATH`. That path keeps working across updates and rollbacks, because the launcher there always starts the active release.

### For automation

`--version` installs an exact release, such as `postmark-mcp-launcher install --version 0.6.2`, and fails unless it is the current release, so a pipeline cannot silently install something else. If a newer release is published while it installs, the newer one ends up active and `install` fails, saying so. It also installs the current release over a newer one, which plain `install` never does.

| Variable | Effect |
| --- | --- |
| `POSTMARK_MCP_INSTALL_ROOT` | Keeps the releases in another folder, which must be new, empty, or an existing installation: the launcher refuses a folder holding anything else. Every launcher command must then be given the same value. |
| `POSTMARK_MCP_UPDATE_URL` | Fetches the signed manifest from another HTTPS address, such as a release that is not yet the latest. It must still be signed with a key built into the launcher. An empty value is refused, rather than taken as unset. |

## Updating

```sh
postmark-mcp-launcher update check
postmark-mcp-launcher update
postmark-mcp-launcher rollback
```

`update check` reports whether a newer release is available, and changes nothing. `update` asks before installing it, unless given `--yes`, then replaces the launcher's copies with the new release's, so fixes to the launcher arrive the same way. The release before it is kept, and `rollback` makes it active again. After a rollback, `update` returns to the newer release without downloading it again.

An update never replaces the files of a running server. A server already running keeps its release until its MCP host restarts it, and new starts use the new release. Restart the host, or reconnect the server in it, to switch.

`update cleanup` deletes releases other than the active one and the one before it. A release still in use is kept, and removed by a later cleanup. `update diagnostics` describes the installation.

Two launchers never change an installation at once: each change waits for the one before it to finish. `update` asks its question before waiting, and stops without installing anything if another launcher changed the installation meanwhile.

## Uninstalling

```sh
postmark-mcp-launcher uninstall
```

`uninstall` asks first, unless given `--yes`. It removes the launcher's copies, the `PATH` changes it made, and what an install created in the installation folder, then the folder itself if nothing else is in it. A command that has changed since it was placed is left and reported, as is anything else in your shell profile or the folder. If anything an install created cannot be removed, typically because a server is still running from it, `uninstall` says what is left and exits with status 1: close the MCP host and run it again. It refuses a folder that is not an installation.

Your token file is kept. The launcher cannot know which MCP hosts you set up, so remove the `postmark-mcp` entry from each one yourself.

## From source

Requires Rust 1.96 or later. Building also needs a C compiler for the TLS library, and CMake on some platforms.

```sh
cargo install --path crates/postmark-mcp
```

To build from a checkout without installing:

```sh
cargo build --workspace --release
```

The binaries are in `target/release`. A source build does not use the launcher or the updater, so update it the same way you installed it.

After installing, set the [required settings](configuration.md) and run `postmark-mcp doctor`.

## How releases are trusted

Every release is published with a manifest, `latest.json`, listing an archive and its SHA-256 checksum for each platform, and a detached Ed25519 signature over the manifest. The launcher verifies the signature against keys built into it before reading anything in the manifest, verifies the archive's checksum before unpacking it, refuses an archive entry that would land outside its folder, and runs the new server with `--version` before making it active. Each release is installed on every platform from its published manifest before it becomes the latest.

Every download starts at an HTTPS address. The updater follows redirects, as GitHub's downloads need, without refusing one to plain HTTP, and does not limit an archive's size. Neither can change what is installed, since an altered manifest fails its signature and an altered archive its checksum, but an interrupted or oversized download fails rather than installs.
