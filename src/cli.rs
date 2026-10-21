//! Command dispatch, read-only checks, and the lock shared by mutating commands.

use crate::{
    Result,
    paths::Paths,
    storage::{managed_ids, restore_one},
    sync,
};
use clap::{Parser, Subcommand};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::PermissionsExt,
    time::Instant,
};

/// Commands exposed to the installer and systemd service.
#[derive(Parser)]
#[command(about, version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_REVISION"), ")"), arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Read-only commands never create the managed directory or acquire its lock.
#[derive(Subcommand)]
enum Command {
    /// Update Desktop Actions from recent history.
    Sync {
        /// Preview changes without writing files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Restore original launchers and remove generated actions.
    Restore,
    /// Check configuration and preview changes without writing files.
    Check,
}

/// Parse commands, lock mutating operations, and report errors through the entry point.
pub(crate) fn run() -> Result<()> {
    let command = Cli::parse().command;
    let paths = Paths::from_env()?;
    let dry = matches!(command, Command::Check | Command::Sync { dry_run: true });
    let _lock = if !dry {
        fs::create_dir_all(&paths.managed)?;
        fs::set_permissions(&paths.managed, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(paths.managed.join("lock"))?;
        lock.try_lock()
            .map_err(|e| format!("another sync/install is running: {e}"))?;
        Some(lock)
    } else {
        None
    };
    let start = Instant::now();
    if matches!(command, Command::Restore) {
        for id in managed_ids(&paths)? {
            restore_one(&paths, &id, false)?;
            println!("Restored {id}");
        }
    } else {
        sync::run(&paths, dry, &which::which("gio")?)?;
    }
    println!(
        "Finished in {:.2} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}
