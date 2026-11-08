//! `postmark-mcp-launcher` under its own name: the program that installs, updates,
//! and removes the server. A real install needs the real release server, so these
//! stop before any request leaves the machine, or start from an installed fixture.

mod isolation;

use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::tempdir;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Serialises every test here. Some copy the launcher and then run the copy, and
/// a process another test starts at that moment inherits the copy's open file,
/// which makes the kernel refuse to run it with "Text file busy".
static EXECUTABLES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    EXECUTABLES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The launcher in an empty environment apart from a home and an installation
/// root inside `directory`.
fn launcher_in(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_postmark-mcp-launcher"));
    isolation::isolate(&mut command)
        .env("HOME", directory.join("home"))
        .env("USERPROFILE", directory.join("home"))
        .env("LOCALAPPDATA", directory.join("local"))
        .env("POSTMARK_MCP_INSTALL_ROOT", directory.join("root"))
        .stdin(Stdio::null());
    command
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Whether anything at all was written under `directory` by the run.
fn untouched(directory: &Path) -> bool {
    !directory.join("root").exists() && !directory.join("home").exists()
}

#[test]
fn the_version_flag_prints_the_launchers_name_and_the_bare_version() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path()).arg("--version").output()?;

    assert!(output.status.success());
    assert_eq!(
        text(&output.stdout),
        format!("postmark-mcp-launcher {}\n", env!("CARGO_PKG_VERSION"))
    );
    Ok(())
}

// A browser saves a second download as `postmark-mcp-launcher (1)`, and people
// rename files. Only the server's own name starts the server, so such a copy
// still manages the installation.
#[test]
fn a_renamed_launcher_still_manages_the_installation() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;
    for name in ["postmark-mcp-launcher (1)", "postmark-mcp-launcher-linux", "launcher"] {
        let renamed = directory.path().join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(env!("CARGO_BIN_EXE_postmark-mcp-launcher"), &renamed)?;

        let mut command = Command::new(&renamed);
        let output = isolation::isolate(&mut command).arg("--version").output()?;

        assert!(output.status.success(), "{name}: {}", text(&output.stderr));
        assert_eq!(
            text(&output.stdout),
            format!("postmark-mcp-launcher {}\n", env!("CARGO_PKG_VERSION")),
            "{name}"
        );
    }
    Ok(())
}

// Releases are only trusted over HTTPS, so a plain address is refused before any
// request is made or anything is written.
#[test]
fn install_refuses_an_update_address_that_is_not_https() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path())
        .arg("install")
        .env("POSTMARK_MCP_UPDATE_URL", "http://127.0.0.1:9/latest.json")
        .output()?;

    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("is not an HTTPS url"), "{stderr}");
    assert_eq!(text(&output.stdout), "");
    assert!(untouched(directory.path()));
    Ok(())
}

#[test]
fn install_refuses_a_malformed_pinned_version_before_any_request() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path())
        .args(["install", "--version", "v1.0"])
        .env("POSTMARK_MCP_UPDATE_URL", "http://127.0.0.1:9/latest.json")
        .output()?;

    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("--version"), "{stderr}");
    assert!(!stderr.contains("HTTPS"), "a request was attempted: {stderr}");
    assert!(untouched(directory.path()));
    Ok(())
}

// An empty variable counts as unset across this server's configuration, but here
// that would quietly switch to the default address, so it is refused instead.
#[test]
fn an_empty_update_address_is_refused() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path())
        .args(["update", "check"])
        .env("POSTMARK_MCP_UPDATE_URL", "")
        .output()?;

    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("POSTMARK_MCP_UPDATE_URL"), "{stderr}");
    assert!(untouched(directory.path()));
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_update_address_that_is_not_unicode_is_refused() -> TestResult {
    use std::os::unix::ffi::OsStrExt;

    let directory = tempdir()?;

    let output = launcher_in(directory.path())
        .args(["update", "check"])
        .env(
            "POSTMARK_MCP_UPDATE_URL",
            std::ffi::OsStr::from_bytes(b"https://example.com/\xff"),
        )
        .output()?;

    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("POSTMARK_MCP_UPDATE_URL"), "{stderr}");
    assert!(untouched(directory.path()));
    Ok(())
}

/// An HTTPS address on loopback where nothing is listening.
fn unreachable() -> TestResult<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(format!("https://127.0.0.1:{port}/latest.json"))
}

#[test]
fn update_check_changes_nothing_when_the_manifest_cannot_be_reached() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path())
        .args(["update", "check"])
        .env("POSTMARK_MCP_UPDATE_URL", unreachable()?)
        .output()?;

    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("failed to retrieve"), "{stderr}");
    assert!(untouched(directory.path()));
    Ok(())
}

#[test]
fn install_changes_nothing_when_the_manifest_cannot_be_reached() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path())
        .arg("install")
        .env("POSTMARK_MCP_UPDATE_URL", unreachable()?)
        .output()?;

    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert!(untouched(directory.path()));
    Ok(())
}

// POSTMARK_MCP_INSTALL_ROOT can name any folder. One that holds anything but
// an installation is never written to, and never removed.
#[test]
fn a_folder_that_is_not_an_installation_is_neither_installed_into_nor_removed() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;
    let notes = directory.path().join("root").join("notes.txt");
    std::fs::create_dir_all(directory.path().join("root"))?;
    std::fs::write(&notes, "mine")?;

    let uninstall = launcher_in(directory.path()).args(["uninstall", "--yes"]).output()?;
    let install = launcher_in(directory.path())
        .arg("install")
        .env("POSTMARK_MCP_UPDATE_URL", unreachable()?)
        .output()?;

    for output in [&uninstall, &install] {
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains("not a postmark-mcp installation"), "{stderr}");
    }
    assert_eq!(std::fs::read_to_string(&notes)?, "mine");
    assert_eq!(std::fs::read_dir(directory.path().join("root"))?.count(), 1);
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_installation_folder_that_is_not_unicode_is_refused_rather_than_ignored() -> TestResult {
    use std::os::unix::ffi::OsStrExt;

    let _exclusive = exclusive();
    let directory = tempdir()?;
    let mut root = directory.path().as_os_str().to_owned();
    root.push(std::ffi::OsStr::from_bytes(b"/\xff"));

    let output = launcher_in(directory.path())
        .args(["update", "diagnostics"])
        .env("POSTMARK_MCP_INSTALL_ROOT", root)
        .output()?;

    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("POSTMARK_MCP_INSTALL_ROOT"), "{stderr}");
    Ok(())
}

#[test]
fn uninstall_with_nothing_installed_says_so() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path()).args(["uninstall", "--yes"]).output()?;

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "postmark-mcp is not installed.\n");
    Ok(())
}

#[test]
fn rollback_with_nothing_installed_fails_and_changes_nothing() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path()).arg("rollback").output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).starts_with("error: "), "{}", text(&output.stderr));
    assert!(untouched(directory.path()));
    Ok(())
}

#[test]
fn an_unknown_command_is_refused_with_usage() -> TestResult {
    let _exclusive = exclusive();
    let directory = tempdir()?;

    let output = launcher_in(directory.path()).arg("upgrade").output()?;

    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("Usage"), "{}", text(&output.stderr));
    assert!(untouched(directory.path()));
    Ok(())
}

#[cfg(unix)]
mod installed {
    use std::fmt::Write;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};

    use sha2::{Digest, Sha256};
    use tempfile::{TempDir, tempdir};

    use super::{TestResult, exclusive, launcher_in, text};

    const RELEASE: &str = r#"{
      "schema_version": 1,
      "product": "postmark-mcp",
      "version": "1.0.0",
      "target": "x86_64-unknown-linux-gnu",
      "published_at": "2026-01-01T00:00:00Z",
      "minimum_launcher_version": "0.1.0",
      "commands": [{ "name": "postmark-mcp", "path": "bin/postmark-mcp" }]
    }"#;
    const PROFILE_BLOCK: &str = "\n# Added by Postmark MCP\nexport PATH=\"$HOME/.local/bin:$PATH\"\n";
    const NAMES: [&str; 2] = ["postmark-mcp", "postmark-mcp-launcher"];

    struct Fixture {
        directory: TempDir,
    }

    impl Fixture {
        fn home(&self) -> PathBuf {
            self.directory.path().join("home")
        }

        fn root(&self) -> PathBuf {
            self.directory.path().join("root")
        }

        fn commands(&self) -> PathBuf {
            self.home().join(".local").join("bin")
        }

        fn uninstall(&self, confirmation: Option<&str>) -> TestResult<Output> {
            let mut command = launcher_in(self.directory.path());
            command.env("SHELL", "/bin/zsh").arg("uninstall");
            match confirmation {
                None => {
                    command.arg("--yes");
                    Ok(command.output()?)
                }
                Some(answer) => answered(&mut command, answer),
            }
        }
    }

    fn answered(command: &mut Command, answer: &str) -> TestResult<Output> {
        use std::io::Write as _;

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().ok_or("no stdin")?.write_all(answer.as_bytes())?;
        Ok(child.wait_with_output()?)
    }

    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
    }

    /// A complete installation: a release, the launcher's copies, the profile
    /// line, and the record of what the installer placed.
    fn fixture() -> TestResult<Fixture> {
        let fixture = Fixture { directory: tempdir()? };
        let release = fixture.root().join("versions").join("1.0.0");
        fs::create_dir_all(release.join("bin"))?;
        fs::write(release.join("release.json"), RELEASE)?;
        let program = release.join("bin").join("postmark-mcp");
        fs::write(&program, "#!/bin/sh\n")?;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
        symlink("versions/1.0.0", fixture.root().join("current"))?;

        fs::create_dir_all(fixture.commands())?;
        let mut launchers = Vec::new();
        for name in NAMES {
            let path = fixture.commands().join(name);
            fs::write(&path, b"a launcher")?;
            launchers.push(serde_json::json!({ "path": path, "sha256": digest(b"a launcher") }));
        }
        let profile = fixture.home().join(".zshrc");
        fs::write(&profile, format!("alias ll='ls -l'\n{PROFILE_BLOCK}"))?;
        fs::write(
            fixture.root().join("launchers.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "launchers": launchers,
                "profiles": [profile],
                "path_folder": null
            }))?,
        )?;
        Ok(fixture)
    }

    fn exists(path: &Path) -> bool {
        path.symlink_metadata().is_ok()
    }

    #[test]
    fn uninstall_removes_the_commands_the_path_line_and_the_releases() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;

        let output = fixture.uninstall(None)?;

        assert!(output.status.success(), "{}", text(&output.stderr));
        for name in NAMES {
            assert!(!exists(&fixture.commands().join(name)), "{name}");
        }
        assert!(!exists(&fixture.root()));
        assert_eq!(fs::read_to_string(fixture.home().join(".zshrc"))?, "alias ll='ls -l'\n");
        assert!(text(&output.stdout).contains("postmark-mcp has been removed."));
        Ok(())
    }

    // The token file and the host's configuration are the operator's, and the
    // launcher cannot know which hosts were set up, so it says what remains.
    #[test]
    fn uninstall_keeps_the_token_file_and_says_hosts_need_updating() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;
        let token = fixture.home().join(".config").join("postmark-mcp").join("token");
        fs::create_dir_all(token.parent().ok_or("no parent")?)?;
        fs::write(&token, "a token")?;

        let output = fixture.uninstall(None)?;

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert_eq!(fs::read_to_string(token)?, "a token");
        let stdout = text(&output.stdout);
        assert!(stdout.contains("MCP host"), "{stdout}");
        assert!(!stdout.contains("a token"), "{stdout}");
        Ok(())
    }

    // A file changed since install is no longer the launcher placed there.
    #[test]
    fn uninstall_leaves_a_command_that_is_no_longer_the_one_it_placed() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;
        let replaced = fixture.commands().join("postmark-mcp");
        fs::write(&replaced, b"something of the operator's")?;
        let unrelated = fixture.commands().join("another-tool");
        fs::write(&unrelated, b"another tool")?;

        let output = fixture.uninstall(None)?;

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert_eq!(fs::read(replaced)?, b"something of the operator's");
        assert_eq!(fs::read(unrelated)?, b"another tool");
        assert!(text(&output.stdout).contains("left as it is"));
        Ok(())
    }

    #[test]
    fn uninstall_asks_first_and_declining_changes_nothing() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;

        let output = fixture.uninstall(Some("n\n"))?;

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(text(&output.stdout).contains("Uninstall cancelled."));
        assert!(exists(&fixture.commands().join("postmark-mcp")));
        assert!(exists(&fixture.root()));
        Ok(())
    }

    #[test]
    fn uninstall_proceeds_when_confirmed() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;

        let output = fixture.uninstall(Some("yes\n"))?;

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(!exists(&fixture.root()));
        Ok(())
    }

    #[test]
    fn rollback_with_no_previous_release_leaves_the_active_one() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;

        let output = launcher_in(fixture.directory.path()).arg("rollback").output()?;

        assert_eq!(output.status.code(), Some(1));
        assert!(
            text(&output.stderr).contains("no previous release"),
            "{}",
            text(&output.stderr)
        );
        assert_eq!(
            fs::read_link(fixture.root().join("current"))?,
            Path::new("versions/1.0.0")
        );
        Ok(())
    }

    #[test]
    fn diagnostics_reports_the_active_release_without_changing_it() -> TestResult {
        let _exclusive = exclusive();
        let fixture = fixture()?;

        let output = launcher_in(fixture.directory.path())
            .args(["update", "diagnostics"])
            .output()?;

        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        assert!(stdout.contains("product: postmark-mcp"), "{stdout}");
        assert!(stdout.contains("active_version: 1.0.0"), "{stdout}");
        assert!(exists(&fixture.commands().join("postmark-mcp")));
        Ok(())
    }
}
