use std::fs;

use super::*;
use crate::lifecycle::install;
use crate::test_support::{TestResult, World};

#[test]
fn uninstall_removes_an_installation_and_its_commands() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;

    let report = run(&world.installation()?, &world.host(), true)?;

    assert!(!world.root().exists());
    assert!(
        !world
            .commands()
            .join(format!("postmark-mcp{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );
    assert!(report.contains("postmark-mcp has been removed."), "{report}");
    Ok(())
}

// POSTMARK_MCP_INSTALL_ROOT can name any folder, so a folder that is not an
// installation is never touched.
#[test]
fn uninstall_refuses_a_folder_that_is_not_an_installation() -> TestResult {
    let world = World::new()?;
    fs::create_dir_all(world.root().join("projects"))?;
    fs::write(world.root().join("projects").join("notes.txt"), "mine")?;

    let refused = run(&world.installation()?, &world.host(), true);

    assert!(matches!(refused, Err(Failure::NotAnInstallation { .. })), "{refused:?}");
    assert_eq!(
        fs::read_to_string(world.root().join("projects").join("notes.txt"))?,
        "mine"
    );
    Ok(())
}

#[test]
fn uninstall_keeps_files_it_did_not_create_and_the_folder_holding_them() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    fs::write(world.root().join("notes.txt"), "mine")?;

    let report = run(&world.installation()?, &world.host(), true)?;

    assert_eq!(fs::read_to_string(world.root().join("notes.txt"))?, "mine");
    assert!(!world.root().join("versions").exists());
    assert!(!world.root().join("launchers.json").exists());
    assert!(report.contains("notes.txt was not placed by an install"), "{report}");
    Ok(())
}

#[test]
fn declining_an_uninstall_changes_nothing() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    let world = world.declining();

    let report = run(&world.installation()?, &world.host(), false)?;

    assert_eq!(report, "Uninstall cancelled.");
    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    Ok(())
}

#[test]
fn uninstall_with_nothing_installed_says_so() -> TestResult {
    let world = World::new()?;

    let report = run(&world.installation()?, &world.host(), true)?;

    assert_eq!(report, "postmark-mcp is not installed.");
    Ok(())
}

// Something that cannot be removed, such as a release a running server holds on
// Windows, must not be reported as a clean removal.
#[cfg(unix)]
#[test]
fn an_uninstall_that_cannot_remove_everything_fails_and_says_what_is_left() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    let release = world.root().join("versions").join("1.2.0");
    fs::set_permissions(&release, fs::Permissions::from_mode(0o555))?;

    let outcome = run(&world.installation()?, &world.host(), true);
    fs::set_permissions(&release, fs::Permissions::from_mode(0o755))?;

    let Err(Failure::PartlyRemoved { report }) = outcome else {
        return Err(format!("expected a partial removal, got {outcome:?}").into());
    };
    assert!(report.contains("versions could not be removed"), "{report}");
    assert!(!report.contains("has been removed."), "{report}");
    Ok(())
}
