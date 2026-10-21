//! XDG search paths and filesystem ownership. An application launcher belongs to this
//! program only while its symlink points to the expected generated file.

use crate::Result;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Resolved data directory and application search roots in XDG precedence order.
#[derive(Clone)]
pub(crate) struct Paths {
    pub(crate) data: PathBuf,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) managed: PathBuf,
}
impl Paths {
    /// Resolve XDG defaults and deduplicate physical search roots.
    ///
    /// # Errors
    /// Returns an error if no data home can be resolved or it is relative.
    pub(crate) fn from_env() -> Result<Self> {
        let xdg = xdg::BaseDirectories::new();
        let data = xdg.get_data_home().ok_or("cannot resolve XDG data home")?;
        let roots = std::iter::once(data.clone()).chain(xdg.get_data_dirs());
        let mut roots: Vec<_> = roots.map(|p| p.join("applications")).collect();
        let mut seen_roots = BTreeSet::new();
        roots.retain(|root| {
            seen_roots.insert(fs::canonicalize(root).unwrap_or_else(|_| root.clone()))
        });
        if !data.is_absolute() {
            return Err("XDG_DATA_HOME must be absolute".into());
        }
        Ok(Self {
            managed: data.join("xdg-recent-actions"),
            data,
            roots,
        })
    }
    /// Return the user launcher path for a desktop file ID.
    pub(crate) fn target(&self, id: &str) -> PathBuf {
        self.data.join("applications").join(id)
    }

    /// Return the generated desktop file path.
    pub(crate) fn generated(&self, id: &str) -> PathBuf {
        self.managed.join("generated").join(id)
    }
    /// Return the saved original user entry path.
    pub(crate) fn original(&self, id: &str) -> PathBuf {
        self.managed.join("originals").join(id)
    }
    /// Return the relative symlink target used by user launchers.
    pub(crate) fn link(&self, id: &str) -> PathBuf {
        Path::new("../xdg-recent-actions/generated").join(id)
    }
    /// Check the launcher symlink itself, without following it to read generated text.
    pub(crate) fn owns(&self, id: &str) -> bool {
        fs::read_link(self.target(id)).is_ok_and(|p| p == self.link(id) || p == self.generated(id))
    }
}
