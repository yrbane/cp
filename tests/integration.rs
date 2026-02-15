//! Integration tests — compare our cp against GNU cp

mod common;
use common::*;

use std::process::Command as StdCommand;

fn has_gnu_cp() -> bool {
    StdCommand::new("/usr/bin/cp")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! skip_no_gnu {
    () => {
        if !has_gnu_cp() {
            eprintln!("SKIP: GNU cp not found");
            return;
        }
    };
}

// ─── Compare single file copy ───────────────────────────────────────────────

#[test]
fn integ_single_file_matches_gnu() {
    skip_no_gnu!();
    let e = Env::new();
    let data: Vec<u8> = (0..=255u8).cycle().take(100_000).collect();
    e.file("src", &data);

    StdCommand::new("/usr/bin/cp")
        .arg(e.p("src"))
        .arg(e.p("gnu"))
        .output()
        .unwrap();
    cp().arg(e.p("src")).arg(e.p("our")).assert().success();

    assert_eq!(bytes(&e.p("gnu")), bytes(&e.p("our")));
}

// ─── Compare recursive copy ────────────────────────────────────────────────

#[test]
fn integ_recursive_matches_gnu() {
    skip_no_gnu!();
    let e = Env::new();
    e.file("src/f1", "one");
    e.file("src/a/f2", "two");
    e.file("src/a/b/f3", "three");
    e.symlink("../f1", "src/a/link");

    StdCommand::new("/usr/bin/cp")
        .arg("-R")
        .arg(e.p("src"))
        .arg(e.p("gnu"))
        .output()
        .unwrap();
    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("our"))
        .assert()
        .success();

    assert_eq!(content(&e.p("gnu/f1")), content(&e.p("our/f1")));
    assert_eq!(content(&e.p("gnu/a/f2")), content(&e.p("our/a/f2")));
    assert_eq!(content(&e.p("gnu/a/b/f3")), content(&e.p("our/a/b/f3")));
    assert_eq!(
        link_target(&e.p("gnu/a/link")),
        link_target(&e.p("our/a/link"))
    );
}

// ─── Compare -p (preserve) ──────────────────────────────────────────────────

#[test]
fn integ_preserve_matches_gnu() {
    skip_no_gnu!();
    let e = Env::new();
    e.file_mode("src", "content", 0o751);
    e.set_mtime("src", 1_500_000_000);

    StdCommand::new("/usr/bin/cp")
        .arg("-p")
        .arg(e.p("src"))
        .arg(e.p("gnu"))
        .output()
        .unwrap();
    cp().arg("-p")
        .arg(e.p("src"))
        .arg(e.p("our"))
        .assert()
        .success();

    assert_eq!(mode(&e.p("gnu")), mode(&e.p("our")));
    assert_eq!(mtime(&e.p("gnu")), mtime(&e.p("our")));
}

// ─── Error messages ─────────────────────────────────────────────────────────

#[test]
fn integ_error_same_file() {
    let e = Env::new();
    e.file("f", "x");

    cp().arg(e.p("f"))
        .arg(e.p("f"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("same file"))
        .code(1);
}

#[test]
fn integ_error_omit_directory() {
    let e = Env::new();
    e.dir("dir");

    cp().arg(e.p("dir"))
        .arg(e.p("dst"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("omitting directory"))
        .code(1);
}

#[test]
fn integ_error_not_a_directory() {
    let e = Env::new();
    e.file("a", "a");
    e.file("b", "b");
    e.file("c", "c");

    cp().arg(e.p("a"))
        .arg(e.p("b"))
        .arg(e.p("c"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("not a directory"));
}

// ─── Verbose output format ──────────────────────────────────────────────────

#[test]
fn integ_verbose_format() {
    let e = Env::new();
    e.file("src", "x");

    let out = cp()
        .arg("-v")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .output()
        .unwrap();

    assert!(String::from_utf8_lossy(&out.stdout).contains("->"));
}

// ─── Exit codes ─────────────────────────────────────────────────────────────

#[test]
fn integ_exit_code_success() {
    let e = Env::new();
    e.file("src", "x");

    cp().arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success()
        .code(0);
}

#[test]
fn integ_exit_code_failure() {
    cp().arg("/nonexistent")
        .arg("/tmp/whatever")
        .assert()
        .failure()
        .code(1);
}

// ─── Multiple sources ───────────────────────────────────────────────────────

#[test]
fn integ_multiple_sources_to_dir() {
    let e = Env::new();
    e.dir("dest");
    for i in 0..5 {
        e.file(&format!("file_{i}"), format!("content {i}"));
    }

    let mut cmd = cp();
    for i in 0..5 {
        cmd.arg(e.p(&format!("file_{i}")));
    }
    cmd.arg(e.p("dest")).assert().success();

    for i in 0..5 {
        assert_eq!(
            content(&e.p(&format!("dest/file_{i}"))),
            format!("content {i}")
        );
    }
}

// ─── --version and --help ───────────────────────────────────────────────────

#[test]
fn integ_help_flag() {
    cp().arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("copy files and directories"));
}

#[test]
fn integ_version_flag() {
    cp().arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("cp"));
}
