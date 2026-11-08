//! The real launcher, copied under the server's name, starting the server from an
//! active release set. The server here is a shell script, so these run where a
//! script is an executable.
#![cfg(unix)]

mod isolation;

use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::{TempDir, tempdir};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Serialises these tests. They write programs and then run them, and a process
/// started by another test at that moment inherits the open file, which makes the
/// kernel refuse to run it with "Text file busy".
static EXECUTABLES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    EXECUTABLES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

const RELEASE: &str = r#"{
  "schema_version": 1,
  "product": "postmark-mcp",
  "version": "1.0.0",
  "target": "x86_64-unknown-linux-gnu",
  "published_at": "2026-01-01T00:00:00Z",
  "minimum_launcher_version": "0.1.0",
  "commands": [{ "name": "postmark-mcp", "path": "bin/postmark-mcp" }]
}"#;

/// An installation whose active release's `postmark-mcp` is `script`, and the
/// launcher copied to `path/postmark-mcp`, as an install places it.
fn installed(script: &str) -> TestResult<(TempDir, PathBuf)> {
    let directory = tempdir()?;
    let release = directory.path().join("root").join("versions").join("1.0.0");
    fs::create_dir_all(release.join("bin"))?;
    fs::write(release.join("release.json"), RELEASE)?;
    let program = release.join("bin").join("postmark-mcp");
    fs::write(&program, format!("#!/bin/sh\n{script}\n"))?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    symlink("versions/1.0.0", directory.path().join("root").join("current"))?;

    let launcher = directory.path().join("path").join("postmark-mcp");
    fs::create_dir_all(directory.path().join("path"))?;
    fs::copy(env!("CARGO_BIN_EXE_postmark-mcp-launcher"), &launcher)?;
    Ok((directory, launcher))
}

fn launcher_command(directory: &Path, launcher: &Path) -> Command {
    let mut command = Command::new(launcher);
    isolation::isolate(&mut command)
        .env("HOME", directory.join("home"))
        .env("POSTMARK_MCP_INSTALL_ROOT", directory.join("root"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn launch(directory: &Path, launcher: &Path, args: &[&str]) -> TestResult<(u32, Output)> {
    let child = launcher_command(directory, launcher).args(args).spawn()?;
    let id = child.id();
    Ok((id, child.wait_with_output()?))
}

// An MCP host stops a stdio server by closing its input or signalling the process
// it started. If that were a launcher waiting on the server, the launcher would
// die at once and leave the server to be killed without shutting down cleanly.
#[test]
fn the_server_takes_over_the_launchers_process() -> TestResult {
    let _exclusive = exclusive();
    let (directory, launcher) = installed(r#"printf '%s' "$$""#)?;

    let (id, output) = launch(directory.path(), &launcher, &[])?;

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout)?, id.to_string());
    Ok(())
}

#[test]
fn the_servers_exit_status_is_the_launchers() -> TestResult {
    let _exclusive = exclusive();
    let (directory, launcher) = installed("exit 7")?;

    let (_, output) = launch(directory.path(), &launcher, &[])?;

    assert_eq!(output.status.code(), Some(7));
    Ok(())
}

// MCP traffic travels over the server's standard input and output, so the
// launcher must add nothing to either.
#[test]
fn standard_input_and_output_reach_the_server_untouched() -> TestResult {
    let _exclusive = exclusive();
    let (directory, launcher) = installed(r#"read -r line; printf 'reply to %s' "$line""#)?;

    let mut child = launcher_command(directory.path(), &launcher)
        .stdin(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(b"{\"jsonrpc\":\"2.0\"}\n")?;
    let output = child.wait_with_output()?;

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout)?, "reply to {\"jsonrpc\":\"2.0\"}");
    assert_eq!(String::from_utf8(output.stderr)?, "");
    Ok(())
}

// `update` belongs to the launcher under its own name only. Under the server's
// name every argument is the server's, so none of its flags can be taken over.
#[test]
fn under_the_servers_name_every_argument_goes_to_the_server() -> TestResult {
    let _exclusive = exclusive();
    let (directory, launcher) = installed(r#"printf '%s|' "$@""#)?;

    let (_, output) = launch(
        directory.path(),
        &launcher,
        &["update", "--yes", "--server-token-file", "/a token"],
    )?;

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        "update|--yes|--server-token-file|/a token|"
    );
    Ok(())
}

#[test]
fn with_nothing_installed_the_server_name_says_how_to_install() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;
    let launcher = directory.path().join("postmark-mcp");
    fs::copy(env!("CARGO_BIN_EXE_postmark-mcp-launcher"), &launcher)?;

    let (_, output) = launch(directory.path(), &launcher, &[])?;

    assert!(!output.status.success());
    assert_eq!(String::from_utf8(output.stdout)?, "");
    assert!(
        String::from_utf8(output.stderr)?.contains("postmark-mcp-launcher install"),
        "no guidance"
    );
    Ok(())
}

// A host may be pointed at a symbolic link named `postmark-mcp`. The link's own
// file resolves to the launcher's name, so the name it was started by decides.
#[test]
fn a_symbolic_link_under_the_servers_name_starts_the_server() -> TestResult {
    let _exclusive = exclusive();
    let (directory, launcher) = installed("printf 'served'")?;
    let renamed = directory.path().join("path").join("postmark-mcp-launcher");
    fs::rename(&launcher, &renamed)?;
    symlink(&renamed, &launcher)?;

    let (_, output) = launch(directory.path(), &launcher, &[])?;

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout)?, "served");
    Ok(())
}
