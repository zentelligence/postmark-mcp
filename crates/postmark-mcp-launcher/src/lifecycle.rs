//! Installing, updating, and rolling back. `updater-engine` does the work: this
//! decides what should happen, names the product, and puts the outcome into
//! words. Every change holds the update lock, so two launchers cannot interleave.

use std::collections::HashSet;
use std::path::Path;

use updater_core::{ActivationPort, ActiveVersionReader, ProductVersion};
use updater_engine::{StableLaunchers, cleanup_obsolete_versions, detect_target, read_state, rollback as roll_back};

use crate::failure::Failure;
use crate::host::Host;
use crate::installation::Installation;
use crate::product::{LAUNCHER_NAME, PRODUCT, PRODUCT_LABEL, SERVER_NAME};
use crate::remote::Remote;
use crate::report::{LeftAlone, PathReport};
use crate::settings::PinnedVersion;

pub(crate) fn stable_launchers<'a>(installation: &'a Installation, home: &'a Path) -> StableLaunchers<'a> {
    StableLaunchers {
        installation_root: installation.root(),
        home,
        launcher_name: LAUNCHER_NAME,
        product_label: PRODUCT_LABEL,
    }
}

/// `install`: makes the current release active, unless it or a newer release
/// already is, then sets up the commands and puts them on `PATH`. Nothing is
/// written until the signed manifest has been checked and any pin matched.
pub(crate) fn install(
    installation: &Installation,
    host: &Host,
    pinned: Option<&PinnedVersion>,
) -> Result<String, Failure> {
    let remote = Remote::new(host)?;
    let home = host.home()?;
    installation.require_writable()?;
    let current = remote.check(installation)?.manifest.version;
    if let Some(PinnedVersion(requested)) = pinned
        && *requested != current
    {
        return Err(Failure::PinnedVersionUnavailable {
            requested: requested.to_string(),
            offered: current.to_string(),
        });
    }

    let _lock = installation.lock_creating()?;
    let active = installation.activation().active_version()?;
    let (installed, said) = match active {
        Some(active) if active == current => {
            let said = format!("{SERVER_NAME} {active} is already installed.");
            (active, said)
        }
        // Installing again is how the commands are repaired, so a newer release
        // is only replaced when the current one is asked for by name.
        Some(active) if active > current && pinned.is_none() => {
            let said = format!(
                "{SERVER_NAME} {active} is installed, which is newer than the current release, {current}, so it was \
                 kept. Give --version {current} to install that instead."
            );
            (active, said)
        }
        _ => {
            let installed = remote.activate(installation, &current, true, pinned.is_some())?;
            let said = format!("Installed {SERVER_NAME} {installed}.");
            (installed, said)
        }
    };

    let mut report = vec![said];
    report.extend(
        set_up_commands(installation, host, &home, &installed).map_err(|source| Failure::CommandsNotSetUp {
            version: installed.to_string(),
            source: Box::new(source),
        })?,
    );
    Ok(report.join("\n"))
}

/// Brings the launcher's copies up to `version`, and puts their folder on `PATH`.
fn set_up_commands(
    installation: &Installation,
    host: &Host,
    home: &Path,
    version: &ProductVersion,
) -> Result<Vec<String>, Failure> {
    let launchers = stable_launchers(installation, home);
    let mut report: Vec<String> = launchers
        .refresh(version)?
        .iter()
        .map(|path| LeftAlone(path).to_string())
        .collect();
    let change = launchers.add_to_path(host.path.as_deref(), host.shell.as_deref())?;
    let folder = launchers.folder();
    report.push(
        PathReport {
            folder: &folder,
            change: &change,
        }
        .to_string(),
    );
    report.push(format!(
        "Open a new terminal, run '{SERVER_NAME} doctor' with your settings, then point your MCP host at {}.",
        folder
            .join(format!("{SERVER_NAME}{}", std::env::consts::EXE_SUFFIX))
            .display()
    ));
    Ok(report)
}

/// `update`: makes a newer current release active, if there is one, and
/// replaces the launcher's copies with that release's. The question is asked
/// before the lock is taken, so a launcher waiting for an answer holds up no
/// other.
pub(crate) fn update(installation: &Installation, host: &Host, yes: bool) -> Result<String, Failure> {
    let remote = Remote::new(host)?;
    let home = host.home()?;
    installation.require_installed()?;
    let outcome = remote.check(installation)?;
    let Some(active) = outcome.active_version else {
        return Err(Failure::NotInstalled);
    };
    let available = outcome.manifest.version;
    if available <= active {
        return Ok(Freshness {
            active: &active,
            available: &available,
        }
        .to_string());
    }
    if !yes && !(host.confirm)(&format!("Update {SERVER_NAME} from {active} to {available}?")) {
        return Ok("Update cancelled.".to_owned());
    }

    let _lock = installation.lock_existing()?;
    let found = installation.activation().active_version()?;
    if found.as_ref() != Some(&active) {
        return Err(Failure::ChangedWhileAsking {
            expected: active.to_string(),
            found: found.map_or_else(|| "nothing".to_owned(), |version| version.to_string()),
        });
    }
    let installed = remote.activate(installation, &available, false, true)?;
    let mut report = vec![
        format!("Updated {SERVER_NAME} from {active} to {installed}."),
        "A server already running keeps its release until its MCP host restarts it.".to_owned(),
    ];
    let refreshed = stable_launchers(installation, &home)
        .refresh(&installed)
        .map_err(|source| Failure::CommandsNotSetUp {
            version: installed.to_string(),
            source: Box::new(source.into()),
        })?;
    report.extend(refreshed.iter().map(|path| LeftAlone(path).to_string()));
    Ok(report.join("\n"))
}

/// `update check`: reports whether a newer release is available, changing
/// nothing.
pub(crate) fn check(installation: &Installation, host: &Host) -> Result<String, Failure> {
    let outcome = Remote::new(host)?.check(installation)?;
    let available = outcome.manifest.version;
    Ok(match outcome.active_version {
        None => format!("{SERVER_NAME} is not installed. Run '{LAUNCHER_NAME} install' to install {available}."),
        Some(active) if available > active => format!(
            "{SERVER_NAME} {available} is available. {active} is installed.\nRun '{LAUNCHER_NAME} update' to update."
        ),
        Some(active) => Freshness {
            active: &active,
            available: &available,
        }
        .to_string(),
    })
}

/// How an active release compares with a current release that is not newer.
struct Freshness<'a> {
    active: &'a ProductVersion,
    available: &'a ProductVersion,
}

impl std::fmt::Display for Freshness<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { active, available } = self;
        if active == available {
            write!(formatter, "{SERVER_NAME} {active} is up to date.")
        } else {
            write!(
                formatter,
                "{SERVER_NAME} {active} is installed, which is newer than the current release, {available}."
            )
        }
    }
}

/// `rollback`: makes the release before the last update active again.
pub(crate) fn rollback(installation: &Installation) -> Result<String, Failure> {
    let _lock = installation.lock_existing()?;
    let activation = installation.activation();
    let from = activation.active_version()?;
    let to = roll_back(&activation, &installation.versions())?;
    let rolled = match from {
        Some(from) => format!("Rolled {SERVER_NAME} back from {from} to {to}."),
        None => format!("Rolled {SERVER_NAME} back to {to}."),
    };
    Ok(format!(
        "{rolled}\nA server already running keeps its release until its MCP host restarts it."
    ))
}

/// `update cleanup`: deletes every release but the active one and the one before
/// it. A release still in use is kept, and tried again next time.
pub(crate) fn cleanup(installation: &Installation) -> Result<String, Failure> {
    let _lock = installation.lock_existing()?;
    let activation = installation.activation();
    let keep: HashSet<ProductVersion> = [activation.active_version()?, activation.previous_version()?]
        .into_iter()
        .flatten()
        .collect();

    let deferred = cleanup_obsolete_versions(&installation.versions(), &keep);
    if deferred.is_empty() {
        return Ok("No old releases remain.".to_owned());
    }
    let mut report = vec!["These releases are still in use, and will be removed by a later cleanup:".to_owned()];
    report.extend(
        deferred
            .iter()
            .map(|item| format!("  {} ({})", item.version, item.reason)),
    );
    Ok(report.join("\n"))
}

/// `update diagnostics`: describes the installation, changing nothing.
pub(crate) fn diagnostics(installation: &Installation, host: &Host) -> Result<String, Failure> {
    let activation = installation.activation();
    let shown = |version: Option<ProductVersion>| version.map_or_else(|| "none".to_owned(), |v| v.to_string());
    let update_url = host
        .update_url()
        .map_or_else(|error| format!("unusable: {error}"), |url| url.as_str().to_owned());

    let mut lines = vec![
        format!("product: {PRODUCT}"),
        format!("launcher_version: {}", env!("CARGO_PKG_VERSION")),
        format!("installation_root: {}", installation.root().display()),
        format!("detected_target: {}", detect_target()?),
        format!("update_url: {update_url}"),
        format!("active_version: {}", shown(activation.active_version()?)),
        format!("previous_version: {}", shown(activation.previous_version()?)),
    ];
    match read_state(installation.root())? {
        Some(state) => {
            let installed: Vec<String> = state.installed_versions.iter().map(ToString::to_string).collect();
            lines.push(format!("channel: {}", state.channel));
            lines.push(format!("installed_versions: {}", installed.join(", ")));
            lines.push(format!(
                "last_check_at: {}",
                state.last_check_at.as_deref().unwrap_or("never")
            ));
            lines.push(format!(
                "last_successful_update_at: {}",
                state.last_successful_update_at.as_deref().unwrap_or("never")
            ));
            lines.push(format!(
                "last_update_error: {}",
                state.last_update_error.as_deref().unwrap_or("none")
            ));
        }
        None => lines.push("state: none recorded".to_owned()),
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
