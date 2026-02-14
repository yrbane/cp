//! Benchmarks comparing our cp against GNU cp and different copy methods.
//!
//! Run with: cargo bench

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use tempfile::TempDir;

fn create_file(path: &Path, size: usize) {
    let mut f = fs::File::create(path).unwrap();
    let chunk = vec![0xABu8; 256 * 1024];
    let mut written = 0;
    while written < size {
        let to_write = std::cmp::min(chunk.len(), size - written);
        f.write_all(&chunk[..to_write]).unwrap();
        written += to_write;
    }
}

fn create_many_files(dir: &Path, count: usize, size: usize) {
    fs::create_dir_all(dir).unwrap();
    for i in 0..count {
        let path = dir.join(format!("file_{:06}", i));
        fs::write(&path, &vec![0xCDu8; size]).unwrap();
    }
}

fn create_sparse_file(path: &Path, total_size: u64, data_offset: u64) {
    let f = fs::File::create(path).unwrap();
    f.set_len(total_size).unwrap();
    use std::io::{Seek, SeekFrom};
    let mut f = f;
    f.seek(SeekFrom::Start(data_offset)).unwrap();
    f.write_all(&[0xEE; 4096]).unwrap();
}

fn our_cp() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_BIN_EXE_cp"));
    assert!(p.exists(), "our cp binary not found at {:?}", p);
    p
}

fn bench_single(label: &str, f: impl Fn()) -> Duration {
    // Warmup
    f();

    const RUNS: u32 = 5;
    let mut total = Duration::ZERO;

    for _ in 0..RUNS {
        let start = Instant::now();
        f();
        total += start.elapsed();
    }

    let avg = total / RUNS;
    eprintln!("  {}: {:?} avg ({} runs)", label, avg, RUNS);
    avg
}

// ─── Benchmark: Large file copy ─────────────────────────────────────────────

#[test]
fn bench_large_file_100mb() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("large_src");
    create_file(&src, 100 * 1024 * 1024);

    eprintln!("\n=== Large file copy (100 MB) ===");

    let gnu_dst = tmp.path().join("gnu_dst");
    let gnu_time = bench_single("GNU cp", || {
        let _ = fs::remove_file(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_dst");
    let our_time = bench_single("our cp (copy_file_range)", || {
        let _ = fs::remove_file(&our_dst);
        Command::new(our_cp())
            .arg("--sparse=never")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    let our_sparse_dst = tmp.path().join("our_sparse_dst");
    let our_sparse_time = bench_single("our cp (sparse=auto)", || {
        let _ = fs::remove_file(&our_sparse_dst);
        Command::new(our_cp())
            .arg(&src)
            .arg(&our_sparse_dst)
            .output()
            .unwrap();
    });

    // Verify correctness
    assert_eq!(fs::read(&gnu_dst).unwrap(), fs::read(&our_dst).unwrap());

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: Many small files ────────────────────────────────────────────

#[test]
fn bench_many_small_files() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("many_src");
    create_many_files(&src, 1000, 1024); // 1000 x 1KB files

    eprintln!("\n=== Many small files (1000 x 1 KB) ===");

    let gnu_dst = tmp.path().join("gnu_many");
    let gnu_time = bench_single("GNU cp -R", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-R")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_many");
    let our_time = bench_single("our cp -R", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: Sparse file ─────────────────────────────────────────────────

#[test]
fn bench_sparse_file() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("sparse_src");
    create_sparse_file(&src, 100 * 1024 * 1024, 50 * 1024 * 1024); // 100MB with hole

    eprintln!("\n=== Sparse file (100 MB with 50 MB hole) ===");

    let gnu_dst = tmp.path().join("gnu_sparse");
    let gnu_time = bench_single("GNU cp", || {
        let _ = fs::remove_file(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("--sparse=auto")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_sparse");
    let our_time = bench_single("our cp (sparse=auto)", || {
        let _ = fs::remove_file(&our_dst);
        Command::new(our_cp())
            .arg("--sparse=auto")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: Preserve metadata ───────────────────────────────────────────

#[test]
fn bench_preserve_metadata() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("meta_src");
    create_many_files(&src, 500, 4096); // 500 x 4KB

    eprintln!("\n=== Recursive copy with -a (500 x 4 KB) ===");

    let gnu_dst = tmp.path().join("gnu_meta");
    let gnu_time = bench_single("GNU cp -a", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-a")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_meta");
    let our_time = bench_single("our cp -a", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-a")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: Empty file ──────────────────────────────────────────────────

#[test]
fn bench_empty_file() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("empty");
    fs::write(&src, "").unwrap();

    eprintln!("\n=== Empty file copy ===");

    let our_dst = tmp.path().join("dst");
    let _ = bench_single("our cp (empty)", || {
        let _ = fs::remove_file(&our_dst);
        Command::new(our_cp())
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });
}

// ─── Helpers for new benchmarks ──────────────────────────────────────────────

/// Create a deep directory tree: `depth` levels × `width` dirs × `files` per dir.
fn create_deep_tree(root: &Path, depth: usize, width: usize, files: usize, size: usize) {
    create_deep_tree_recurse(root, depth, width, files, size);
}

fn create_deep_tree_recurse(dir: &Path, depth: usize, width: usize, files: usize, size: usize) {
    fs::create_dir_all(dir).unwrap();
    let data = vec![0xABu8; size];
    for i in 0..files {
        fs::write(dir.join(format!("f_{:04}", i)), &data).unwrap();
    }
    if depth > 0 {
        for w in 0..width {
            let sub = dir.join(format!("d_{}", w));
            create_deep_tree_recurse(&sub, depth - 1, width, files, size);
        }
    }
}

/// Create files with a distribution of sizes: &[(count, size)].
fn create_mixed_files(dir: &Path, dist: &[(usize, usize)]) {
    fs::create_dir_all(dir).unwrap();
    let mut idx = 0;
    for &(count, size) in dist {
        let data = vec![0xCDu8; size];
        for _ in 0..count {
            fs::write(dir.join(format!("mixed_{:06}", idx)), &data).unwrap();
            idx += 1;
        }
    }
}

// ─── Benchmark: Deep tree (5 levels × 4 dirs × 10 files 4KB) ────────────────

#[test]
fn bench_deep_tree() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("deep_src");
    // 5 levels × 4 dirs × 10 files 4KB ≈ tree with many directories
    create_deep_tree(&src, 5, 4, 10, 4096);

    eprintln!("\n=== Deep tree (5 levels × 4 dirs × 10 files 4KB) ===");

    let gnu_dst = tmp.path().join("gnu_deep");
    let gnu_time = bench_single("GNU cp -R", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-R")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_deep");
    let our_time = bench_single("our cp -R", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!(
        "  Speedup vs GNU: {:.1}x",
        gnu_time.as_secs_f64() / our_time.as_secs_f64()
    );
}

// ─── Benchmark: Mixed file sizes ─────────────────────────────────────────────

#[test]
fn bench_mixed_sizes() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("mixed_src");
    // 50×1KB + 30×100KB + 15×1MB + 5×10MB
    create_mixed_files(
        &src,
        &[
            (50, 1024),
            (30, 100 * 1024),
            (15, 1024 * 1024),
            (5, 10 * 1024 * 1024),
        ],
    );

    eprintln!("\n=== Mixed sizes (50×1KB + 30×100KB + 15×1MB + 5×10MB) ===");

    let gnu_dst = tmp.path().join("gnu_mixed");
    let gnu_time = bench_single("GNU cp -R", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-R")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_mixed");
    let our_time = bench_single("our cp -R", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!(
        "  Speedup vs GNU: {:.1}x",
        gnu_time.as_secs_f64() / our_time.as_secs_f64()
    );
}

// ─── Benchmark: Symlink-heavy directory ──────────────────────────────────────

#[test]
fn bench_symlink_heavy() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("sym_src");
    fs::create_dir(&src).unwrap();

    // 100 real files
    for i in 0..100 {
        fs::write(src.join(format!("f_{:04}", i)), &vec![0xAAu8; 1024]).unwrap();
    }
    // 400 symlinks pointing to those files
    for i in 0..400 {
        std::os::unix::fs::symlink(
            format!("f_{:04}", i % 100),
            src.join(format!("sym_{:04}", i)),
        )
        .unwrap();
    }

    eprintln!("\n=== Symlink-heavy (100 files + 400 symlinks) ===");

    let gnu_dst = tmp.path().join("gnu_sym");
    let gnu_time = bench_single("GNU cp -R", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-R")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_sym");
    let our_time = bench_single("our cp -R", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!(
        "  Speedup vs GNU: {:.1}x",
        gnu_time.as_secs_f64() / our_time.as_secs_f64()
    );
}

// ─── Benchmark: Hardlink-heavy directory ─────────────────────────────────────

#[test]
fn bench_hardlink_heavy() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("hl_src");
    fs::create_dir(&src).unwrap();

    // 50 real files × 20 hardlinks each = 1000 entries
    for i in 0..50 {
        let orig = src.join(format!("f_{:04}", i));
        fs::write(&orig, &vec![0xBBu8; 4096]).unwrap();
        for j in 1..20 {
            fs::hard_link(&orig, src.join(format!("f_{:04}_hl_{:02}", i, j))).unwrap();
        }
    }

    eprintln!("\n=== Hardlink-heavy (50 files × 20 links = 1000 entries) ===");

    let gnu_dst = tmp.path().join("gnu_hl");
    let gnu_time = bench_single("GNU cp -a", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-a")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_hl");
    let our_time = bench_single("our cp -a", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-a")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!(
        "  Speedup vs GNU: {:.1}x",
        gnu_time.as_secs_f64() / our_time.as_secs_f64()
    );
}

// ─── Benchmark: Parallel threshold sweep ─────────────────────────────────────

#[test]
fn bench_parallel_threshold() {
    eprintln!("\n=== Parallel threshold sweep (32/64/128/256 files per dir) ===");

    for &count in &[32, 64, 128, 256] {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        create_many_files(&src, count, 4096);

        let our_dst = tmp.path().join("our_dst");
        bench_single(&format!("our cp -R ({} files)", count), || {
            let _ = fs::remove_dir_all(&our_dst);
            Command::new(our_cp())
                .arg("-R")
                .arg(&src)
                .arg(&our_dst)
                .output()
                .unwrap();
        });
    }
}

// ─── Benchmark: Single file startup overhead ─────────────────────────────────

#[test]
fn bench_single_file_startup() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("tiny");
    fs::write(&src, "x").unwrap();

    eprintln!("\n=== Single file startup overhead (1 byte × 50 runs) ===");

    let dst = tmp.path().join("tiny_dst");

    const RUNS: u32 = 50;
    let mut total = Duration::ZERO;

    for _ in 0..RUNS {
        let _ = fs::remove_file(&dst);
        let start = Instant::now();
        Command::new(our_cp())
            .arg(&src)
            .arg(&dst)
            .output()
            .unwrap();
        total += start.elapsed();
    }

    let avg = total / RUNS;
    eprintln!("  our cp (1-byte): {:?} avg ({} runs)", avg, RUNS);
    eprintln!("  startup overhead: ~{:.1}ms", avg.as_secs_f64() * 1000.0);
}

// ─── Benchmark: 1GB file ─────────────────────────────────────────────────────

#[test]
fn bench_1gb_file() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("1gb_src");
    create_file(&src, 1024 * 1024 * 1024);

    eprintln!("\n=== 1 GB file copy ===");

    let gnu_dst = tmp.path().join("gnu_1gb");
    let gnu_time = bench_single("GNU cp", || {
        let _ = fs::remove_file(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_1gb");
    let our_time = bench_single("our cp", || {
        let _ = fs::remove_file(&our_dst);
        Command::new(our_cp())
            .arg("--sparse=never")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    // Verify correctness (just size to avoid 1GB memcmp)
    assert_eq!(
        fs::metadata(&our_dst).unwrap().len(),
        1024 * 1024 * 1024,
    );

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: 10K tiny files ───────────────────────────────────────────────

#[test]
fn bench_ten_thousand_tiny() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("tiny_src");
    create_many_files(&src, 10_000, 100); // 10K × 100B

    eprintln!("\n=== 10,000 tiny files (100B each) ===");

    let gnu_dst = tmp.path().join("gnu_tiny");
    let gnu_time = bench_single("GNU cp -R", || {
        let _ = fs::remove_dir_all(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("-R")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_tiny");
    let our_time = bench_single("our cp -R", || {
        let _ = fs::remove_dir_all(&our_dst);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: Metadata overhead ────────────────────────────────────────────

#[test]
fn bench_metadata_overhead() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("meta_src");
    create_many_files(&src, 500, 4096); // 500 × 4KB

    eprintln!("\n=== Metadata overhead: no flags vs -p vs -a (500 × 4KB) ===");

    let dst_plain = tmp.path().join("dst_plain");
    bench_single("our cp -R (no flags)", || {
        let _ = fs::remove_dir_all(&dst_plain);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&dst_plain)
            .output()
            .unwrap();
    });

    let dst_p = tmp.path().join("dst_p");
    bench_single("our cp -Rp", || {
        let _ = fs::remove_dir_all(&dst_p);
        Command::new(our_cp())
            .arg("-Rp")
            .arg(&src)
            .arg(&dst_p)
            .output()
            .unwrap();
    });

    let dst_a = tmp.path().join("dst_a");
    bench_single("our cp -a", || {
        let _ = fs::remove_dir_all(&dst_a);
        Command::new(our_cp())
            .arg("-a")
            .arg(&src)
            .arg(&dst_a)
            .output()
            .unwrap();
    });
}

// ─── Benchmark: Sparse fragmented ────────────────────────────────────────────

#[test]
fn bench_sparse_fragmented() {
    use std::io::{Seek, SeekFrom};

    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("sparse_frag");
    // 100MB: alternating 1MB data / 1MB hole × 50
    {
        let mut f = fs::File::create(&src).unwrap();
        let data_chunk = vec![0xABu8; 1024 * 1024];
        for i in 0..50 {
            f.seek(SeekFrom::Start(i * 2 * 1024 * 1024)).unwrap();
            f.write_all(&data_chunk).unwrap();
        }
        f.set_len(100 * 1024 * 1024).unwrap();
    }

    eprintln!("\n=== Sparse fragmented (100MB, 50 × 1MB data/hole) ===");

    let gnu_dst = tmp.path().join("gnu_sfrag");
    let gnu_time = bench_single("GNU cp --sparse=auto", || {
        let _ = fs::remove_file(&gnu_dst);
        Command::new("/usr/bin/cp")
            .arg("--sparse=auto")
            .arg(&src)
            .arg(&gnu_dst)
            .output()
            .unwrap();
    });

    let our_dst = tmp.path().join("our_sfrag");
    let our_time = bench_single("our cp --sparse=auto", || {
        let _ = fs::remove_file(&our_dst);
        Command::new(our_cp())
            .arg("--sparse=auto")
            .arg(&src)
            .arg(&our_dst)
            .output()
            .unwrap();
    });

    eprintln!("  Speedup vs GNU: {:.1}x", gnu_time.as_secs_f64() / our_time.as_secs_f64());
}

// ─── Benchmark: Attributes only ──────────────────────────────────────────────

#[test]
fn bench_attributes_only() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("attr_src");
    create_many_files(&src, 1000, 4096); // 1000 × 4KB

    // Pre-create destination for --attributes-only
    let dst_attr = tmp.path().join("dst_attr");
    Command::new(our_cp())
        .arg("-R")
        .arg(&src)
        .arg(&dst_attr)
        .output()
        .unwrap();

    eprintln!("\n=== Attributes-only vs full copy (1000 × 4KB) ===");

    let dst_full = tmp.path().join("dst_full");
    bench_single("our cp -R (full copy)", || {
        let _ = fs::remove_dir_all(&dst_full);
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&dst_full)
            .output()
            .unwrap();
    });

    bench_single("our cp --attributes-only -p", || {
        Command::new(our_cp())
            .arg("-R")
            .arg("--attributes-only")
            .arg("-p")
            .arg(&src)
            .arg(&dst_attr)
            .output()
            .unwrap();
    });
}

// ─── Benchmark: Update mode overhead ─────────────────────────────────────────

#[test]
fn bench_update_mode() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("upd_src");
    create_many_files(&src, 1000, 4096); // 1000 × 4KB

    // Create dst with some files newer, some older
    let dst = tmp.path().join("upd_dst");
    Command::new(our_cp())
        .arg("-R")
        .arg(&src)
        .arg(&dst)
        .output()
        .unwrap();

    // Touch half the source files to make them newer
    for i in 0..500 {
        let p = src.join(format!("file_{:06}", i));
        filetime::set_file_mtime(
            &p,
            filetime::FileTime::from_unix_time(2_000_000_000, 0),
        )
        .unwrap();
    }

    eprintln!("\n=== Update mode: -u vs plain (1000 files, 50% newer) ===");

    bench_single("our cp -R (no -u)", || {
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&dst)
            .output()
            .unwrap();
    });

    bench_single("our cp -Ru", || {
        Command::new(our_cp())
            .arg("-Ru")
            .arg(&src)
            .arg(&dst)
            .output()
            .unwrap();
    });
}

// ─── Benchmark: Fine parallel threshold ──────────────────────────────────────

#[test]
fn bench_fine_threshold() {
    eprintln!("\n=== Fine parallel threshold sweep (48/56/63/65/80 files) ===");

    for &count in &[48, 56, 63, 65, 80] {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        create_many_files(&src, count, 4096);

        let our_dst = tmp.path().join("our_dst");
        bench_single(&format!("our cp -R ({} files)", count), || {
            let _ = fs::remove_dir_all(&our_dst);
            Command::new(our_cp())
                .arg("-R")
                .arg(&src)
                .arg(&our_dst)
                .output()
                .unwrap();
        });
    }
}

// ─── Benchmark: Backup overhead ──────────────────────────────────────────────

#[test]
fn bench_backup_overhead() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("bak_src");
    create_many_files(&src, 1000, 4096); // 1000 × 4KB

    eprintln!("\n=== Backup overhead: --backup=simple vs plain (1000 × 4KB) ===");

    // Pre-create destination
    let dst_plain = tmp.path().join("bak_dst_plain");
    Command::new(our_cp())
        .arg("-R")
        .arg(&src)
        .arg(&dst_plain)
        .output()
        .unwrap();

    let dst_bak = tmp.path().join("bak_dst_bak");
    Command::new(our_cp())
        .arg("-R")
        .arg(&src)
        .arg(&dst_bak)
        .output()
        .unwrap();

    bench_single("our cp -R (no backup)", || {
        Command::new(our_cp())
            .arg("-R")
            .arg(&src)
            .arg(&dst_plain)
            .output()
            .unwrap();
    });

    bench_single("our cp -R --backup=simple", || {
        // Clean up backups from previous run
        for entry in fs::read_dir(&dst_bak).unwrap() {
            let e = entry.unwrap();
            if e.file_name().to_string_lossy().ends_with('~') {
                let _ = fs::remove_file(e.path());
            }
        }
        Command::new(our_cp())
            .arg("-R")
            .arg("--backup=simple")
            .arg(&src)
            .arg(&dst_bak)
            .output()
            .unwrap();
    });
}

fn main() {
    // This file uses #[test] functions as benchmarks
    // Run with: cargo test --release --test copy_bench -- --nocapture
    eprintln!("Run benchmarks with: cargo test --release -p cp bench_ -- --nocapture");
}
