//! Requests to the release server, through `updater-engine`, which verifies the
//! signed manifest before anything in it is used.

use std::fs;

use updater_core::{ActivationPort, ProductVersion, ReleaseMetadata, TargetTriple};
use updater_engine::{
    CheckOutcome, InstallCollaborators, UpdateContext, check_for_update, detect_target, install_update, validate_bundle,
};

use crate::failure::Failure;
use crate::host::Host;
use crate::installation::Installation;
use crate::product::{PRODUCT, RETAINED_VERSIONS, SERVER_NAME};
use crate::settings::UpdateUrl;

/// The release server, as described by validated settings.
pub(crate) struct Remote<'a> {
    host: &'a Host<'a>,
    url: UpdateUrl,
    target: TargetTriple,
    commands: Vec<String>,
}

impl<'a> Remote<'a> {
    /// Validates the settings a request needs, before any request.
    pub(crate) fn new(host: &'a Host<'a>) -> Result<Self, Failure> {
        Ok(Self {
            host,
            url: host.update_url()?,
            target: detect_target()?,
            commands: vec![SERVER_NAME.to_owned()],
        })
    }

    fn context<'b>(&'b self, installation: &'b Installation) -> UpdateContext<'b> {
        UpdateContext {
            product: PRODUCT,
            installation_root: installation.root(),
            manifest_url: self.url.as_str(),
            target: &self.target,
            declared_public_commands: &self.commands,
            retain_versions: RETAINED_VERSIONS,
        }
    }

    /// Fetches and verifies the manifest, writing nothing.
    pub(crate) fn check(&self, installation: &Installation) -> Result<CheckOutcome, Failure> {
        Ok(check_for_update(
            self.host.transport,
            self.host.verifier,
            &installation.activation(),
            &self.context(installation),
        )?)
    }

    /// Makes `version`, the release the manifest named when checked, active.
    ///
    /// A release kept from before, such as the one a rollback left, is checked
    /// again and activated where it is, since the updater will not install a
    /// release that is already there. Otherwise the updater fetches the manifest
    /// again and installs what it names. When `exact`, a different release
    /// there, because one was published in between, is reported as a failure,
    /// although it is by then active.
    pub(crate) fn activate(
        &self,
        installation: &Installation,
        version: &ProductVersion,
        allow_downgrade: bool,
        exact: bool,
    ) -> Result<ProductVersion, Failure> {
        let activation = installation.activation();
        if self.is_kept(installation, version) {
            activation.activate(version)?;
            return Ok(version.clone());
        }
        let collaborators = InstallCollaborators {
            transport: self.host.transport,
            verifier: self.host.verifier,
            probe: self.host.probe,
            activation: &activation,
            clock: self.host.clock,
        };
        let installed = install_update(&collaborators, &self.context(installation), allow_downgrade)?;
        if exact && installed != *version {
            return Err(Failure::InstalledOtherVersion {
                expected: version.to_string(),
                installed: installed.to_string(),
            });
        }
        Ok(installed)
    }

    /// Whether `version` is already installed and still a whole, valid release
    /// set for this product and platform.
    fn is_kept(&self, installation: &Installation, version: &ProductVersion) -> bool {
        let folder = installation.versions().join(version.to_string());
        let Ok(bytes) = fs::read(folder.join("release.json")) else {
            return false;
        };
        ReleaseMetadata::try_from(bytes.as_slice()).is_ok_and(|metadata| {
            validate_bundle(&metadata, &folder, PRODUCT, version, &self.target, &self.commands).is_ok()
        })
    }
}
