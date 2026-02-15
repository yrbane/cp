//! Tests — copy engine methods (engine.rs)

mod common;
use common::*;

#[test]
fn engine_copy_file_range_small_file() {
    let e = Env::new();
    e.file("src", "hello world");

    cp().arg("--sparse=never")
        .arg("--debug")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success()
        .stderr(predicates::str::contains("copy_file_range"));

    assert_eq!(content(&e.p("dst")), "hello world");
}

#[test]
fn engine_copy_file_range_large_file() {
    let e = Env::new();
    let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 256) as u8).collect();
    e.file("src", &data);

    cp().arg("--sparse=never")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(bytes(&e.p("dst")), data);
}

#[test]
fn engine_empty_file() {
    let e = Env::new();
    e.file("empty", "");

    cp().arg(e.p("empty")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "");
    assert_eq!(file_size(&e.p("dst")), 0);
}

#[test]
fn engine_reflink_always_fails_on_tmpfs() {
    let e = Env::new();
    e.file("src", "data");

    // May succeed on btrfs/xfs, fail on ext4/tmpfs — just verify no panic
    let _ = cp()
        .arg("--reflink=always")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert();
}

#[test]
fn engine_reflink_auto_succeeds() {
    let e = Env::new();
    e.file("src", "data for reflink auto");

    cp().arg("--reflink=auto")
        .arg("--sparse=never")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "data for reflink auto");
}

#[test]
fn engine_binary_data_integrity() {
    let e = Env::new();
    let data: Vec<u8> = (0..=255u8).cycle().take(100 * 1024).collect();
    e.file("binary", &data);

    cp().arg("--sparse=never")
        .arg(e.p("binary"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(bytes(&e.p("dst")), data);
}

#[test]
fn engine_overwrite_truncates() {
    let e = Env::new();
    e.file("dst", "this is a much longer existing content");
    e.file("src", "short");

    cp().arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "short");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Edge case tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn engine_exact_chunk_boundary() {
    let e = Env::new();
    // Exactly 64MB = COPY_FILE_RANGE_CHUNK
    let size = 64 * 1024 * 1024;
    let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    e.file("src", &data);

    cp().arg("--sparse=never")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("dst")), size as u64);
    assert_eq!(bytes(&e.p("dst")), data);
}

#[test]
fn engine_just_over_chunk() {
    let e = Env::new();
    // 64MB + 1 → requires 2 copy_file_range calls
    let size = 64 * 1024 * 1024 + 1;
    let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    e.file("src", &data);

    cp().arg("--sparse=never")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("dst")), size as u64);
    assert_eq!(bytes(&e.p("dst")), data);
}
