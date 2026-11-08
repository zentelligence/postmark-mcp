//! Launching the launcher with an environment of the test's choosing.

use std::process::Command;

/// Variables Windows itself needs in a process before it can start another or
/// reach the network. They carry no configuration.
#[cfg(windows)]
const REQUIRED_BY_THE_OPERATING_SYSTEM: [&str; 2] = ["SystemRoot", "windir"];

/// Clears the environment, so the test sees only what it sets, apart from what
/// the operating system needs to run the process at all.
pub fn isolate(command: &mut Command) -> &mut Command {
    command.env_clear();
    #[cfg(windows)]
    for name in REQUIRED_BY_THE_OPERATING_SYSTEM {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}
