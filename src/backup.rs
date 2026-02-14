use std::path::{Path, PathBuf};

use crate::options::BackupMode;

/// Make a backup of the destination file if it exists.
/// Returns the backup path if a backup was created.
pub fn make_backup(
    dest: &Path,
    mode: BackupMode,
    suffix: &str,
) -> Option<PathBuf> {
    if mode == BackupMode::None || !dest.exists() {
        return None;
    }

    let backup_path = match mode {
        BackupMode::Simple => simple_backup_path(dest, suffix),
        BackupMode::Numbered => numbered_backup_path(dest),
        BackupMode::Existing => {
            // If numbered backups already exist, make numbered; otherwise simple
            if has_numbered_backups(dest) {
                numbered_backup_path(dest)
            } else {
                simple_backup_path(dest, suffix)
            }
        }
        BackupMode::None => return None,
    };

    if std::fs::rename(dest, &backup_path).is_ok() {
        Some(backup_path)
    } else {
        None
    }
}

fn simple_backup_path(dest: &Path, suffix: &str) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn numbered_backup_path(dest: &Path) -> PathBuf {
    let mut n = 1u64;
    loop {
        let mut s = dest.as_os_str().to_os_string();
        s.push(format!(".~{}~", n));
        let candidate = PathBuf::from(s);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

fn has_numbered_backups(dest: &Path) -> bool {
    let parent = dest.parent().unwrap_or(Path::new("."));
    let name = match dest.file_name() {
        Some(n) => n.to_string_lossy(),
        None => return false,
    };

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_name = entry.file_name();
            let entry_str = entry_name.to_string_lossy();
            if entry_str.starts_with(name.as_ref())
                && entry_str.contains(".~")
                && entry_str.ends_with('~')
            {
                return true;
            }
        }
    }

    false
}
