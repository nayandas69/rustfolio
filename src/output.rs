//! Atomic output writing and content fingerprinting.
//!
//! The site is always built into a staging directory next to the final
//! output and only swapped in after every page has been rendered and
//! verified. A failed build never replaces the previous output.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A staging area that is published (atomically-ish) on success.
pub struct Staging {
    pub dir: PathBuf,
    output: PathBuf,
}

impl Staging {
    pub fn stage(output: &Path) -> Result<Self> {
        let parent = output.parent().unwrap_or_else(|| Path::new("."));
        let name = output
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "dist".to_string());
        let dir = parent.join(format!(".{name}.staging-{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .with_context(|| format!("clearing staging dir {}", dir.display()))?;
        }
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            output: output.to_path_buf(),
        })
    }

    /// Write a file (creating parent directories) relative to the staging root.
    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes).with_context(|| format!("writing `{rel}`"))
    }

    /// Swap the staging directory into the output location.
    pub fn publish(self) -> Result<()> {
        let parent = self.output.parent().unwrap_or_else(|| Path::new("."));
        if self.output.exists() {
            let backup = parent.join(format!(
                ".{}.old-{}",
                self.output.file_name().unwrap().to_string_lossy(),
                std::process::id()
            ));
            fs::rename(&self.output, &backup)?;
            fs::rename(&self.dir, &self.output)?;
            fs::remove_dir_all(&backup)?;
        } else {
            fs::rename(&self.dir, &self.output)?;
        }
        Ok(())
    }
}

/// FNV-1a 64-bit content fingerprint — short, stable across runs and
/// platforms, used for long-cached fingerprinted asset filenames.
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_distinct() {
        let a = fingerprint(b"hello");
        assert_eq!(a, fingerprint(b"hello"));
        assert_ne!(a, fingerprint(b"world"));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn staging_publishes_and_does_not_partial_publish_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("dist");
        let staging = Staging::stage(&output).unwrap();
        staging.write("index.html", b"<html></html>").unwrap();
        assert!(!output.exists());
        staging.publish().unwrap();
        assert!(output.join("index.html").exists());

        // A failed build (publish never called) leaves the old output intact.
        let staging2 = Staging::stage(&output).unwrap();
        staging2.write("index.html", b"half-written").unwrap();
        drop(staging2); // simulate an aborted build
        let content = fs::read_to_string(output.join("index.html")).unwrap();
        assert_eq!(content, "<html></html>");
    }
}
