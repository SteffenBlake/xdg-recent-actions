//! Conservative association of XBEL application identities with desktop entries.
//! Unknown or ambiguous identities are skipped rather than guessed.

use crate::desktop::{App, executable, normalized};
use std::collections::{BTreeMap, BTreeSet};

/// Application lookup tables containing indices into the compact inventory.
pub(crate) struct Index {
    names: BTreeMap<String, BTreeSet<usize>>,
    executables: BTreeMap<String, BTreeSet<usize>>,
}
impl Index {
    /// Index executable basenames, display names, and desktop IDs.
    pub(crate) fn new(apps: &[App]) -> Self {
        let mut index = Self {
            names: BTreeMap::new(),
            executables: BTreeMap::new(),
        };
        for (i, app) in apps.iter().enumerate() {
            for name in [normalized(&app.id), app.name.clone()] {
                index.names.entry(name).or_default().insert(i);
            }
            index
                .executables
                .entry(app.executable.clone())
                .or_default()
                .insert(i);
        }
        index
    }
    /// Return a unique application match. Commands take precedence; generic wrappers
    /// require an application identity. Ambiguous or unknown records return `None`.
    pub(crate) fn find(&self, recorded_name: &str, recorded_exec: &str) -> Option<usize> {
        let name = normalized(recorded_name);
        let names = self.names.get(&name);
        let command = executable(recorded_exec, true);
        let execs = command.as_ref().and_then(|e| self.executables.get(e));
        // The actual launch command takes precedence over a reporting app's name.
        if let Some(execs) = execs {
            let wrapper = command.as_deref().is_some_and(|c| {
                matches!(
                    c,
                    "sh" | "bash"
                        | "dash"
                        | "zsh"
                        | "fish"
                        | "flatpak"
                        | "snap"
                        | "python"
                        | "python3"
                        | "perl"
                        | "ruby"
                        | "node"
                        | "java"
                        | "wine"
                        | "wine64"
                        | "gio"
                        | "gtk-launch"
                        | "xdg-open"
                )
            });
            if execs.len() == 1 && !wrapper {
                return execs.first().copied();
            }
            let matches = names
                .map(|n| n.intersection(execs).copied().collect::<Vec<_>>())
                .unwrap_or_default();
            return (matches.len() == 1).then(|| matches[0]);
        }
        // Some GTK records use a desktop ID as the executable.
        let matches: BTreeSet<_> = names
            .into_iter()
            .flatten()
            .copied()
            .chain(
                command
                    .as_ref()
                    .and_then(|c| self.names.get(&normalized(c)))
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .collect();
        (matches.len() == 1).then(|| *matches.first().unwrap())
    }
}

#[cfg(test)]
#[path = "tests/matching.rs"]
mod tests;
