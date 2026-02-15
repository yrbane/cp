use std::fs::{self, File};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

use indicatif::ProgressBar;

use crate::backup;
use crate::cli::{SparseMode, UpdateMode};
use crate::engine;
use crate::error::{CpError, CpResult};
use crate::metadata;
use crate::options::CopyOptions;
use crate::sparse;
use crate::util;

/// Threshold below which we skip sparse detection (no holes in tiny files).
pub const SPARSE_THRESHOLD: u64 = 32 * 1024;

/// Check if options are "simple" — no special flags that require per-file checks.
pub fn is_simple_opts(opts: &CopyOptions) -> bool {
    !opts.interactive
        && !opts.no_clobber
        && !opts.remove_destination
        && opts.update.is_none()
        && opts.backup == crate::options::BackupMode::None
        && !opts.hard_link
        && !opts.symbolic_link
        && !opts.attributes_only
}

/// Copy a single file (regular, symlink, or special).
/// `is_cli_arg`: whether source was specified on command line (affects -H).
pub fn copy_single(
    src: &Path,
    dst: &Path,
    opts: &CopyOptions,
    is_cli_arg: bool,
    pb: &ProgressBar,
) -> CpResult<()> {
    let follow = util::should_follow_symlink(src, opts.dereference, is_cli_arg);
    let src_meta = util::get_metadata(src, follow).map_err(|e| CpError::Stat {
        path: src.to_path_buf(),
        source: e,
    })?;

    // Single stat on dest — cache the result to avoid repeated exists()/metadata() calls
    let dst_meta = fs::symlink_metadata(dst).ok();
    let dst_exists = dst_meta.is_some();

    // Dangling symlink check: if dest is a symlink pointing nowhere,
    // refuse to write through it unless --force or --remove-destination
    if let Some(ref dm) = dst_meta {
        if dm.file_type().is_symlink() && !dst.exists() && !opts.force && !opts.remove_destination {
            return Err(CpError::DanglingSymlink {
                path: dst.to_path_buf(),
            });
        }
    }

    // Same file check
    if dst_exists && util::is_same_file(src, dst) {
        return Err(CpError::SameFile {
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
        });
    }

    // Update check
    if let Some(update_mode) = opts.update
        && dst_exists
    {
        match update_mode {
            UpdateMode::None | UpdateMode::NoneFail => return Ok(()),
            UpdateMode::Older => {
                if let Some(ref dm) = dst_meta
                    && dm.modified().ok() >= src_meta.modified().ok()
                {
                    return Ok(());
                }
            }
            UpdateMode::All => {} // always copy
        }
    }

    // No-clobber check
    if opts.no_clobber && dst_exists {
        return Ok(());
    }

    // Interactive check
    if opts.interactive
        && dst_exists
        && !util::prompt_yes(&format!("cp: overwrite '{}'? ", dst.display()))
    {
        return Ok(());
    }

    // Backup
    if dst_exists {
        backup::make_backup(dst, opts.backup, &opts.backup_suffix);
    }

    // Remove destination if requested
    if opts.remove_destination && dst_exists {
        fs::remove_file(dst)
            .or_else(|_| fs::remove_dir_all(dst))
            .map_err(|e| CpError::Remove {
                path: dst.to_path_buf(),
                source: e,
            })?;
    }

    let file_type = src_meta.file_type();

    if file_type.is_symlink() && !follow {
        copy_symlink(src, dst, &src_meta, opts)?;
    } else if file_type.is_dir() || (follow && src.is_dir()) {
        return Err(CpError::OmitDirectory {
            path: src.to_path_buf(),
        });
    } else if file_type.is_file() || (follow && src.is_file()) {
        copy_regular_file(src, dst, &src_meta, opts, pb)?;
    } else if file_type.is_fifo() {
        copy_fifo(dst, &src_meta, opts)?;
    } else if file_type.is_block_device() || file_type.is_char_device() {
        copy_device(dst, &src_meta, opts)?;
    } else if file_type.is_socket() {
        eprintln!("cp: warning: cannot copy socket '{}'", src.display());
    } else {
        copy_regular_file(src, dst, &src_meta, opts, pb)?;
    }

    if opts.verbose {
        eprintln!("'{}' -> '{}'", src.display(), dst.display());
    }

    Ok(())
}

fn copy_regular_file(
    src: &Path,
    dst: &Path,
    src_meta: &fs::Metadata,
    opts: &CopyOptions,
    pb: &ProgressBar,
) -> CpResult<()> {
    if opts.hard_link {
        return do_hard_link(src, dst);
    }

    if opts.symbolic_link {
        return do_symbolic_link(src, dst);
    }

    if opts.attributes_only {
        if !dst.exists() {
            File::create(dst).map_err(|e| CpError::CreateFile {
                path: dst.to_path_buf(),
                source: e,
            })?;
        }
        metadata::preserve_metadata(src, dst, src_meta, opts, false)?;
        return Ok(());
    }

    let size = src_meta.len();

    // Open source
    let src_file = File::open(src).map_err(|e| CpError::OpenRead {
        path: src.to_path_buf(),
        source: e,
    })?;

    // Open destination — File::create does open+truncate in one syscall
    let dst_file = open_dest_create(dst, opts)?;

    if size > 0 {
        // Skip sparse detection for small files — no meaningful holes
        let use_sparse = opts.sparse != SparseMode::Never && size >= SPARSE_THRESHOLD;

        if use_sparse {
            let mut src_f = src_file;
            let mut dst_f = dst_file;
            if sparse::copy_sparse(&mut src_f, &mut dst_f, size, src, dst, opts.sparse, pb)? {
                if opts.debug {
                    eprintln!("cp: copy method: sparse (SEEK_HOLE/SEEK_DATA)");
                }
                metadata::preserve_metadata(src, dst, src_meta, opts, false)?;
                return Ok(());
            }

            // Sparse didn't handle it, reopen and do normal copy
            drop(src_f);
            drop(dst_f);
            let src_file = File::open(src).map_err(|e| CpError::OpenRead {
                path: src.to_path_buf(),
                source: e,
            })?;
            let dst_file = open_dest_create(dst, opts)?;

            let method =
                engine::copy_file_data(&src_file, &dst_file, size, src, dst, opts.reflink, pb)?;
            if opts.debug {
                eprintln!("cp: copy method: {}", method);
            }
        } else {
            let method =
                engine::copy_file_data(&src_file, &dst_file, size, src, dst, opts.reflink, pb)?;
            if opts.debug {
                eprintln!("cp: copy method: {}", method);
            }
        }
    }

    metadata::preserve_metadata(src, dst, src_meta, opts, false)?;
    Ok(())
}

/// Open dest with create+truncate in one syscall.
/// Falls back to force-remove+create if opts.force is set.
fn open_dest_create(dst: &Path, opts: &CopyOptions) -> CpResult<File> {
    match File::create(dst) {
        Ok(f) => Ok(f),
        Err(_e) if opts.force => {
            let _ = fs::remove_file(dst);
            File::create(dst).map_err(|e2| CpError::CreateFile {
                path: dst.to_path_buf(),
                source: e2,
            })
        }
        Err(e) => Err(CpError::CreateFile {
            path: dst.to_path_buf(),
            source: e,
        }),
    }
}

fn copy_symlink(
    src: &Path,
    dst: &Path,
    src_meta: &fs::Metadata,
    opts: &CopyOptions,
) -> CpResult<()> {
    let target = fs::read_link(src).map_err(|e| CpError::ReadLink {
        path: src.to_path_buf(),
        source: e,
    })?;

    if dst.exists() || dst.symlink_metadata().is_ok() {
        fs::remove_file(dst).map_err(|e| CpError::Remove {
            path: dst.to_path_buf(),
            source: e,
        })?;
    }

    std::os::unix::fs::symlink(&target, dst).map_err(|e| CpError::Symlink {
        dst: dst.to_path_buf(),
        source: e,
    })?;

    metadata::preserve_metadata(src, dst, src_meta, opts, true)?;

    Ok(())
}

fn copy_fifo(dst: &Path, src_meta: &fs::Metadata, opts: &CopyOptions) -> CpResult<()> {
    let mode = nix::sys::stat::Mode::from_bits_truncate(src_meta.mode());
    nix::unistd::mkfifo(dst, mode).map_err(|e| CpError::MkNod {
        path: dst.to_path_buf(),
        source: e,
    })?;

    metadata::preserve_metadata(dst, dst, src_meta, opts, false)?;

    Ok(())
}

fn copy_device(dst: &Path, src_meta: &fs::Metadata, opts: &CopyOptions) -> CpResult<()> {
    let mode = nix::sys::stat::Mode::from_bits_truncate(src_meta.mode());
    let dev = src_meta.rdev();

    let sflag = if src_meta.file_type().is_block_device() {
        nix::sys::stat::SFlag::S_IFBLK
    } else {
        nix::sys::stat::SFlag::S_IFCHR
    };

    nix::sys::stat::mknod(dst, sflag, mode, dev).map_err(|e| CpError::MkNod {
        path: dst.to_path_buf(),
        source: e,
    })?;

    metadata::preserve_metadata(dst, dst, src_meta, opts, false)?;

    Ok(())
}

fn do_hard_link(src: &Path, dst: &Path) -> CpResult<()> {
    if dst.exists() {
        fs::remove_file(dst).map_err(|e| CpError::Remove {
            path: dst.to_path_buf(),
            source: e,
        })?;
    }
    fs::hard_link(src, dst).map_err(|e| CpError::HardLink {
        src: src.to_path_buf(),
        dst: dst.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

fn do_symbolic_link(src: &Path, dst: &Path) -> CpResult<()> {
    if dst.exists() || dst.symlink_metadata().is_ok() {
        fs::remove_file(dst).map_err(|e| CpError::Remove {
            path: dst.to_path_buf(),
            source: e,
        })?;
    }
    let abs_src = if src.is_absolute() {
        src.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(src)
    };
    std::os::unix::fs::symlink(&abs_src, dst).map_err(|e| CpError::Symlink {
        dst: dst.to_path_buf(),
        source: e,
    })?;
    Ok(())
}
