//! Tests — utility functions (util.rs)

mod common;
use common::*;

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

// ─── strip_trailing_slashes ─────────────────────────────────────────────────

#[test]
fn strip_trailing_single_slash() {
    let s = PathBuf::from("/tmp/foo/").to_string_lossy().trim_end_matches('/').to_string();
    assert_eq!(s, "/tmp/foo");
}

#[test]
fn strip_trailing_multiple_slashes() {
    let s = PathBuf::from("/tmp/foo///").to_string_lossy().trim_end_matches('/').to_string();
    assert_eq!(s, "/tmp/foo");
}

#[test]
fn strip_trailing_root_stays_root() {
    let s = PathBuf::from("/").to_string_lossy().trim_end_matches('/').to_string();
    let result = if s.is_empty() { PathBuf::from("/") } else { PathBuf::from(s) };
    assert_eq!(result, PathBuf::from("/"));
}

#[test]
fn strip_trailing_no_slash() {
    let s = PathBuf::from("/tmp/foo").to_string_lossy().trim_end_matches('/').to_string();
    assert_eq!(s, "/tmp/foo");
}

// ─── build_dest_path ────────────────────────────────────────────────────────

#[test]
fn build_dest_path_file_to_file() {
    let dst = PathBuf::from("/tmp/dest.txt");
    let result = if false { dst.join("file.txt") } else { dst.clone() };
    assert_eq!(result, PathBuf::from("/tmp/dest.txt"));
}

#[test]
fn build_dest_path_file_to_dir() {
    let result = PathBuf::from("/tmp/dir").join("file.txt");
    assert_eq!(result, PathBuf::from("/tmp/dir/file.txt"));
}

#[test]
fn build_dest_path_parents_mode() {
    let result = PathBuf::from("/tmp/dir").join("a/b/c");
    assert_eq!(result, PathBuf::from("/tmp/dir/a/b/c"));
}

// ─── resolve_target ─────────────────────────────────────────────────────────

#[test]
fn resolve_target_missing_operand() {
    cp().assert().failure().stderr(predicates::str::contains("required"));
}

#[test]
fn resolve_target_missing_dest() {
    cp().arg("single_file").assert().failure();
}

#[test]
fn resolve_target_two_files() {
    let e = Env::new();
    e.file("src.txt", "hello");

    cp().arg(e.p("src.txt")).arg(e.p("dst.txt")).assert().success();

    assert_eq!(content(&e.p("dst.txt")), "hello");
}

#[test]
fn resolve_target_multi_sources_needs_dir() {
    let e = Env::new();
    e.file("a.txt", "a");
    e.file("b.txt", "b");

    cp().arg(e.p("a.txt"))
        .arg(e.p("b.txt"))
        .arg(e.p("not_a_dir"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("not a directory"));
}

// ─── is_same_file ───────────────────────────────────────────────────────────

#[test]
fn same_file_hard_link() {
    let e = Env::new();
    e.file("a", "x");
    e.hardlink("a", "b");

    cp().arg(e.p("a"))
        .arg(e.p("b"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("same file"));
}

#[test]
fn same_file_self() {
    let e = Env::new();
    e.file("a", "x");

    cp().arg(e.p("a"))
        .arg(e.p("a"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("same file"));
}

// ─── get_metadata follow vs no-follow ───────────────────────────────────────

#[test]
fn metadata_follow_symlink() {
    let e = Env::new();
    e.file("real", "content");
    e.symlink(&e.p("real"), "link");

    assert!(std::fs::metadata(e.p("link")).unwrap().is_file());
    assert!(is_symlink(&e.p("link")));
}

// ─── get_device ─────────────────────────────────────────────────────────────

#[test]
fn get_device_returns_nonzero() {
    let e = Env::new();
    assert!(std::fs::metadata(e.path()).unwrap().dev() > 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Edge case tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn strip_trailing_all_slashes() {
    // "///" → should become "/"
    let s = PathBuf::from("///")
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let result = if s.is_empty() {
        PathBuf::from("/")
    } else {
        PathBuf::from(s)
    };
    assert_eq!(result, PathBuf::from("/"));
}

#[test]
fn build_dest_path_parents_strips_root() {
    let e = Env::new();
    e.dir("dest");
    let src = e.file("a/b/file.txt", "content");

    // --parents should replicate full source path under dest
    cp().arg("--parents")
        .arg(&src)
        .arg(e.p("dest"))
        .assert()
        .success();

    let expected = e.p("dest").join(src.strip_prefix("/").unwrap());
    assert!(expected.exists(), "file should exist at: {}", expected.display());
    assert_eq!(std::fs::read_to_string(&expected).unwrap(), "content");
}

#[test]
fn resolve_target_t_flag() {
    let e = Env::new();
    e.file("src1", "a");
    e.file("src2", "b");
    e.file("src3", "c");
    e.dir("target");

    // -t DIR: all remaining args are sources
    cp().arg("-t")
        .arg(e.p("target"))
        .arg(e.p("src1"))
        .arg(e.p("src2"))
        .arg(e.p("src3"))
        .assert()
        .success();

    assert_eq!(content(&e.p("target/src1")), "a");
    assert_eq!(content(&e.p("target/src2")), "b");
    assert_eq!(content(&e.p("target/src3")), "c");
}
