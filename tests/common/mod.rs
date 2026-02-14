//! Shared test harness — zero-boilerplate test fixtures and assertions.
//!
//! Design patterns:
//! - **Factory**: `cp()` creates a pre-configured Command
//! - **Fixture**: `Env` wraps TempDir with convenience builders
//! - **Fluent API**: chainable setup via `Env` methods returning PathBuf
#![allow(dead_code)]

pub use assert_cmd::Command;
pub use predicates;

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

// ─── Test environment (Fixture pattern) ──────────────────────────────────────

/// Lightweight test fixture wrapping a temporary directory.
/// All paths are relative to the temp root — auto-creates parent dirs.
pub struct Env(TempDir);

impl Env {
    #[inline]
    pub fn new() -> Self {
        Self(TempDir::new().unwrap())
    }

    #[inline]
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    /// Resolve a relative path under the temp root.
    #[inline]
    pub fn p(&self, rel: &str) -> PathBuf {
        self.0.path().join(rel)
    }

    /// Ensure parent directory exists (idempotent).
    #[inline]
    fn ensure_parent(p: &Path) {
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).unwrap();
            }
        }
    }

    /// Public variant for use in test-local helpers (e.g. sparse file creation).
    #[inline]
    pub fn ensure_parent_pub(p: &Path) {
        Self::ensure_parent(p);
    }

    /// Create a file with arbitrary content. Auto-creates parent dirs.
    pub fn file(&self, rel: &str, data: impl AsRef<[u8]>) -> PathBuf {
        let p = self.p(rel);
        Self::ensure_parent(&p);
        fs::write(&p, data).unwrap();
        p
    }

    /// Create a file with content and explicit mode.
    pub fn file_mode(&self, rel: &str, data: impl AsRef<[u8]>, mode: u32) -> PathBuf {
        let p = self.file(rel, data);
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    /// Create a directory tree (recursive). Returns the leaf path.
    pub fn dir(&self, rel: &str) -> PathBuf {
        let p = self.p(rel);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// Create a symbolic link. `target` is the link's target value (relative or absolute).
    pub fn symlink(&self, target: impl AsRef<Path>, rel: &str) -> PathBuf {
        let p = self.p(rel);
        Self::ensure_parent(&p);
        std::os::unix::fs::symlink(target.as_ref(), &p).unwrap();
        p
    }

    /// Create a hard link from an existing file.
    pub fn hardlink(&self, orig: &str, link: &str) -> PathBuf {
        let l = self.p(link);
        Self::ensure_parent(&l);
        fs::hard_link(self.p(orig), &l).unwrap();
        l
    }

    /// Set mtime (seconds, 0 nanoseconds).
    pub fn set_mtime(&self, rel: &str, secs: i64) {
        filetime::set_file_mtime(
            self.p(rel),
            filetime::FileTime::from_unix_time(secs, 0),
        )
        .unwrap();
    }

    /// Set mtime with nanosecond precision.
    pub fn set_mtime_ns(&self, rel: &str, secs: i64, nsec: u32) {
        filetime::set_file_mtime(
            self.p(rel),
            filetime::FileTime::from_unix_time(secs, nsec),
        )
        .unwrap();
    }

    /// Set symlink's own mtime.
    pub fn set_symlink_mtime(&self, rel: &str, secs: i64) {
        let ft = filetime::FileTime::from_unix_time(secs, 0);
        filetime::set_symlink_file_times(self.p(rel), ft, ft).unwrap();
    }

    /// Set permissions (chmod).
    pub fn chmod(&self, rel: &str, mode: u32) {
        fs::set_permissions(self.p(rel), fs::Permissions::from_mode(mode)).unwrap();
    }
}

// ─── Command factory ─────────────────────────────────────────────────────────

/// Create a pre-configured `cp` Command ready for `.arg()` chaining.
#[inline]
#[allow(deprecated)]
pub fn cp() -> Command {
    Command::cargo_bin("cp").unwrap()
}

// ─── Zero-cost reader helpers ────────────────────────────────────────────────

#[inline]
pub fn content(p: &Path) -> String {
    fs::read_to_string(p).unwrap()
}

#[inline]
pub fn bytes(p: &Path) -> Vec<u8> {
    fs::read(p).unwrap()
}

#[inline]
pub fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().mode() & 0o7777
}

#[inline]
pub fn mtime(p: &Path) -> i64 {
    fs::metadata(p).unwrap().mtime()
}

#[inline]
pub fn mtime_nsec(p: &Path) -> i64 {
    fs::metadata(p).unwrap().mtime_nsec()
}

#[inline]
pub fn ino(p: &Path) -> u64 {
    fs::metadata(p).unwrap().ino()
}

#[inline]
pub fn is_symlink(p: &Path) -> bool {
    p.symlink_metadata().unwrap().file_type().is_symlink()
}

#[inline]
pub fn symlink_mtime(p: &Path) -> i64 {
    fs::symlink_metadata(p).unwrap().mtime()
}

#[inline]
pub fn link_target(p: &Path) -> PathBuf {
    fs::read_link(p).unwrap()
}

#[inline]
pub fn file_count(dir: &Path) -> usize {
    fs::read_dir(dir).unwrap().count()
}

#[inline]
pub fn file_size(p: &Path) -> u64 {
    fs::metadata(p).unwrap().len()
}

#[inline]
pub fn blocks(p: &Path) -> u64 {
    fs::metadata(p).unwrap().blocks()
}
