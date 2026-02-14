use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use indicatif::ProgressBar;

use crate::cli::SparseMode;
use crate::error::{CpError, CpResult};

/// SEEK_HOLE and SEEK_DATA constants (Linux).
const SEEK_DATA: i32 = 3;
const SEEK_HOLE: i32 = 4;

/// Buffer size for sparse read/write.
const BUF_SIZE: usize = 256 * 1024;

/// Copy a file preserving sparse holes using SEEK_HOLE/SEEK_DATA.
/// Returns true if sparse copy was performed, false if fallback needed.
pub fn copy_sparse(
    src: &mut File,
    dst: &mut File,
    size: u64,
    src_path: &Path,
    dst_path: &Path,
    mode: SparseMode,
    pb: &ProgressBar,
) -> CpResult<bool> {
    match mode {
        SparseMode::Never => return Ok(false),
        SparseMode::Always => {
            // Always: detect zero blocks and create holes (maximizes sparseness)
            copy_sparse_by_zero_detection(src, dst, src_path, dst_path, size, pb)?;
            Ok(true)
        }
        SparseMode::Auto => {
            // Auto: use SEEK_HOLE/SEEK_DATA to preserve existing holes
            let scan = scan_sparse_regions(src, size);
            match scan {
                Some(regions) if !regions.is_empty() => {
                    // Check if there are actual holes (not just one region spanning the whole file)
                    let data_bytes: u64 = regions.iter().map(|r| r.length).sum();
                    if data_bytes >= size {
                        // No holes found, fall back to normal copy
                        return Ok(false);
                    }

                    // Set the file size to create trailing holes
                    dst.set_len(size).map_err(|e| CpError::Write {
                        path: dst_path.to_path_buf(),
                        source: e,
                    })?;

                    let mut buf = vec![0u8; BUF_SIZE];

                    for region in &regions {
                        src.seek(SeekFrom::Start(region.offset))
                            .map_err(|e| CpError::Seek {
                                path: src_path.to_path_buf(),
                                source: e,
                            })?;
                        dst.seek(SeekFrom::Start(region.offset))
                            .map_err(|e| CpError::Seek {
                                path: dst_path.to_path_buf(),
                                source: e,
                            })?;

                        let mut remaining = region.length;
                        while remaining > 0 {
                            let to_read = std::cmp::min(remaining as usize, BUF_SIZE);
                            let n = src.read(&mut buf[..to_read]).map_err(|e| CpError::Read {
                                path: src_path.to_path_buf(),
                                source: e,
                            })?;
                            if n == 0 {
                                break;
                            }
                            dst.write_all(&buf[..n]).map_err(|e| CpError::Write {
                                path: dst_path.to_path_buf(),
                                source: e,
                            })?;
                            remaining -= n as u64;
                            pb.inc(n as u64);
                        }
                    }

                    // Account for holes in progress
                    if size > data_bytes {
                        pb.inc(size - data_bytes);
                    }

                    Ok(true)
                }
                _ => Ok(false),
            }
        }
    }
}

/// A data region in a file (non-hole).
struct DataRegion {
    offset: u64,
    length: u64,
}

/// Scan a file for data regions using SEEK_HOLE/SEEK_DATA.
fn scan_sparse_regions(file: &File, size: u64) -> Option<Vec<DataRegion>> {
    let fd = file.as_raw_fd();
    let mut regions = Vec::new();
    let mut pos: i64 = 0;

    loop {
        // Find start of data
        let data_start = unsafe { nix::libc::lseek(fd, pos, SEEK_DATA) };
        if data_start < 0 {
            // ENXIO means no more data -- rest is a hole
            break;
        }

        // Find end of data (start of next hole)
        let hole_start = unsafe { nix::libc::lseek(fd, data_start, SEEK_HOLE) };
        let end = if hole_start < 0 {
            size as i64
        } else {
            hole_start
        };

        if end > data_start {
            regions.push(DataRegion {
                offset: data_start as u64,
                length: (end - data_start) as u64,
            });
        }

        pos = end;
        if pos as u64 >= size {
            break;
        }
    }

    // Reset file position
    unsafe { nix::libc::lseek(fd, 0, nix::libc::SEEK_SET) };

    Some(regions)
}

/// For --sparse=always: detect zero blocks and punch holes.
fn copy_sparse_by_zero_detection(
    src: &mut File,
    dst: &mut File,
    src_path: &Path,
    dst_path: &Path,
    size: u64,
    pb: &ProgressBar,
) -> CpResult<()> {
    dst.set_len(size).map_err(|e| CpError::Write {
        path: dst_path.to_path_buf(),
        source: e,
    })?;

    let mut buf = vec![0u8; BUF_SIZE];
    let mut offset: u64 = 0;

    loop {
        let n = src.read(&mut buf).map_err(|e| CpError::Read {
            path: src_path.to_path_buf(),
            source: e,
        })?;
        if n == 0 {
            break;
        }

        let is_zero = buf[..n].iter().all(|&b| b == 0);
        if !is_zero {
            dst.seek(SeekFrom::Start(offset))
                .map_err(|e| CpError::Seek {
                    path: dst_path.to_path_buf(),
                    source: e,
                })?;
            dst.write_all(&buf[..n]).map_err(|e| CpError::Write {
                path: dst_path.to_path_buf(),
                source: e,
            })?;
        }
        // If all zeros, don't write -- leave as hole

        offset += n as u64;
        pb.inc(n as u64);
    }

    Ok(())
}
