//! Bridge XDG recent bookmarks to application Desktop Actions.
//!
//! [`sync`] coordinates discovery, bounded streaming history selection, and
//! per-application publication. [`storage`] owns the symlink and original-file
//! protocol; [`cli`] supplies command dispatch and process locking.

mod cli;
mod desktop;
mod history;
mod matching;
mod paths;
mod storage;
mod sync;

/// Fallible operation with filesystem, XML, or validation errors.
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("xdg-recent-actions: {e}");
        std::process::exit(1);
    }
}
