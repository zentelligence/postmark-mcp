#![forbid(unsafe_code)]

//! The stable launcher, placed on `PATH` as `postmark-mcp` and as
//! `postmark-mcp-launcher`: the same binary, copied under each name. What it does
//! depends on its own file name.
//!
//! - Under the server's name it resolves the active release and becomes the
//!   server, passing on every argument. An MCP host's entry naming this copy
//!   follows every update and rollback.
//! - Under any other name it installs, updates, rolls back, and removes the
//!   server. That name is normally `postmark-mcp-launcher`, but a downloaded copy
//!   may be `postmark-mcp-launcher (1)`, or whatever its owner chose. `update` also
//!   replaces the launcher's copies, which is how fixes to this code reach an
//!   installation.

mod confirm;
mod failure;
mod host;
mod installation;
mod lifecycle;
mod manage;
mod product;
mod remote;
mod report;
mod settings;
#[cfg(test)]
mod test_support;
mod uninstall;

use std::ffi::OsString;
use std::process::{Command, ExitCode};

use updater_launcher::{LauncherError, resolve_command};

use crate::failure::Failure;
use crate::installation::Installation;
use crate::product::{LAUNCHER_NAME, SERVER_NAME};

fn main() -> ExitCode {
    if invoked_as_server() {
        start_server()
    } else {
        manage::run()
    }
}

/// Starts the active release's server. Stdout belongs to MCP traffic, so every
/// problem is reported on stderr.
fn start_server() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let resolved = Installation::detect()
        .and_then(|installation| Ok(resolve_command(installation.installation_root(), SERVER_NAME)?));
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(Failure::Launcher(LauncherError::NoActiveRelease { .. })) => {
            eprintln!("{SERVER_NAME}: no release is installed. Run '{LAUNCHER_NAME} install' to install it.");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("{SERVER_NAME}: {error}. Run '{LAUNCHER_NAME} install' to repair the installation.");
            return ExitCode::FAILURE;
        }
    };

    let mut command = Command::new(&resolved.executable_path);
    command.args(args);
    become_server(command)
}

/// Becomes the server. An MCP host stops a stdio server by closing its input or
/// signalling the process it started, so that process must be the server itself,
/// not a launcher that would leave it to be killed.
#[cfg(unix)]
fn become_server(mut command: Command) -> ExitCode {
    use std::os::unix::process::CommandExt;

    let error = command.exec();
    eprintln!("{SERVER_NAME}: the installed server could not be started: {error}");
    ExitCode::FAILURE
}

/// Windows cannot replace a running process, so the launcher waits, with the
/// server inheriting its standard streams, and passes on the exit status.
#[cfg(not(unix))]
fn become_server(mut command: Command) -> ExitCode {
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => {
            eprintln!("{SERVER_NAME}: the installed server could not be started: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Whether this process was started under the server's name, either as the
/// name it was invoked by or as its file's own name. The first covers a symbolic
/// link named `postmark-mcp`, which the second resolves away on Linux.
fn invoked_as_server() -> bool {
    let invoked = std::env::args_os().next().map(std::path::PathBuf::from);
    let file = std::env::current_exe().ok();
    [invoked, file]
        .into_iter()
        .flatten()
        .any(|path| path.file_stem().is_some_and(|stem| stem == SERVER_NAME))
}
