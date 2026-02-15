use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{CpError, CpResult};
use crate::options::CopyOptions;

const ENOTSUP: i32 = 95; // linux ENOTSUP

/// Cached: are we running as root? (checked once)
static IS_ROOT: AtomicBool = AtomicBool::new(false);
static ROOT_CHECKED: AtomicBool = AtomicBool::new(false);

pub fn is_root() -> bool {
    if !ROOT_CHECKED.load(Ordering::Relaxed) {
        IS_ROOT.store(unsafe { nix::libc::geteuid() } == 0, Ordering::Relaxed);
        ROOT_CHECKED.store(true, Ordering::Relaxed);
    }
    IS_ROOT.load(Ordering::Relaxed)
}

/// Cached: does the filesystem support xattr? (reset on ENOTSUP)
static XATTR_SUPPORTED: AtomicBool = AtomicBool::new(true);

/// Cached: does the filesystem support ACLs?
static ACL_SUPPORTED: AtomicBool = AtomicBool::new(true);

/// Preserve metadata from source to destination.
/// Order matters: xattr -> chown -> chmod -> utimensat -> ACL
pub fn preserve_metadata(
    src: &Path,
    dst: &Path,
    src_meta: &fs::Metadata,
    opts: &CopyOptions,
    is_symlink: bool,
) -> CpResult<()> {
    // 1. Extended attributes (before chown which may strip them)
    if opts.preserve_xattr && XATTR_SUPPORTED.load(Ordering::Relaxed) {
        preserve_xattr(src, dst)?;
    }

    // 2. Ownership (before chmod, since chown can clear setuid/setgid)
    // Skip entirely for non-root — chown always fails with EPERM
    if opts.preserve_ownership && is_root() {
        preserve_ownership(dst, src_meta, is_symlink)?;
    }

    // 3. Permissions
    if opts.preserve_mode && !is_symlink {
        preserve_mode(dst, src_meta)?;
    }

    // 4. Timestamps
    if opts.preserve_timestamps {
        preserve_timestamps(dst, src_meta, is_symlink)?;
    }

    // 5. ACL (includes POSIX permission bits — may override mode)
    if opts.preserve_acl && ACL_SUPPORTED.load(Ordering::Relaxed) {
        // ACL entries include the POSIX permission bits (owner/group/other).
        // If mode is NOT being preserved, save the current mode and restore after ACL.
        let saved_mode = if !opts.preserve_mode && !is_symlink {
            fs::metadata(dst).ok().map(|m| m.mode() & 0o7777)
        } else {
            None
        };

        preserve_acl(src, dst)?;

        if let Some(mode) = saved_mode {
            fs::set_permissions(dst, fs::Permissions::from_mode(mode)).ok();
        }
    }

    Ok(())
}

/// Public wrapper for xattr preservation (used by dir.rs fast path).
pub fn preserve_xattr_pub(src: &Path, dst: &Path) -> CpResult<()> {
    if !XATTR_SUPPORTED.load(Ordering::Relaxed) {
        return Ok(());
    }
    preserve_xattr(src, dst)
}

fn preserve_xattr(src: &Path, dst: &Path) -> CpResult<()> {
    match xattr::list(src) {
        Ok(attrs) => {
            for attr in attrs {
                match xattr::get(src, &attr) {
                    Ok(Some(value)) => {
                        if let Err(e) = xattr::set(dst, &attr, &value) {
                            if e.raw_os_error() == Some(ENOTSUP) {
                                XATTR_SUPPORTED.store(false, Ordering::Relaxed);
                                return Ok(());
                            }
                            // Non-fatal for permission denied
                            if e.kind() != std::io::ErrorKind::PermissionDenied {
                                return Err(CpError::Xattr {
                                    path: dst.to_path_buf(),
                                    source: e,
                                });
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::PermissionDenied {
                            return Err(CpError::Xattr {
                                path: src.to_path_buf(),
                                source: e,
                            });
                        }
                    }
                }
            }
        }
        Err(e) => {
            if e.raw_os_error() == Some(ENOTSUP) {
                XATTR_SUPPORTED.store(false, Ordering::Relaxed);
                return Ok(());
            }
            if e.kind() != std::io::ErrorKind::PermissionDenied {
                return Err(CpError::Xattr {
                    path: src.to_path_buf(),
                    source: e,
                });
            }
        }
    }
    Ok(())
}

fn preserve_ownership(dst: &Path, meta: &fs::Metadata, is_symlink: bool) -> CpResult<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let uid = meta.uid();
    let gid = meta.gid();

    let c_path = CString::new(dst.as_os_str().as_bytes()).map_err(|_| CpError::Chown {
        path: dst.to_path_buf(),
        source: nix::Error::EINVAL,
    })?;

    let ret = if is_symlink {
        unsafe { nix::libc::lchown(c_path.as_ptr(), uid, gid) }
    } else {
        unsafe { nix::libc::chown(c_path.as_ptr(), uid, gid) }
    };

    if ret != 0 {
        let err = nix::Error::last();
        if err != nix::Error::EPERM {
            return Err(CpError::Chown {
                path: dst.to_path_buf(),
                source: err,
            });
        }
    }

    Ok(())
}

fn preserve_mode(dst: &Path, meta: &fs::Metadata) -> CpResult<()> {
    let mode = meta.mode();
    fs::set_permissions(dst, fs::Permissions::from_mode(mode)).map_err(|e| CpError::Chmod {
        path: dst.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

fn preserve_timestamps(dst: &Path, meta: &fs::Metadata, is_symlink: bool) -> CpResult<()> {
    let atime = filetime::FileTime::from_last_access_time(meta);
    let mtime = filetime::FileTime::from_last_modification_time(meta);

    let result = if is_symlink {
        filetime::set_symlink_file_times(dst, atime, mtime)
    } else {
        filetime::set_file_times(dst, atime, mtime)
    };

    result.map_err(|e| CpError::Timestamps {
        path: dst.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

/// Public wrapper for ACL preservation (used by dir.rs fast path).
pub fn preserve_acl_pub(src: &Path, dst: &Path) -> CpResult<()> {
    if !ACL_SUPPORTED.load(Ordering::Relaxed) {
        return Ok(());
    }
    preserve_acl(src, dst)
}

fn preserve_acl(src: &Path, dst: &Path) -> CpResult<()> {
    match posix_acl::PosixACL::read_acl(src) {
        Ok(mut acl) => {
            if let Err(e) = acl.write_acl(dst) {
                let msg = e.to_string();
                if msg.contains("not supported") || msg.contains("Operation not supported") {
                    ACL_SUPPORTED.store(false, Ordering::Relaxed);
                    return Ok(());
                }
                return Err(CpError::Acl {
                    path: dst.to_path_buf(),
                    msg,
                });
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not supported") || msg.contains("No data available") {
                ACL_SUPPORTED.store(false, Ordering::Relaxed);
                return Ok(());
            }
            return Err(CpError::Acl {
                path: src.to_path_buf(),
                msg,
            });
        }
    }

    // Also try default ACL for directories
    if src.is_dir()
        && let Ok(mut acl) = posix_acl::PosixACL::read_default_acl(src)
    {
        let _ = acl.write_default_acl(dst);
    }

    Ok(())
}
