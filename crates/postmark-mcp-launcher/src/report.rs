//! How the launcher words what it did outside the installation folder.

use std::fmt;
use std::path::Path;

use updater_engine::PathChange;

use crate::product::LAUNCHER_NAME;

/// A file with a command's name that an install did not place, and left alone.
pub(crate) struct LeftAlone<'a>(pub(crate) &'a Path);

impl fmt::Display for LeftAlone<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} already exists and was not placed by {LAUNCHER_NAME}, so it was left as it is.",
            self.0.display()
        )
    }
}

/// Where the commands are, and how their folder came to be on `PATH`.
pub(crate) struct PathReport<'a> {
    pub(crate) folder: &'a Path,
    pub(crate) change: &'a PathChange,
}

impl fmt::Display for PathReport<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let folder = self.folder.display();
        match self.change {
            PathChange::AlreadyOnPath => write!(formatter, "The commands are in {folder}, which is on your PATH."),
            PathChange::AddedToProfiles(profiles) => {
                let names: Vec<String> = profiles.iter().map(|profile| profile.display().to_string()).collect();
                write!(
                    formatter,
                    "The commands are in {folder}, which was added to your PATH in {}.",
                    names.join(" and ")
                )
            }
            PathChange::AddedToUserPath => {
                write!(formatter, "The commands are in {folder}, which was added to your Path.")
            }
            PathChange::NeedsAddingByHand => write!(
                formatter,
                "The commands are in {folder}. Add that folder to your PATH to use them."
            ),
        }
    }
}
