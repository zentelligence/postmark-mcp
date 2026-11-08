//! The folder the release sets are kept in, and what the launcher may do to it.
//!
//! The folder can be anywhere `POSTMARK_MCP_INSTALL_ROOT` names, including a
//! folder of the operator's own, so the launcher only ever writes to a folder
//! that is missing, empty, or recognisably an installation, and only ever
//! removes the entries an install creates.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use updater_core::InstallationRoot;
use updater_engine::{EngineError, FilesystemActivationPort, UpdateLock};
use updater_launcher::{ProcessEnvironment, resolve_installation_root};

use crate::failure::Failure;
use crate::product::{ENV_PREFIX, INSTALL_FOLDER};

/// The variable that moves the installation.
pub(crate) const INSTALL_ROOT_VARIABLE: &str = "POSTMARK_MCP_INSTALL_ROOT";

/// The lock file, which is removed last.
const LOCK: &str = "update.lock";

/// Entries whose presence shows the folder is an installation.
const MARKERS: [&str; 4] = ["launchers.json", "state.json", "current", "current.txt"];

/// Every entry an install creates in the folder, apart from the lock and the
/// launchers' folder on Windows.
const CREATED: [&str; 9] = [
    "versions",
    "staging",
    "downloads",
    "current",
    "previous",
    "current.txt",
    "previous.txt",
    "state.json",
    "launchers.json",
];

/// The launchers' folder on Windows, removed only once empty.
const LAUNCHER_FOLDER: &str = "bin";

/// What is at the installation root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Presence {
    /// Nothing, or an empty folder.
    Vacant,
    /// An installation.
    Installed,
    /// A folder holding something else, which is not the launcher's to change.
    Foreign,
}

/// What [`Installation::remove`] could not do.
#[derive(Debug, Default)]
pub(crate) struct Removal {
    /// Entries an install created that could not be removed.
    pub(crate) failed: Vec<PathBuf>,
    /// Entries an install did not create, which were left, keeping the folder.
    pub(crate) kept: Vec<PathBuf>,
}

/// Where this computer keeps, or would keep, the release sets.
pub(crate) struct Installation {
    root: InstallationRoot,
}

impl Installation {
    /// The installation root, from `POSTMARK_MCP_INSTALL_ROOT` or the platform's
    /// default. A value that is not Unicode is refused, rather than ignored in
    /// favour of the default folder.
    pub(crate) fn detect() -> Result<Self, Failure> {
        if std::env::var_os(INSTALL_ROOT_VARIABLE).is_some_and(|value| value.to_str().is_none()) {
            return Err(Failure::InstallRootNotUnicode);
        }
        let root = resolve_installation_root(&ProcessEnvironment, ENV_PREFIX, INSTALL_FOLDER)?;
        Ok(Self { root })
    }

    #[cfg(test)]
    pub(crate) fn at(root: PathBuf) -> Result<Self, Failure> {
        Ok(Self {
            root: InstallationRoot::try_from(root)?,
        })
    }

    pub(crate) fn installation_root(&self) -> &InstallationRoot {
        &self.root
    }

    pub(crate) fn root(&self) -> &Path {
        self.root.as_path()
    }

    pub(crate) fn activation(&self) -> FilesystemActivationPort {
        FilesystemActivationPort::new(self.root().to_path_buf())
    }

    pub(crate) fn versions(&self) -> PathBuf {
        self.root().join("versions")
    }

    /// What is at the root now.
    pub(crate) fn presence(&self) -> Result<Presence, Failure> {
        let mut entries = match fs::read_dir(self.root()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Presence::Vacant),
            Err(source) => return Err(self.io(source)),
        };
        if MARKERS
            .iter()
            .any(|marker| self.root().join(marker).symlink_metadata().is_ok())
        {
            return Ok(Presence::Installed);
        }
        Ok(if entries.next().is_none() {
            Presence::Vacant
        } else {
            Presence::Foreign
        })
    }

    /// Refuses a folder that holds something other than an installation.
    pub(crate) fn require_writable(&self) -> Result<Presence, Failure> {
        match self.presence()? {
            Presence::Foreign => Err(self.foreign()),
            presence => Ok(presence),
        }
    }

    /// Takes the update lock, creating the root if need be.
    pub(crate) fn lock_creating(&self) -> Result<UpdateLock, Failure> {
        self.require_writable()?;
        fs::create_dir_all(self.root()).map_err(|source| self.io(source))?;
        Ok(UpdateLock::acquire(self.root())?)
    }

    /// Refuses anything but an installation.
    pub(crate) fn require_installed(&self) -> Result<(), Failure> {
        match self.presence()? {
            Presence::Installed => Ok(()),
            Presence::Vacant => Err(Failure::NotInstalled),
            Presence::Foreign => Err(self.foreign()),
        }
    }

    /// Takes the update lock of an existing installation.
    pub(crate) fn lock_existing(&self) -> Result<UpdateLock, Failure> {
        self.require_installed()?;
        Ok(UpdateLock::acquire(self.root())?)
    }

    /// Removes the entries an install creates while `lock` is held, then the lock
    /// and the root, if nothing else is left in it.
    pub(crate) fn remove(&self, lock: UpdateLock) -> Removal {
        let mut removal = Removal::default();
        let entries: Vec<PathBuf> = fs::read_dir(self.root())
            .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).collect())
            .unwrap_or_default();
        for entry in entries {
            let Some(name) = entry.file_name().and_then(OsStr::to_str) else {
                removal.kept.push(entry);
                continue;
            };
            if name == LOCK {
                continue;
            }
            let outcome = if CREATED.contains(&name) || is_temporary(name) {
                remove_entry(&entry)
            } else if name == LAUNCHER_FOLDER {
                // Removed only when empty: anything still there is not ours.
                fs::remove_dir(&entry).or_else(|_| {
                    removal.kept.push(entry.clone());
                    Ok(())
                })
            } else {
                removal.kept.push(entry.clone());
                Ok(())
            };
            if outcome.is_err() {
                removal.failed.push(entry);
            }
        }

        drop(lock);
        let lock_file = self.root().join(LOCK);
        if fs::remove_file(&lock_file).is_err() && lock_file.exists() {
            removal.failed.push(lock_file);
        }
        if removal.kept.is_empty() && removal.failed.is_empty() && fs::remove_dir(self.root()).is_err() {
            removal.failed.push(self.root().to_path_buf());
        }
        removal
    }

    fn foreign(&self) -> Failure {
        Failure::NotAnInstallation {
            root: self.root().to_path_buf(),
        }
    }

    fn io(&self, source: std::io::Error) -> Failure {
        Failure::Engine(EngineError::Io {
            path: self.root().to_path_buf(),
            source,
        })
    }
}

/// Whether `name` is a temporary file the updater writes beside one of its own,
/// as `.<name>.<process>.tmp`, left behind if it was interrupted.
fn is_temporary(name: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .and_then(|rest| rest.rsplit_once('.'))
        .is_some_and(|(stem, process)| {
            CREATED.contains(&stem) && !process.is_empty() && process.bytes().all(|digit| digit.is_ascii_digit())
        })
}

fn remove_entry(path: &Path) -> std::io::Result<()> {
    let metadata = path.symlink_metadata()?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
#[path = "installation_test.rs"]
mod tests;
