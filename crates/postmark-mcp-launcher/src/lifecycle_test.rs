use std::fs;

use super::*;
use crate::test_support::{TestResult, World};

fn pin(version: &str) -> TestResult<PinnedVersion> {
    Ok(PinnedVersion::try_from(version)?)
}

#[test]
fn a_fresh_install_activates_the_release_and_places_the_commands() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;

    let report = install(&world.installation()?, &world.host(), None)?;

    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    let launcher = world
        .commands()
        .join(format!("postmark-mcp-launcher{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(fs::read_to_string(launcher)?, "launcher 1.2.0\n");
    assert!(report.contains("Installed postmark-mcp 1.2.0."), "{report}");
    Ok(())
}

#[test]
fn installing_again_downloads_nothing() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;

    install(&world.installation()?, &world.host(), None)?;

    assert_eq!(world.releases.downloads(), 1);
    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    Ok(())
}

#[test]
fn a_pin_that_is_not_the_current_release_is_refused_before_anything_is_written() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;

    let refused = install(&world.installation()?, &world.host(), Some(&pin("1.1.0")?));

    assert!(
        matches!(refused, Err(Failure::PinnedVersionUnavailable { .. })),
        "{refused:?}"
    );
    assert!(!world.root().exists());
    Ok(())
}

#[test]
fn a_manifest_signed_by_an_unknown_key_is_refused_before_anything_is_written() -> TestResult {
    let world = World::signed_by_a_stranger()?;
    world.releases.publish("1.2.0")?;

    let refused = install(&world.installation()?, &world.host(), None);

    assert!(refused.is_err());
    assert!(!world.root().exists());
    assert_eq!(world.releases.downloads(), 0);
    Ok(())
}

// Installing again is how the commands are repaired, so it must never quietly
// move a newer installation back to an older current release.
#[test]
fn install_never_downgrades_unasked() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.3.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.2.0")?;

    let report = install(&world.installation()?, &world.host(), None)?;

    assert_eq!(world.active()?.as_deref(), Some("1.3.0"));
    assert_eq!(world.releases.downloads(), 1);
    assert!(report.contains("1.3.0"), "{report}");
    Ok(())
}

#[test]
fn a_pinned_install_may_return_to_the_current_release() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.3.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.2.0")?;

    install(&world.installation()?, &world.host(), Some(&pin("1.2.0")?))?;

    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    Ok(())
}

// The updater fetches the manifest again to install, so a release promoted in
// between would otherwise be installed in place of the one pinned.
#[test]
fn a_release_published_during_a_pinned_install_is_reported() -> TestResult {
    let world = World::new()?;
    world.releases.publish_in_turn(&["1.2.0", "1.3.0"])?;

    let outcome = install(&world.installation()?, &world.host(), Some(&pin("1.2.0")?));

    assert!(
        matches!(outcome, Err(Failure::InstalledOtherVersion { .. })),
        "{outcome:?}"
    );
    Ok(())
}

#[test]
fn install_refuses_a_folder_holding_other_files() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    fs::create_dir_all(world.root())?;
    fs::write(world.root().join("notes.txt"), "mine")?;

    let refused = install(&world.installation()?, &world.host(), None);

    assert!(matches!(refused, Err(Failure::NotAnInstallation { .. })), "{refused:?}");
    assert_eq!(fs::read_dir(world.root())?.count(), 1);
    assert_eq!(world.releases.downloads(), 0);
    Ok(())
}

// Once a release is active, failing to set up the commands must not read as a
// failed install.
#[cfg(unix)]
#[test]
fn a_command_set_up_failure_after_activation_says_the_release_is_active() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    fs::create_dir_all(world.home().join(".local"))?;
    fs::write(world.commands(), "not a folder")?;

    let outcome = install(&world.installation()?, &world.host(), None);

    assert!(matches!(outcome, Err(Failure::CommandsNotSetUp { .. })), "{outcome:?}");
    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    Ok(())
}

#[test]
fn update_installs_a_newer_release_and_replaces_the_launcher() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.3.0")?;

    let report = update(&world.installation()?, &world.host(), true)?;

    assert_eq!(world.active()?.as_deref(), Some("1.3.0"));
    let launcher = world
        .commands()
        .join(format!("postmark-mcp-launcher{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(fs::read_to_string(launcher)?, "launcher 1.3.0\n");
    assert!(report.contains("from 1.2.0 to 1.3.0"), "{report}");
    Ok(())
}

#[test]
fn update_when_up_to_date_downloads_nothing() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;

    let report = update(&world.installation()?, &world.host(), true)?;

    assert_eq!(world.releases.downloads(), 1);
    assert!(report.contains("up to date"), "{report}");
    Ok(())
}

#[test]
fn declining_an_update_changes_nothing() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.3.0")?;
    let world = world.declining();

    let report = update(&world.installation()?, &world.host(), false)?;

    assert_eq!(report, "Update cancelled.");
    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    assert_eq!(world.releases.downloads(), 1);
    Ok(())
}

// A rollback keeps the newer release, so updating again returns to it rather
// than failing on a release that is already there.
#[test]
fn update_after_a_rollback_returns_to_the_kept_release_without_downloading() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.3.0")?;
    update(&world.installation()?, &world.host(), true)?;
    rollback(&world.installation()?)?;
    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));

    update(&world.installation()?, &world.host(), true)?;

    assert_eq!(world.active()?.as_deref(), Some("1.3.0"));
    assert_eq!(world.releases.downloads(), 2);
    Ok(())
}

#[test]
fn update_with_nothing_installed_says_to_install() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;

    let refused = update(&world.installation()?, &world.host(), true);

    assert!(matches!(refused, Err(Failure::NotInstalled)), "{refused:?}");
    assert!(!world.root().exists());
    Ok(())
}

#[test]
fn check_reports_a_newer_release_and_changes_nothing() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.3.0")?;

    let report = check(&world.installation()?, &world.host())?;

    assert!(report.contains("1.3.0 is available"), "{report}");
    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    assert_eq!(world.releases.downloads(), 1);
    Ok(())
}

#[test]
fn rollback_returns_to_the_previous_release() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.3.0")?;
    update(&world.installation()?, &world.host(), true)?;

    let report = rollback(&world.installation()?)?;

    assert_eq!(world.active()?.as_deref(), Some("1.2.0"));
    assert!(report.contains("from 1.3.0 to 1.2.0"), "{report}");
    Ok(())
}

// Another launcher may act while the question is open, since it is asked
// without the lock. What was agreed to is then no longer what would happen.
#[test]
fn an_installation_changed_while_asking_is_left_as_it_is() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;
    install(&world.installation()?, &world.host(), None)?;
    world.releases.publish("1.3.0")?;
    update(&world.installation()?, &world.host(), true)?;
    rollback(&world.installation()?)?;
    let root = world.root();
    let world = world.answering(move |_| {
        // Another launcher rolls back meanwhile, to 1.3.0.
        Installation::at(root.clone()).is_ok_and(|other| rollback(&other).is_ok())
    });
    world.releases.publish("1.4.0")?;

    let refused = update(&world.installation()?, &world.host(), false);

    assert!(
        matches!(refused, Err(Failure::ChangedWhileAsking { .. })),
        "{refused:?}"
    );
    assert_eq!(world.active()?.as_deref(), Some("1.3.0"));
    assert_eq!(world.releases.downloads(), 2);
    Ok(())
}

#[test]
fn install_without_a_home_folder_is_refused_before_anything_is_written() -> TestResult {
    let world = World::new()?;
    world.releases.publish("1.2.0")?;

    let refused = install(&world.installation()?, &world.homeless_host(), None);

    assert!(matches!(refused, Err(Failure::NoHomeFolder)), "{refused:?}");
    assert!(!world.root().exists());
    Ok(())
}
