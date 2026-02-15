use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use indicatif::ProgressBar;

use crate::cli::ReflinkMode;
use crate::error::{CpError, CpResult};

/// Size of chunks for copy_file_range (64 MiB).
const COPY_FILE_RANGE_CHUNK: usize = 64 * 1024 * 1024;

/// Size of chunks for sendfile (64 MiB).
const SENDFILE_CHUNK: usize = 64 * 1024 * 1024;

/// Buffer size for read/write fallback (256 KiB).
const RW_BUF_SIZE: usize = 256 * 1024;

/// FICLONE ioctl number (from linux/fs.h: _IOW(0x94, 9, int))
const FICLONE: nix::libc::c_ulong = 0x40049409;

/// Threshold below which FICLONE is skipped for reflink=auto.
/// The ioctl overhead isn't worth it for tiny files on non-CoW fs.
const FICLONE_THRESHOLD: u64 = 256 * 1024;

/// Copy file data using the optimal kernel mechanism.
/// Returns the method used as a string (for --debug).
pub fn copy_file_data(
    src: &File,
    dst: &File,
    size: u64,
    src_path: &Path,
    dst_path: &Path,
    reflink: ReflinkMode,
    pb: &ProgressBar,
) -> CpResult<&'static str> {
    // Step 1: Try FICLONE (reflink/CoW)
    // Skip for small files with reflink=auto — the ioctl syscall cost isn't worthwhile
    let try_reflink = match reflink {
        ReflinkMode::Never => false,
        ReflinkMode::Always => true,
        ReflinkMode::Auto => size >= FICLONE_THRESHOLD,
    };
    if try_reflink {
        match try_ficlone(src, dst) {
            Ok(()) => {
                pb.inc(size);
                return Ok("reflink (FICLONE)");
            }
            Err(_) if reflink == ReflinkMode::Always => {
                return Err(CpError::Copy {
                    src: src_path.to_path_buf(),
                    dst: dst_path.to_path_buf(),
                    reason: "failed to clone: Operation not supported".into(),
                });
            }
            Err(_) => {} // fall through
        }
    }

    // Step 2: Try copy_file_range (zero-copy kernel)
    match try_copy_file_range(src, dst, size, pb) {
        Ok(copied) if copied == size => return Ok("copy_file_range"),
        Ok(copied) if copied > 0 => {
            // Partial success, finish with sendfile or read/write
            let remaining = size - copied;
            if try_sendfile(src, dst, remaining, pb).is_ok() {
                return Ok("copy_file_range+sendfile");
            }
            do_read_write(src, dst, src_path, dst_path, pb)?;
            return Ok("copy_file_range+read/write");
        }
        _ => {}
    }

    // Step 3: Try sendfile
    if try_sendfile(src, dst, size, pb).is_ok() {
        return Ok("sendfile");
    }

    // Step 4: Fallback to read/write
    do_read_write(src, dst, src_path, dst_path, pb)?;
    Ok("read/write")
}

/// Try to clone via FICLONE ioctl.
fn try_ficlone(src: &File, dst: &File) -> Result<(), ()> {
    let ret = unsafe { nix::libc::ioctl(dst.as_raw_fd(), FICLONE, src.as_raw_fd()) };
    if ret == 0 { Ok(()) } else { Err(()) }
}

/// Try copy_file_range syscall in a loop, feeding progress.
fn try_copy_file_range(src: &File, dst: &File, size: u64, pb: &ProgressBar) -> Result<u64, ()> {
    let mut copied: u64 = 0;

    while copied < size {
        let chunk = std::cmp::min((size - copied) as usize, COPY_FILE_RANGE_CHUNK);
        let ret = unsafe {
            nix::libc::copy_file_range(
                src.as_raw_fd(),
                std::ptr::null_mut(),
                dst.as_raw_fd(),
                std::ptr::null_mut(),
                chunk,
                0,
            )
        };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            let errno = err.raw_os_error().unwrap_or(0);
            if errno == nix::libc::ENOSYS || errno == nix::libc::EXDEV || errno == nix::libc::EINVAL
            {
                if copied == 0 {
                    return Err(());
                }
                break;
            }
            if copied == 0 {
                return Err(());
            }
            break;
        } else if ret == 0 {
            break; // EOF
        } else {
            let n = ret as u64;
            copied += n;
            pb.inc(n);
        }
    }

    Ok(copied)
}

/// Try sendfile syscall in a loop, feeding progress.
fn try_sendfile(src: &File, dst: &File, size: u64, pb: &ProgressBar) -> Result<(), ()> {
    let mut remaining = size;

    while remaining > 0 {
        let chunk = std::cmp::min(remaining as usize, SENDFILE_CHUNK);
        let ret = unsafe {
            nix::libc::sendfile64(
                dst.as_raw_fd(),
                src.as_raw_fd(),
                std::ptr::null_mut(),
                chunk,
            )
        };
        if ret < 0 {
            if remaining == size {
                return Err(());
            }
            return Err(());
        } else if ret == 0 {
            break;
        } else {
            let n = ret as u64;
            remaining -= n;
            pb.inc(n);
        }
    }

    Ok(())
}

/// Fallback: read/write in userspace.
fn do_read_write(
    src: &File,
    dst: &File,
    src_path: &Path,
    dst_path: &Path,
    pb: &ProgressBar,
) -> CpResult<()> {
    let mut reader = src;
    let mut writer = dst;
    let mut buf = vec![0u8; RW_BUF_SIZE];

    loop {
        let n = reader.read(&mut buf).map_err(|e| CpError::Read {
            path: src_path.to_path_buf(),
            source: e,
        })?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).map_err(|e| CpError::Write {
            path: dst_path.to_path_buf(),
            source: e,
        })?;
        pb.inc(n as u64);
    }

    Ok(())
}
