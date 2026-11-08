//! Everything the launcher takes from the computer it runs on, gathered in one
//! place so that tests can supply each part instead.

use std::ffi::OsString;
use std::path::PathBuf;

use updater_engine::{
    Clock, Ed25519ManifestVerifier, ExecutableVersionProbe, HttpTransport, ManifestVerifier, SpawnVersionProbe,
    SystemClock, TrustedKey, UreqTransport,
};

use crate::confirm::confirmed;
use crate::failure::Failure;
use crate::product::TRUSTED_KEYS;
use crate::settings::{UPDATE_URL_VARIABLE, UpdateUrl};

/// The network, the clock, the person at the terminal, and the environment.
pub(crate) struct Host<'a> {
    pub(crate) transport: &'a dyn HttpTransport,
    pub(crate) verifier: &'a dyn ManifestVerifier,
    pub(crate) probe: &'a dyn ExecutableVersionProbe,
    pub(crate) clock: &'a dyn Clock,
    /// Asks the person at the terminal a yes or no question.
    pub(crate) confirm: &'a dyn Fn(&str) -> bool,
    /// The value of `POSTMARK_MCP_UPDATE_URL`.
    pub(crate) update_url: Option<OsString>,
    /// The operator's home folder.
    pub(crate) home: Option<PathBuf>,
    /// The values of `PATH` and `SHELL`.
    pub(crate) path: Option<OsString>,
    pub(crate) shell: Option<OsString>,
}

impl Host<'_> {
    pub(crate) fn update_url(&self) -> Result<UpdateUrl, Failure> {
        UpdateUrl::try_from(self.update_url.clone())
    }

    pub(crate) fn home(&self) -> Result<PathBuf, Failure> {
        self.home.clone().ok_or(Failure::NoHomeFolder)
    }
}

/// The real collaborators, which a [`Host`] borrows.
pub(crate) struct System {
    transport: UreqTransport,
    verifier: Ed25519ManifestVerifier,
}

impl System {
    pub(crate) fn new() -> Result<Self, Failure> {
        let keys = TRUSTED_KEYS
            .iter()
            .map(TrustedKey::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            transport: UreqTransport::default(),
            verifier: Ed25519ManifestVerifier::new(keys),
        })
    }

    pub(crate) fn host(&self) -> Host<'_> {
        Host {
            transport: &self.transport,
            verifier: &self.verifier,
            probe: &SpawnVersionProbe,
            clock: &SystemClock,
            confirm: &confirmed,
            update_url: std::env::var_os(UPDATE_URL_VARIABLE),
            home: std::env::home_dir(),
            path: std::env::var_os("PATH"),
            shell: std::env::var_os("SHELL"),
        }
    }
}
