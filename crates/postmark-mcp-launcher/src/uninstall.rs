//! `uninstall`: removes what an install added, and nothing else. The token file
//! and the MCP hosts' configuration are the operator's, so they stay. A launcher
//! copy is removed only while it is still the one that was placed, and the
//! installation folder only loses the entries an install creates.

use updater_engine::UpdateLock;

use crate::failure::Failure;
use crate::host::Host;
use crate::installation::{Installation, Presence};
use crate::lifecycle::stable_launchers;
use crate::product::SERVER_NAME;

/// Removes the commands, the `PATH` changes, and the release sets. Whatever
/// cannot be removed is reported, and makes the uninstall a failure.
pub(crate) fn run(installation: &Installation, host: &Host, yes: bool) -> Result<String, Failure> {
    let root = installation.root();
    match installation.presence()? {
        Presence::Vacant => return Ok(format!("{SERVER_NAME} is not installed.")),
        Presence::Foreign => {
            return Err(Failure::NotAnInstallation {
                root: root.to_path_buf(),
            });
        }
        Presence::Installed => {}
    }
    if !yes
        && !(host.confirm)(&format!(
            "Remove {SERVER_NAME} and {} from this computer? Your token file is kept.",
            root.display()
        ))
    {
        return Ok("Uninstall cancelled.".to_owned());
    }

    let home = host.home()?;
    let lock = UpdateLock::acquire(root)?;
    let launchers = stable_launchers(installation, &home).remove()?;
    let folder = installation.remove(lock);

    let mut report: Vec<String> = launchers
        .changed
        .iter()
        .map(|path| {
            format!(
                "{} has changed since it was installed, so it was left as it is.",
                path.display()
            )
        })
        .collect();
    report.extend(folder.kept.iter().map(|path| {
        format!(
            "{} was not placed by an install, so it was kept, and so was {}.",
            path.display(),
            root.display()
        )
    }));
    let failed: Vec<String> = launchers
        .failed
        .iter()
        .chain(&folder.failed)
        .map(|path| {
            format!(
                "{} could not be removed, perhaps because a server there is still running.",
                path.display()
            )
        })
        .collect();
    if !failed.is_empty() {
        report.extend(failed);
        report.push(format!(
            "Close any MCP host running {SERVER_NAME}, then run uninstall again."
        ));
        return Err(Failure::PartlyRemoved {
            report: report.join("\n"),
        });
    }

    report.push(format!("{SERVER_NAME} has been removed."));
    report.push(format!(
        "Your token file has been kept. Remove {SERVER_NAME} from each MCP host's configuration too, or the host will \
         keep trying to start it."
    ));
    Ok(report.join("\n"))
}

#[cfg(test)]
#[path = "uninstall_test.rs"]
mod tests;
