//! Scratch directory for tests, removed on drop, so no test writes where another can see it.
//!
//! Twenty lines of the standard library, so no test needs a crate for it.

use std::path::{Path, PathBuf};

/// A folder of its own under the system's temporary folder, removed when it is dropped.
#[derive(Debug)]
pub struct Dir(PathBuf);

impl Default for Dir {
    fn default() -> Self {
        Self::new()
    }
}

impl Dir {
    /// A new, empty folder named for the process and thread.
    ///
    /// # Panics
    ///
    /// Panics when the folder cannot be created.
    #[must_use]
    pub fn new() -> Self {
        // Named for the process and thread so two tests running at once cannot share one, and
        // cleared first so a previous run that was killed cannot leave a file behind that a
        // later run then reads as its own.
        let unique = format!(
            "steamgauge-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
