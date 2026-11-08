//! `postmark-mcp-launcher` under its own name: installing, updating, rolling
//! back, and removing the server.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::failure::Failure;
use crate::host::System;
use crate::installation::Installation;
use crate::lifecycle;
use crate::settings::PinnedVersion;
use crate::uninstall;

#[derive(Debug, Parser)]
#[command(
    name = "postmark-mcp-launcher",
    version,
    about = "Installs, updates, and removes the Postmark MCP server"
)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Install the current release and put `postmark-mcp` on your PATH. Running it again repairs the commands without
    /// reinstalling.
    Install {
        /// Install this exact release, such as 1.2.3, for automation. It must be the current release.
        #[arg(long)]
        version: Option<String>,
    },
    /// Install the newest release. Check, clean up, or diagnose with a subcommand.
    Update {
        #[command(subcommand)]
        action: Option<UpdateAction>,
        /// Install without asking first.
        #[arg(long)]
        yes: bool,
    },
    /// Return to the release that was active before the last update.
    Rollback,
    /// Remove postmark-mcp from this computer, keeping your token file.
    Uninstall {
        /// Remove without asking first.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum UpdateAction {
    /// Report whether a newer release is available, without installing it.
    Check,
    /// Delete releases that are neither active nor the one before it.
    Cleanup,
    /// Describe the installation.
    Diagnostics,
}

/// Carries out the command line's action, printing the outcome on stdout, or the
/// failure on stderr.
pub(crate) fn run() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // Help and version print to stdout and succeed. Usage errors print to
            // stderr and exit with 2.
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
        }
    };
    match perform(&cli.action) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn perform(action: &Action) -> Result<String, Failure> {
    // A pinned version is checked before anything else, so a mistyped one never
    // reaches the network.
    let pinned = match action {
        Action::Install { version } => version.as_deref().map(PinnedVersion::try_from).transpose()?,
        _ => None,
    };
    let installation = Installation::detect()?;
    let system = System::new()?;
    let host = system.host();
    match action {
        Action::Install { .. } => lifecycle::install(&installation, &host, pinned.as_ref()),
        Action::Update { action: None, yes } => lifecycle::update(&installation, &host, *yes),
        Action::Update {
            action: Some(UpdateAction::Check),
            ..
        } => lifecycle::check(&installation, &host),
        Action::Update {
            action: Some(UpdateAction::Cleanup),
            ..
        } => lifecycle::cleanup(&installation),
        Action::Update {
            action: Some(UpdateAction::Diagnostics),
            ..
        } => lifecycle::diagnostics(&installation, &host),
        Action::Rollback => lifecycle::rollback(&installation),
        Action::Uninstall { yes } => uninstall::run(&installation, &host, *yes),
    }
}
