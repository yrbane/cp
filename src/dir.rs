use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};

/// Convert raw bytes to OsStr (safe wrapper — bytes come from kernel dirent).
#[inline]
fn bytes_to_os(b: &[u8]) -> &OsStr {
    unsafe { OsStr::from_encoded_bytes_unchecked(b) }
}

use indicatif::ProgressBar;
use walkdir::WalkDir;

use crate::copy;
use crate::error::{CpError, CpResult};
use crate::metadata;
use crate::options::{CopyOptions, Dereference};
use crate::progress;
use crate::util;

/// Max chunk for copy_file_range (1 GiB — will return actual bytes for small files).
const CFR_MAX: usize = 1024 * 1024 * 1024;

/// Copy a directory recursively.
pub fn copy_directory(src: &Path, dst: &Path, opts: &CopyOptions) -> CpResult<()> {
    // Check for copy-into-self
    if dst.starts_with(src) && dst != src {
        return Err(CpError::CopyIntoSelf {
            path: src.to_path_buf(),
            dest: dst.to_path_buf(),
        });
    }

    // Fast path: openat-based raw copy (no walkdir, no PathBuf allocations)
    if copy::is_simple_opts(opts) && opts.dereference != Dereference::Always {
        return copy_directory_raw(src, dst, opts);
    }

    // Slow path: walkdir-based copy for complex options
    copy_directory_walkdir(src, dst, opts)
}

/// State shared across the recursive raw copy.
struct RawCopyState<'a> {
    opts: &'a CopyOptions,
    hard_link_map: Option<HashMap<(u64, u64), PathBuf>>,
    src_dev: Option<u64>,
    need_file_meta: bool,
    need_dir_meta: bool,
    /// Deferred directory metadata: (src_path, dst_path, stat)
    dir_meta: Vec<(PathBuf, PathBuf, nix::libc::stat)>,
    /// Progress counter for directory copy
    progress: std::sync::Arc<progress::DirProgressCounter>,
}

/// Ultra-fast directory copy using raw libc: openat, readdir, mkdirat.
/// Zero PathBuf allocations in the hot path — paths only built for errors/metadata.
fn copy_directory_raw(src: &Path, dst: &Path, opts: &CopyOptions) -> CpResult<()> {
    // Create destination root
    if !dst.exists() {
        fs::create_dir_all(dst).map_err(|e| CpError::CreateDir {
            path: dst.to_path_buf(),
            source: e,
        })?;
    }

    let src_fd = open_dir_fd(src)?;
    let dst_fd = open_dir_fd(dst)?;

    let src_dev = if opts.one_file_system {
        Some(fstat_dev(src_fd))
    } else {
        None
    };

    let dir_pb = progress::make_dir_progress(&src.display().to_string(), opts.progress);
    let progress_counter = std::sync::Arc::new(progress::DirProgressCounter::new(dir_pb));

    let mut state = RawCopyState {
        opts,
        hard_link_map: if opts.preserve_links {
            Some(HashMap::new())
        } else {
            None
        },
        src_dev,
        need_file_meta: opts.preserve_mode
            || opts.preserve_ownership
            || opts.preserve_timestamps
            || opts.preserve_xattr
            || opts.preserve_acl,
        need_dir_meta: opts.preserve_mode || opts.preserve_ownership || opts.preserve_timestamps,
        dir_meta: Vec::new(),
        progress: progress_counter,
    };

    // Save root directory metadata if needed
    if state.need_dir_meta {
        let mut stat: nix::libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { nix::libc::fstat(src_fd, &mut stat) } == 0 {
            state
                .dir_meta
                .push((src.to_path_buf(), dst.to_path_buf(), stat));
        }
    }

    copy_dir_recurse(src_fd, dst_fd, src, dst, &mut state)?;

    unsafe {
        nix::libc::close(src_fd);
        nix::libc::close(dst_fd);
    }

    // Apply deferred directory metadata in reverse order (deepest first)
    for (src_path, dst_path, stat) in state.dir_meta.iter().rev() {
        apply_dir_metadata(dst_path, stat, state.opts)?;
        // xattr + ACL need path-based (only for directories, rare)
        if state.opts.preserve_xattr {
            metadata::preserve_xattr_pub(src_path, dst_path).ok();
        }
        if state.opts.preserve_acl {
            metadata::preserve_acl_pub(src_path, dst_path).ok();
        }
    }

    state.progress.finish();

    Ok(())
}

/// Minimum files in a directory to trigger parallel copy.
const PARALLEL_THRESHOLD: usize = 64;

/// Recurse into a directory using readdir + openat.
/// Files are copied in parallel using scoped threads when there are enough entries.
fn copy_dir_recurse(
    src_fd: RawFd,
    dst_fd: RawFd,
    src_path: &Path,
    dst_path: &Path,
    state: &mut RawCopyState,
) -> CpResult<()> {
    // dup the fd because fdopendir takes ownership
    let src_fd_dup = unsafe { nix::libc::dup(src_fd) };
    if src_fd_dup < 0 {
        return Err(CpError::OpenRead {
            path: src_path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }

    let dirp = unsafe { nix::libc::fdopendir(src_fd_dup) };
    if dirp.is_null() {
        unsafe { nix::libc::close(src_fd_dup) };
        return Err(CpError::OpenRead {
            path: src_path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }

    // Phase 1: Read all directory entries (readdir buffer is reused, so we must copy names)
    let mut reg_files: Vec<CString> = Vec::new();
    let mut symlinks: Vec<CString> = Vec::new();
    let mut subdirs: Vec<(RawFd, RawFd, PathBuf, PathBuf)> = Vec::new();
    let mut special_files: Vec<(CString, u8)> = Vec::new(); // (name, d_type)

    loop {
        unsafe { *nix::libc::__errno_location() = 0 };
        let entry = unsafe { nix::libc::readdir(dirp) };
        if entry.is_null() {
            break;
        }

        let d_type = unsafe { (*entry).d_type };
        let d_name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        let name_bytes = d_name.to_bytes();

        if name_bytes == b"." || name_bytes == b".." {
            continue;
        }

        match d_type {
            nix::libc::DT_REG => {
                reg_files.push(d_name.to_owned());
            }
            nix::libc::DT_LNK => {
                symlinks.push(d_name.to_owned());
            }
            nix::libc::DT_DIR => {
                // One-file-system check
                if let Some(dev) = state.src_dev {
                    let mut stat: nix::libc::stat = unsafe { std::mem::zeroed() };
                    if unsafe {
                        nix::libc::fstatat(
                            src_fd,
                            d_name.as_ptr(),
                            &mut stat,
                            nix::libc::AT_SYMLINK_NOFOLLOW,
                        )
                    } == 0
                        && stat.st_dev != dev
                    {
                        continue;
                    }
                }

                // mkdirat — single syscall, ignore EEXIST
                let ret = unsafe { nix::libc::mkdirat(dst_fd, d_name.as_ptr(), 0o777) };
                if ret != 0 {
                    let err = std::io::Error::last_os_error();
                    if err.raw_os_error() != Some(nix::libc::EEXIST) {
                        unsafe { nix::libc::closedir(dirp) };
                        return Err(CpError::CreateDir {
                            path: dst_path.join(bytes_to_os(name_bytes)),
                            source: err,
                        });
                    }
                }

                let child_src_fd = unsafe {
                    nix::libc::openat(
                        src_fd,
                        d_name.as_ptr(),
                        nix::libc::O_RDONLY | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC,
                    )
                };
                let child_dst_fd = unsafe {
                    nix::libc::openat(
                        dst_fd,
                        d_name.as_ptr(),
                        nix::libc::O_RDONLY | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC,
                    )
                };

                if child_src_fd >= 0 && child_dst_fd >= 0 {
                    let child_src = src_path.join(bytes_to_os(name_bytes));
                    let child_dst = dst_path.join(bytes_to_os(name_bytes));

                    if state.need_dir_meta {
                        let mut stat: nix::libc::stat = unsafe { std::mem::zeroed() };
                        if unsafe { nix::libc::fstat(child_src_fd, &mut stat) } == 0 {
                            state
                                .dir_meta
                                .push((child_src.clone(), child_dst.clone(), stat));
                        }
                    }

                    subdirs.push((child_src_fd, child_dst_fd, child_src, child_dst));
                } else {
                    if child_src_fd >= 0 {
                        unsafe { nix::libc::close(child_src_fd) };
                    }
                    if child_dst_fd >= 0 {
                        unsafe { nix::libc::close(child_dst_fd) };
                    }
                }
            }
            nix::libc::DT_FIFO | nix::libc::DT_CHR | nix::libc::DT_BLK => {
                special_files.push((d_name.to_owned(), d_type));
            }
            nix::libc::DT_SOCK => {
                eprintln!(
                    "cp: warning: cannot copy socket '{}'",
                    src_path.join(bytes_to_os(name_bytes)).display()
                );
            }
            _ => {}
        }
    }

    unsafe { nix::libc::closedir(dirp) };

    // Phase 2: Copy regular files — parallel when enough entries
    if reg_files.len() >= PARALLEL_THRESHOLD {
        copy_files_parallel(&reg_files, src_fd, dst_fd, src_path, dst_path, state)?;
    } else {
        for name in &reg_files {
            copy_file_openat(src_fd, dst_fd, name.as_c_str(), src_path, dst_path, state)?;
            state.progress.inc();
        }
    }

    if state.opts.verbose {
        for name in &reg_files {
            let nb = name.as_bytes();
            println!(
                "'{}' -> '{}'",
                src_path.join(bytes_to_os(nb)).display(),
                dst_path.join(bytes_to_os(nb)).display()
            );
        }
    }

    // Phase 3: Create special files (FIFOs, devices)
    for (name, dtype) in &special_files {
        let name_os = bytes_to_os(name.as_bytes());
        let src_special = src_path.join(name_os);
        let dst_special = dst_path.join(name_os);

        // fstatat to get mode and rdev
        let mut stat: nix::libc::stat = unsafe { std::mem::zeroed() };
        if unsafe {
            nix::libc::fstatat(
                src_fd,
                name.as_ptr(),
                &mut stat,
                nix::libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            eprintln!(
                "cp: cannot stat '{}': {}",
                src_special.display(),
                std::io::Error::last_os_error()
            );
            continue;
        }

        // Remove existing destination if any
        unsafe {
            nix::libc::unlinkat(dst_fd, name.as_ptr(), 0);
        }

        let ret = if *dtype == nix::libc::DT_FIFO {
            unsafe { nix::libc::mkfifoat(dst_fd, name.as_ptr(), stat.st_mode & 0o7777) }
        } else {
            let sflag = if *dtype == nix::libc::DT_BLK {
                nix::libc::S_IFBLK
            } else {
                nix::libc::S_IFCHR
            };
            unsafe {
                nix::libc::mknodat(
                    dst_fd,
                    name.as_ptr(),
                    sflag | (stat.st_mode & 0o7777),
                    stat.st_rdev,
                )
            }
        };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            // Tolerate EPERM for device nodes (non-root)
            if err.raw_os_error() != Some(nix::libc::EPERM) {
                return Err(CpError::MkNod {
                    path: dst_special,
                    source: nix::Error::last(),
                });
            }
        }

        if state.opts.verbose {
            println!("'{}' -> '{}'", src_special.display(), dst_special.display());
        }
        state.progress.inc();
    }

    // Phase 4: Copy symlinks (sequential — usually few)
    for name in &symlinks {
        copy_symlink_at(
            src_fd,
            dst_fd,
            name.as_c_str(),
            src_path,
            dst_path,
            state.opts,
        )?;
        state.progress.inc();
    }

    // Phase 4: Recurse into subdirectories
    for (child_src_fd, child_dst_fd, child_src, child_dst) in subdirs {
        copy_dir_recurse(child_src_fd, child_dst_fd, &child_src, &child_dst, state)?;
        unsafe {
            nix::libc::close(child_src_fd);
            nix::libc::close(child_dst_fd);
        }
    }

    Ok(())
}

/// Copy a regular file using openat (relative to directory fd).
/// No PathBuf allocation in the common case (paths only built on error).
fn copy_file_openat(
    src_dir_fd: RawFd,
    dst_dir_fd: RawFd,
    name: &CStr,
    src_dir_path: &Path,
    dst_dir_path: &Path,
    state: &mut RawCopyState,
) -> CpResult<()> {
    // openat: relative to directory fd — no path resolution
    let src_fd = unsafe {
        nix::libc::openat(
            src_dir_fd,
            name.as_ptr(),
            nix::libc::O_RDONLY | nix::libc::O_CLOEXEC,
        )
    };
    if src_fd < 0 {
        let name_os = bytes_to_os(name.to_bytes());
        return Err(CpError::OpenRead {
            path: src_dir_path.join(name_os),
            source: std::io::Error::last_os_error(),
        });
    }

    // fstat for metadata + hard link tracking — one syscall serves both
    let stat = if state.need_file_meta || state.hard_link_map.is_some() {
        let mut stat: nix::libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { nix::libc::fstat(src_fd, &mut stat) } != 0 {
            unsafe { nix::libc::close(src_fd) };
            let name_os = bytes_to_os(name.to_bytes());
            return Err(CpError::Stat {
                path: src_dir_path.join(name_os),
                source: std::io::Error::last_os_error(),
            });
        }
        Some(stat)
    } else {
        None
    };

    // Hard link detection using the fstat we already did
    if let Some(hlmap) = state.hard_link_map.as_mut()
        && let Some(ref s) = stat
        && s.st_nlink > 1
    {
        let key = (s.st_dev, s.st_ino);
        let name_os = bytes_to_os(name.to_bytes());
        let dst_file_path = dst_dir_path.join(name_os);
        if let Some(first_dest) = hlmap.get(&key) {
            unsafe { nix::libc::close(src_fd) };
            // unlinkat + linkat relative to dir fd
            unsafe {
                nix::libc::unlinkat(dst_dir_fd, name.as_ptr(), 0);
            }
            fs::hard_link(first_dest, &dst_file_path).map_err(|e| CpError::HardLink {
                src: first_dest.clone(),
                dst: dst_file_path,
                source: e,
            })?;
            return Ok(());
        }
        hlmap.insert(key, dst_file_path);
    }

    // Create destination: openat relative to dir fd
    let dst_fd = unsafe {
        nix::libc::openat(
            dst_dir_fd,
            name.as_ptr(),
            nix::libc::O_WRONLY | nix::libc::O_CREAT | nix::libc::O_TRUNC | nix::libc::O_CLOEXEC,
            0o666,
        )
    };
    if dst_fd < 0 {
        let err = std::io::Error::last_os_error();
        if state.opts.force {
            // Try unlink + recreate
            unsafe { nix::libc::unlinkat(dst_dir_fd, name.as_ptr(), 0) };
            let dst_fd2 = unsafe {
                nix::libc::openat(
                    dst_dir_fd,
                    name.as_ptr(),
                    nix::libc::O_WRONLY
                        | nix::libc::O_CREAT
                        | nix::libc::O_TRUNC
                        | nix::libc::O_CLOEXEC,
                    0o666,
                )
            };
            if dst_fd2 < 0 {
                unsafe { nix::libc::close(src_fd) };
                let name_os = bytes_to_os(name.to_bytes());
                return Err(CpError::CreateFile {
                    path: dst_dir_path.join(name_os),
                    source: std::io::Error::last_os_error(),
                });
            }
            // Continue with dst_fd2
            copy_and_close(src_fd, dst_fd2, stat.as_ref(), state)?;
            return Ok(());
        }
        unsafe { nix::libc::close(src_fd) };
        let name_os = bytes_to_os(name.to_bytes());
        return Err(CpError::CreateFile {
            path: dst_dir_path.join(name_os),
            source: err,
        });
    }

    copy_and_close(src_fd, dst_fd, stat.as_ref(), state)
}

/// Copy regular files in parallel using scoped threads.
/// Temporarily takes `hard_link_map` out of `state` for thread-safe Mutex wrapping,
/// then puts it back after all threads join.
fn copy_files_parallel(
    files: &[CString],
    src_fd: RawFd,
    dst_fd: RawFd,
    src_path: &Path,
    dst_path: &Path,
    state: &mut RawCopyState,
) -> CpResult<()> {
    use std::sync::Mutex;

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    let chunk_size = files.len().div_ceil(n_threads);

    // Take hard_link_map out so the rest of state is immutable + Sync
    let hlmap = state.hard_link_map.take().map(Mutex::new);
    let state_ref: &RawCopyState = &*state;
    let first_err: Mutex<Option<CpError>> = Mutex::new(None);
    // Deferred hard links: created after all files are copied to avoid races
    let deferred_links: Mutex<Vec<(PathBuf, PathBuf)>> = Mutex::new(Vec::new());

    let hlmap_ref = hlmap.as_ref();
    let err_ref = &first_err;
    let deferred_ref = &deferred_links;
    let progress_ref = &state.progress;

    std::thread::scope(|scope| {
        for chunk in files.chunks(chunk_size) {
            scope.spawn(move || {
                for name in chunk {
                    if err_ref.lock().map_or(true, |g| g.is_some()) {
                        return;
                    }
                    if let Err(e) = copy_file_openat_mt(
                        src_fd,
                        dst_fd,
                        name.as_c_str(),
                        src_path,
                        dst_path,
                        state_ref,
                        hlmap_ref,
                        deferred_ref,
                    ) {
                        let mut g = err_ref.lock().unwrap();
                        if g.is_none() {
                            *g = Some(e);
                        }
                        return;
                    }
                    progress_ref.inc();
                }
            });
        }
    });

    // Restore hard_link_map
    state.hard_link_map = hlmap.map(|m| m.into_inner().unwrap());

    if let Some(e) = first_err.into_inner().unwrap() {
        return Err(e);
    }

    // Phase 2: Create deferred hard links now that all originals exist
    for (src, dst) in deferred_links.into_inner().unwrap() {
        // Remove any placeholder file created by parallel copy
        let _ = fs::remove_file(&dst);
        fs::hard_link(&src, &dst).map_err(|e| CpError::HardLink {
            src: src.clone(),
            dst: dst.clone(),
            source: e,
        })?;
    }

    Ok(())
}

/// Thread-safe file copy via openat. Like `copy_file_openat` but uses Mutex for hard link map.
/// Hard links are deferred: the first occurrence of an inode is copied normally and registered
/// in the map; subsequent occurrences push to `deferred_links` for creation after all copies finish.
#[allow(clippy::too_many_arguments)]
fn copy_file_openat_mt(
    src_dir_fd: RawFd,
    dst_dir_fd: RawFd,
    name: &CStr,
    src_dir_path: &Path,
    dst_dir_path: &Path,
    state: &RawCopyState,
    hlmap: Option<&std::sync::Mutex<HashMap<(u64, u64), PathBuf>>>,
    deferred_links: &std::sync::Mutex<Vec<(PathBuf, PathBuf)>>,
) -> CpResult<()> {
    let src_fd = unsafe {
        nix::libc::openat(
            src_dir_fd,
            name.as_ptr(),
            nix::libc::O_RDONLY | nix::libc::O_CLOEXEC,
        )
    };
    if src_fd < 0 {
        return Err(CpError::OpenRead {
            path: src_dir_path.join(bytes_to_os(name.to_bytes())),
            source: std::io::Error::last_os_error(),
        });
    }

    let stat = if state.need_file_meta || hlmap.is_some() {
        let mut st: nix::libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { nix::libc::fstat(src_fd, &mut st) } != 0 {
            unsafe { nix::libc::close(src_fd) };
            return Err(CpError::Stat {
                path: src_dir_path.join(bytes_to_os(name.to_bytes())),
                source: std::io::Error::last_os_error(),
            });
        }
        Some(st)
    } else {
        None
    };

    // Hard link detection with Mutex — defer link creation to avoid race conditions
    if let Some(hlm) = hlmap
        && let Some(ref s) = stat
        && s.st_nlink > 1
    {
        let key = (s.st_dev, s.st_ino);
        let name_os = bytes_to_os(name.to_bytes());
        let dst_file = dst_dir_path.join(name_os);
        let mut guard = hlm.lock().unwrap();
        if let Some(first) = guard.get(&key) {
            // Another thread already claimed this inode — defer the hard link
            let first = first.clone();
            drop(guard);
            unsafe { nix::libc::close(src_fd) };
            deferred_links.lock().unwrap().push((first, dst_file));
            return Ok(());
        }
        // First occurrence: register in map, then copy the file below
        guard.insert(key, dst_file);
        drop(guard);
    }

    let dst_fd = unsafe {
        nix::libc::openat(
            dst_dir_fd,
            name.as_ptr(),
            nix::libc::O_WRONLY | nix::libc::O_CREAT | nix::libc::O_TRUNC | nix::libc::O_CLOEXEC,
            0o666,
        )
    };
    if dst_fd < 0 {
        let err = std::io::Error::last_os_error();
        if state.opts.force {
            unsafe { nix::libc::unlinkat(dst_dir_fd, name.as_ptr(), 0) };
            let dst_fd2 = unsafe {
                nix::libc::openat(
                    dst_dir_fd,
                    name.as_ptr(),
                    nix::libc::O_WRONLY
                        | nix::libc::O_CREAT
                        | nix::libc::O_TRUNC
                        | nix::libc::O_CLOEXEC,
                    0o666,
                )
            };
            if dst_fd2 < 0 {
                unsafe { nix::libc::close(src_fd) };
                return Err(CpError::CreateFile {
                    path: dst_dir_path.join(bytes_to_os(name.to_bytes())),
                    source: std::io::Error::last_os_error(),
                });
            }
            return copy_and_close(src_fd, dst_fd2, stat.as_ref(), state);
        }
        unsafe { nix::libc::close(src_fd) };
        return Err(CpError::CreateFile {
            path: dst_dir_path.join(bytes_to_os(name.to_bytes())),
            source: err,
        });
    }

    copy_and_close(src_fd, dst_fd, stat.as_ref(), state)
}

/// Copy file data + metadata using raw fds, then close both.
#[inline]
fn copy_and_close(
    src_fd: RawFd,
    dst_fd: RawFd,
    stat: Option<&nix::libc::stat>,
    state: &RawCopyState,
) -> CpResult<()> {
    // Copy data: loop copy_file_range until EOF
    loop {
        let ret = unsafe {
            nix::libc::copy_file_range(
                src_fd,
                std::ptr::null_mut(),
                dst_fd,
                std::ptr::null_mut(),
                CFR_MAX,
                0,
            )
        };
        if ret <= 0 {
            break;
        }
    }

    // Preserve metadata using fd-based syscalls
    if state.need_file_meta
        && let Some(s) = stat
    {
        if state.opts.preserve_xattr {
            preserve_xattr_fd(src_fd, dst_fd);
        }
        if state.opts.preserve_ownership {
            unsafe {
                nix::libc::fchown(dst_fd, s.st_uid, s.st_gid);
            }
        }
        if state.opts.preserve_mode {
            unsafe {
                nix::libc::fchmod(dst_fd, s.st_mode);
            }
        }
        if state.opts.preserve_timestamps {
            let atime = nix::libc::timespec {
                tv_sec: s.st_atime,
                tv_nsec: s.st_atime_nsec,
            };
            let mtime = nix::libc::timespec {
                tv_sec: s.st_mtime,
                tv_nsec: s.st_mtime_nsec,
            };
            let times = [atime, mtime];
            unsafe {
                nix::libc::futimens(dst_fd, times.as_ptr());
            }
        }
        if state.opts.preserve_acl {
            preserve_acl_fd(src_fd, dst_fd);
        }
    }

    unsafe {
        nix::libc::close(src_fd);
        nix::libc::close(dst_fd);
    }

    Ok(())
}

/// Copy a symlink using readlinkat + symlinkat.
fn copy_symlink_at(
    src_dir_fd: RawFd,
    dst_dir_fd: RawFd,
    name: &CStr,
    src_dir_path: &Path,
    dst_dir_path: &Path,
    opts: &CopyOptions,
) -> CpResult<()> {
    let mut buf = [0u8; 4096];
    let len = unsafe {
        nix::libc::readlinkat(
            src_dir_fd,
            name.as_ptr(),
            buf.as_mut_ptr() as *mut nix::libc::c_char,
            buf.len(),
        )
    };
    if len < 0 {
        let name_os = bytes_to_os(name.to_bytes());
        return Err(CpError::ReadLink {
            path: src_dir_path.join(name_os),
            source: std::io::Error::last_os_error(),
        });
    }

    // Null-terminate the target
    let target_bytes = &buf[..len as usize];
    let mut target_z = Vec::with_capacity(target_bytes.len() + 1);
    target_z.extend_from_slice(target_bytes);
    target_z.push(0);

    // Remove existing symlink if present
    unsafe {
        nix::libc::unlinkat(dst_dir_fd, name.as_ptr(), 0);
    }

    // symlinkat
    let ret = unsafe {
        nix::libc::symlinkat(
            target_z.as_ptr() as *const nix::libc::c_char,
            dst_dir_fd,
            name.as_ptr(),
        )
    };
    if ret != 0 {
        let name_os = bytes_to_os(name.to_bytes());
        return Err(CpError::Symlink {
            dst: dst_dir_path.join(name_os),
            source: std::io::Error::last_os_error(),
        });
    }

    // Preserve symlink metadata if needed
    if opts.preserve_timestamps || opts.preserve_ownership {
        let name_os = bytes_to_os(name.to_bytes());
        let src_path = src_dir_path.join(name_os);
        let dst_path = dst_dir_path.join(name_os);
        if let Ok(meta) = fs::symlink_metadata(&src_path) {
            metadata::preserve_metadata(&src_path, &dst_path, &meta, opts, true)?;
        }
    }

    if opts.verbose {
        let name_os = bytes_to_os(name.to_bytes());
        println!(
            "'{}' -> '{}'",
            src_dir_path.join(name_os).display(),
            dst_dir_path.join(name_os).display()
        );
    }

    Ok(())
}

/// Apply deferred directory metadata from raw stat.
fn apply_dir_metadata(dst: &Path, stat: &nix::libc::stat, opts: &CopyOptions) -> CpResult<()> {
    if opts.preserve_ownership {
        let c_path = CString::new(dst.as_os_str().as_bytes()).ok();
        if let Some(c) = c_path {
            unsafe {
                nix::libc::chown(c.as_ptr(), stat.st_uid, stat.st_gid);
            }
        }
    }

    if opts.preserve_mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dst, fs::Permissions::from_mode(stat.st_mode)).ok();
    }

    if opts.preserve_timestamps {
        let atime = nix::libc::timespec {
            tv_sec: stat.st_atime,
            tv_nsec: stat.st_atime_nsec,
        };
        let mtime = nix::libc::timespec {
            tv_sec: stat.st_mtime,
            tv_nsec: stat.st_mtime_nsec,
        };
        let c_path = CString::new(dst.as_os_str().as_bytes()).ok();
        if let Some(c) = c_path {
            let times = [atime, mtime];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    Ok(())
}

/// Open a directory fd for openat operations.
fn open_dir_fd(path: &Path) -> CpResult<RawFd> {
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| CpError::OpenRead {
        path: path.to_path_buf(),
        source: std::io::Error::from_raw_os_error(nix::libc::EINVAL),
    })?;
    let fd = unsafe {
        nix::libc::open(
            c_path.as_ptr(),
            nix::libc::O_RDONLY | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(CpError::OpenRead {
            path: path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(fd)
}

/// Get device number from an open fd.
fn fstat_dev(fd: RawFd) -> u64 {
    let mut stat: nix::libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { nix::libc::fstat(fd, &mut stat) } == 0 {
        stat.st_dev
    } else {
        0
    }
}

// ─── Walkdir-based slow path (complex options) ──────────────────────────────

/// Walkdir-based directory copy for complex options (-i, -n, --backup, etc.)
fn copy_directory_walkdir(src: &Path, dst: &Path, opts: &CopyOptions) -> CpResult<()> {
    let follow_links = opts.dereference == Dereference::Always;

    let mut hard_link_map: Option<HashMap<(u64, u64), PathBuf>> = if opts.preserve_links {
        Some(HashMap::new())
    } else {
        None
    };

    let src_dev = if opts.one_file_system {
        Some(util::get_device(src).unwrap_or(0))
    } else {
        None
    };

    let need_dir_meta = opts.preserve_mode || opts.preserve_ownership || opts.preserve_timestamps;
    let mut dir_metadata: Vec<(PathBuf, PathBuf, fs::Metadata)> = Vec::new();

    let dir_pb = progress::make_dir_progress(&src.display().to_string(), opts.progress);
    let dir_progress = progress::DirProgressCounter::new(dir_pb);

    let mut pb: Option<ProgressBar> = None;

    let walker = WalkDir::new(src).follow_links(follow_links).min_depth(0);

    let mut dest_path = PathBuf::with_capacity(dst.as_os_str().len() + 64);
    let mut last_parent: Option<PathBuf> = None;

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                eprintln!("cp: {}", e);
                continue;
            }
        };

        let path = entry.path();
        let relative = match path.strip_prefix(src) {
            Ok(r) => r,
            Err(_) => path,
        };

        dest_path.clear();
        dest_path.push(dst);
        dest_path.push(relative);

        let ft = entry.file_type();

        if ft.is_dir() {
            if let Some(dev) = src_dev
                && let Ok(m) = fs::metadata(path)
                && m.dev() != dev
            {
                continue;
            }

            if !dest_path.exists() {
                fs::create_dir_all(&dest_path).map_err(|e| CpError::CreateDir {
                    path: dest_path.clone(),
                    source: e,
                })?;
            }

            if need_dir_meta {
                let meta = if follow_links {
                    fs::metadata(path)
                } else {
                    fs::symlink_metadata(path)
                };
                if let Ok(meta) = meta {
                    dir_metadata.push((path.to_path_buf(), dest_path.clone(), meta));
                }
            }

            continue;
        }

        if let Some(dev) = src_dev {
            let meta = if follow_links {
                fs::metadata(path)
            } else {
                fs::symlink_metadata(path)
            };
            if let Ok(meta) = meta
                && meta.dev() != dev
            {
                continue;
            }
        }

        if let Some(parent) = dest_path.parent() {
            let need_check = match last_parent {
                Some(ref lp) => lp != parent,
                None => true,
            };
            if need_check {
                if !parent.exists() {
                    fs::create_dir_all(parent).map_err(|e| CpError::CreateDir {
                        path: parent.to_path_buf(),
                        source: e,
                    })?;
                }
                last_parent = Some(parent.to_path_buf());
            }
        }

        // Handle hard links in slow path
        if let Some(ref mut hlmap) = hard_link_map
            && !ft.is_symlink()
            && let Ok(meta) = fs::symlink_metadata(path)
            && meta.nlink() > 1
        {
            let key = (meta.dev(), meta.ino());
            if let Some(first_dest) = hlmap.get(&key) {
                if dest_path.exists() {
                    let _ = fs::remove_file(&dest_path);
                }
                fs::hard_link(first_dest, &dest_path).map_err(|e| CpError::HardLink {
                    src: first_dest.clone(),
                    dst: dest_path.clone(),
                    source: e,
                })?;
                continue;
            }
            hlmap.insert(key, dest_path.clone());
        }

        let slow_pb = pb.get_or_insert_with(ProgressBar::hidden);
        copy::copy_single(path, &dest_path, opts, false, slow_pb)?;
        dir_progress.inc();
    }

    dir_progress.finish();

    for (src_path, dst_path, meta) in dir_metadata.iter().rev() {
        metadata::preserve_metadata(src_path, dst_path, meta, opts, false)?;
    }

    Ok(())
}

// ─── fd-based helpers ────────────────────────────────────────────────────────

/// Preserve xattrs using fd-based syscalls (no path resolution).
fn preserve_xattr_fd(src_fd: i32, dst_fd: i32) {
    use nix::libc::{c_char, c_void, fgetxattr, flistxattr, fsetxattr, ssize_t};

    let size: ssize_t = unsafe { flistxattr(src_fd, std::ptr::null_mut(), 0) };
    if size <= 0 {
        return;
    }

    let mut list = vec![0u8; size as usize];
    let size = unsafe { flistxattr(src_fd, list.as_mut_ptr() as *mut c_char, list.len()) };
    if size <= 0 {
        return;
    }

    let mut val_buf: Vec<u8> = Vec::with_capacity(256);

    for name in list[..size as usize].split(|&b| b == 0) {
        if name.is_empty() {
            continue;
        }

        let mut name_z = Vec::with_capacity(name.len() + 1);
        name_z.extend_from_slice(name);
        name_z.push(0);
        let name_ptr = name_z.as_ptr() as *const c_char;

        let val_size = unsafe { fgetxattr(src_fd, name_ptr, std::ptr::null_mut(), 0) };
        if val_size < 0 {
            continue;
        }

        if val_size == 0 {
            unsafe { fsetxattr(dst_fd, name_ptr, std::ptr::null(), 0, 0) };
            continue;
        }

        val_buf.resize(val_size as usize, 0);
        let got = unsafe {
            fgetxattr(
                src_fd,
                name_ptr,
                val_buf.as_mut_ptr() as *mut c_void,
                val_buf.len(),
            )
        };
        if got < 0 {
            continue;
        }

        unsafe {
            fsetxattr(
                dst_fd,
                name_ptr,
                val_buf.as_ptr() as *const c_void,
                got as usize,
                0,
            );
        }
    }
}

/// Preserve ACL using fd-based syscalls (no path resolution).
fn preserve_acl_fd(src_fd: i32, dst_fd: i32) {
    unsafe extern "C" {
        fn acl_get_fd(fd: i32) -> *mut std::ffi::c_void;
        fn acl_set_fd(fd: i32, acl: *mut std::ffi::c_void) -> i32;
        fn acl_free(obj_p: *mut std::ffi::c_void) -> i32;
    }

    let acl = unsafe { acl_get_fd(src_fd) };
    if acl.is_null() {
        return;
    }

    unsafe {
        acl_set_fd(dst_fd, acl);
        acl_free(acl);
    }
}
