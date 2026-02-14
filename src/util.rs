use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::error::{CpError, CpResult};
use crate::options::Dereference;

/// Check if two paths refer to the same file (same device + inode).
pub fn is_same_file(src: &Path, dst: &Path) -> bool {
    same_file::is_same_file(src, dst).unwrap_or(false)
}

/// Strip trailing slashes from a path.
pub fn strip_trailing_slashes(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(trimmed)
    }
}

/// Determine the target path for a copy operation.
/// Returns (sources, target_dir_or_file).
pub fn resolve_target(
    paths: &[PathBuf],
    target_dir: &Option<PathBuf>,
    no_target_dir: bool,
) -> CpResult<(Vec<PathBuf>, PathBuf)> {
    if let Some(dir) = target_dir {
        // -t DIR: all paths are sources
        if !dir.is_dir() {
            return Err(CpError::NotADirectory { path: dir.clone() });
        }
        return Ok((paths.to_vec(), dir.clone()));
    }

    match paths.len() {
        0 => Err(CpError::MissingOperand),
        1 => Err(CpError::MissingDestination {
            src: paths[0].to_string_lossy().into_owned(),
        }),
        _ => {
            let sources = paths[..paths.len() - 1].to_vec();
            let dest = paths[paths.len() - 1].clone();

            if sources.len() > 1 && !dest.is_dir() && !no_target_dir {
                return Err(CpError::NotADirectory { path: dest });
            }

            Ok((sources, dest))
        }
    }
}

/// Get the final destination path for a source file being copied.
pub fn build_dest_path(
    source: &Path,
    dest: &Path,
    dest_is_dir: bool,
    parents: bool,
) -> PathBuf {
    if dest_is_dir {
        if parents {
            // --parents: replicate full source path under dest
            // e.g., cp --parents a/b/c dest → dest/a/b/c
            dest.join(source.strip_prefix("/").unwrap_or(source))
        } else {
            dest.join(source.file_name().unwrap_or(source.as_ref()))
        }
    } else {
        dest.to_path_buf()
    }
}

/// Get file metadata, optionally following symlinks.
pub fn get_metadata(path: &Path, follow: bool) -> io::Result<fs::Metadata> {
    if follow {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    }
}

/// Check if we should follow symlinks for this path.
pub fn should_follow_symlink(
    _path: &Path,
    deref: Dereference,
    is_command_line_arg: bool,
) -> bool {
    match deref {
        Dereference::Always => true,
        Dereference::Never => false,
        Dereference::CommandLine => is_command_line_arg,
    }
}

/// Get the device ID of a path's filesystem.
pub fn get_device(path: &Path) -> io::Result<u64> {
    fs::metadata(path).map(|m| m.dev())
}

/// Check if a path is a directory, following symlinks.
pub fn path_is_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// Prompt user on stderr and read y/n.
pub fn prompt_yes(msg: &str) -> bool {
    eprint!("{}", msg);
    let mut buf = String::new();
    if io::stdin().read_line(&mut buf).is_ok() {
        let answer = buf.trim().to_lowercase();
        answer == "y" || answer == "yes"
    } else {
        false
    }
}
