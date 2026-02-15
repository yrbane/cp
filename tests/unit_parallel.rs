//! Tests — parallel copy (PARALLEL_THRESHOLD=64 in dir.rs)

mod common;
use common::*;

/// Populate a flat directory with `n` files of predictable content.
fn populate(e: &Env, n: usize) {
    e.dir("src");
    for i in 0..n {
        e.file(&format!("src/f_{i:04}"), format!("data_{i}"));
    }
}

// ─── Below threshold (63 files) — sequential path ────────────────────────────

#[test]
fn parallel_below_threshold() {
    let e = Env::new();
    populate(&e, 63);

    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_count(&e.p("dst")), 63);
}

// ─── At threshold (64 files) — parallel path triggers ────────────────────────

#[test]
fn parallel_at_threshold() {
    let e = Env::new();
    populate(&e, 64);

    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_count(&e.p("dst")), 64);
}

// ─── Many files with content verification ────────────────────────────────────

#[test]
fn parallel_many_files() {
    let e = Env::new();
    populate(&e, 200);

    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_count(&e.p("dst")), 200);
    for i in [0, 50, 99, 150, 199] {
        assert_eq!(content(&e.p(&format!("dst/f_{i:04}"))), format!("data_{i}"));
    }
}

// ─── Parallel preserves hard links with -a ───────────────────────────────────

#[test]
fn parallel_preserves_hard_links() {
    let e = Env::new();
    populate(&e, 100);
    for i in 0..50 {
        e.hardlink(&format!("src/f_{i:04}"), &format!("src/link_{i:04}"));
    }

    cp().arg("-a")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    for i in 0..50 {
        assert_eq!(
            ino(&e.p(&format!("dst/f_{i:04}"))),
            ino(&e.p(&format!("dst/link_{i:04}"))),
            "hardlink pair {i} should share inode"
        );
    }
}

// ─── Parallel preserves metadata with -a ─────────────────────────────────────

#[test]
fn parallel_with_metadata() {
    let e = Env::new();
    e.dir("src");
    for i in 0..100 {
        e.file_mode(&format!("src/f_{i:04}"), format!("data_{i}"), 0o755);
    }

    cp().arg("-a")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    for i in [0, 25, 50, 75, 99] {
        assert_eq!(mode(&e.p(&format!("dst/f_{i:04}"))), 0o755);
    }
}

// ─── Parallel error on unreadable file ───────────────────────────────────────

#[test]
fn parallel_error_unreadable() {
    let e = Env::new();
    populate(&e, 100);
    e.chmod("src/f_0050", 0o000);

    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .failure();

    // Cleanup for TempDir removal
    e.chmod("src/f_0050", 0o644);
}

// ─── Parallel with mixed types: files + symlinks + subdirs ───────────────────

#[test]
fn parallel_mixed_types() {
    let e = Env::new();
    // 80 regular files (above threshold)
    e.dir("src");
    for i in 0..80 {
        e.file(&format!("src/f_{i:04}"), format!("data_{i}"));
    }
    // 10 symlinks
    for i in 0..10 {
        e.symlink(format!("f_{i:04}"), &format!("src/sym_{i:04}"));
    }
    // 5 subdirectories with files
    for i in 0..5 {
        e.file(&format!("src/dir_{i}/inner.txt"), format!("inner_{i}"));
    }

    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst/f_0000")), "data_0");
    assert!(is_symlink(&e.p("dst/sym_0000")));
    assert_eq!(content(&e.p("dst/dir_0/inner.txt")), "inner_0");
}

// ─── Parallel data integrity with random content ─────────────────────────────

#[test]
fn parallel_data_integrity() {
    use rand::Rng;

    let e = Env::new();
    e.dir("src");
    let mut rng = rand::rng();
    let mut expected = Vec::with_capacity(128);

    for i in 0..128 {
        let name = format!("src/f_{i:04}");
        let data: Vec<u8> = (0..4096).map(|_| rng.random::<u8>()).collect();
        e.file(&name, &data);
        expected.push((format!("dst/f_{i:04}"), data));
    }

    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    for (rel, data) in &expected {
        assert_eq!(&bytes(&e.p(rel)), data, "integrity mismatch: {rel}");
    }
}
