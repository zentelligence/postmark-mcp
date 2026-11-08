//! Why the launcher could not do what it was asked.

use std::path::PathBuf;

use updater_core::CoreError;
use updater_engine::EngineError;
use updater_launcher::LauncherError;

use crate::product::{LAUNCHER_NAME, SERVER_NAME};

/// Every way managing the installation can fail. Each message says what to do,
/// where there is something to do.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Failure {
    #[error("POSTMARK_MCP_UPDATE_URL is empty. Unset it to use the published releases, or give an HTTPS address")]
    EmptyUpdateUrl,

    #[error("POSTMARK_MCP_UPDATE_URL is not valid Unicode. Unset it, or give an HTTPS address")]
    UpdateUrlNotUnicode,

    #[error("POSTMARK_MCP_INSTALL_ROOT is not valid Unicode. Unset it, or give a folder whose path is")]
    InstallRootNotUnicode,

    #[error("--version must be a release number such as 1.2.3, with no leading v")]
    MalformedPinnedVersion(#[source] CoreError),

    #[error("--version {requested} is not available. The current release is {offered}")]
    PinnedVersionUnavailable { requested: String, offered: String },

    #[error(
        "{installed} was installed and is now active, but {expected} was expected: the current release changed \
         while installing. Run '{LAUNCHER_NAME} rollback' to return to the release before it"
    )]
    InstalledOtherVersion { expected: String, installed: String },

    #[error(
        "{SERVER_NAME} changed from {expected} to {found} while waiting for an answer, so nothing was installed. Run \
         the command again"
    )]
    ChangedWhileAsking { expected: String, found: String },

    #[error(
        "{SERVER_NAME} {version} is installed and active, but its commands could not be set up: {source}. Run \
         '{LAUNCHER_NAME} install' again to finish"
    )]
    CommandsNotSetUp {
        version: String,
        #[source]
        source: Box<Failure>,
    },

    #[error("{SERVER_NAME} is not installed. Run '{LAUNCHER_NAME} install' to install it")]
    NotInstalled,

    #[error(
        "{} holds files that are not a {SERVER_NAME} installation, so it was left alone. Set \
         POSTMARK_MCP_INSTALL_ROOT to an empty or new folder",
        root.display()
    )]
    NotAnInstallation { root: PathBuf },

    #[error("{SERVER_NAME} was only partly removed:\n{report}")]
    PartlyRemoved { report: String },

    #[error("no home folder could be found for the {SERVER_NAME} commands. Set HOME, or USERPROFILE on Windows")]
    NoHomeFolder,

    #[error("trusted key {id} built into this launcher is malformed")]
    MalformedTrustedKey { id: &'static str },

    #[error(transparent)]
    Engine(#[from] EngineError),

    #[error(transparent)]
    Launcher(#[from] LauncherError),

    #[error(transparent)]
    Core(#[from] CoreError),
}
