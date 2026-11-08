use super::*;
use crate::test_support::TestResult;

fn at(path: &Path) -> TestResult<Installation> {
    Ok(Installation::at(path.to_path_buf())?)
}

#[test]
fn a_missing_or_empty_folder_is_vacant() -> TestResult {
    let directory = tempfile::tempdir()?;
    assert_eq!(at(&directory.path().join("missing"))?.presence()?, Presence::Vacant);
    assert_eq!(at(directory.path())?.presence()?, Presence::Vacant);
    Ok(())
}

#[test]
fn a_folder_with_any_marker_is_an_installation() -> TestResult {
    for marker in MARKERS {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join(marker), "")?;
        assert_eq!(at(directory.path())?.presence()?, Presence::Installed, "{marker}");
    }
    Ok(())
}

#[test]
fn a_folder_holding_anything_else_is_foreign() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("notes.txt"), "mine")?;
    assert_eq!(at(directory.path())?.presence()?, Presence::Foreign);
    Ok(())
}

#[test]
fn only_the_updaters_own_temporary_files_count_as_its_own() {
    for name in [".state.json.123.tmp", ".current.9.tmp", ".launchers.json.4242.tmp"] {
        assert!(is_temporary(name), "{name}");
    }
    for name in [
        ".notes.txt.1.tmp",
        ".state.json.tmp",
        ".state.json.x1.tmp",
        "state.json.1.tmp",
        ".state.json.1.tmp.bak",
    ] {
        assert!(!is_temporary(name), "{name}");
    }
}

#[test]
fn removal_takes_the_installers_entries_and_keeps_everything_else() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("root");
    fs::create_dir_all(root.join("versions").join("1.0.0"))?;
    fs::write(root.join("state.json"), "{}")?;
    fs::write(root.join(".state.json.77.tmp"), "")?;
    fs::write(root.join("notes.txt"), "mine")?;
    let installation = at(&root)?;
    let lock = UpdateLock::acquire(&root)?;

    let removal = installation.remove(lock);

    assert!(removal.failed.is_empty(), "{:?}", removal.failed);
    assert_eq!(removal.kept, vec![root.join("notes.txt")]);
    let mut left: Vec<String> = fs::read_dir(&root)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    left.sort();
    assert_eq!(left, vec!["notes.txt"]);
    Ok(())
}

#[test]
fn removal_of_only_the_installers_entries_removes_the_folder() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("root");
    fs::create_dir_all(root.join("versions"))?;
    fs::write(root.join("launchers.json"), "{}")?;
    let installation = at(&root)?;
    let lock = UpdateLock::acquire(&root)?;

    let removal = installation.remove(lock);

    assert!(removal.failed.is_empty() && removal.kept.is_empty());
    assert!(!root.exists());
    Ok(())
}
