//! Tests — single file copy orchestration (copy.rs)

mod common;
use common::*;

#[test]
fn copy_basic_file() {
    let e = Env::new();
    e.file("src", "hello");

    cp().arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "hello");
}

#[test]
fn copy_no_clobber() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "keep_me");

    cp().arg("-n").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}

#[test]
fn copy_update_older_skips_newer_dest() {
    let e = Env::new();
    e.file("src", "old");
    e.set_mtime("src", 1_000_000);
    e.file("dst", "new"); // dst has current mtime (newer)

    cp().arg("-u").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_update_older_overwrites_older_dest() {
    let e = Env::new();
    e.file("dst", "old");
    e.set_mtime("dst", 1_000_000);
    e.file("src", "new"); // src has current mtime (newer)

    cp().arg("-u").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_force_removes_readonly() {
    let e = Env::new();
    e.file("src", "new");
    e.file_mode("dst", "readonly", 0o444);

    cp().arg("-f").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_hard_link() {
    let e = Env::new();
    e.file("src", "content");

    cp().arg("-l").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(ino(&e.p("src")), ino(&e.p("dst")));
}

#[test]
fn copy_symbolic_link() {
    let e = Env::new();
    e.file("src", "content");

    cp().arg("-s").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst")));
    assert_eq!(content(&e.p("dst")), "content");
}

#[test]
fn copy_preserve_mode() {
    let e = Env::new();
    e.file_mode("src", "content", 0o751);

    cp().arg("--preserve=mode").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mode(&e.p("dst")), 0o751);
}

#[test]
fn copy_preserve_timestamps() {
    let e = Env::new();
    e.file("src", "content");
    e.set_mtime("src", 1_500_000_000);

    cp().arg("--preserve=timestamps").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mtime(&e.p("dst")), 1_500_000_000);
}

#[test]
fn copy_symlink_no_deref() {
    let e = Env::new();
    e.file("target", "real");
    e.symlink(&e.p("target"), "link");

    cp().arg("-P").arg(e.p("link")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst")));
    assert_eq!(link_target(&e.p("dst")), e.p("target"));
}

#[test]
fn copy_symlink_deref() {
    let e = Env::new();
    e.file("target", "real content");
    e.symlink(&e.p("target"), "link");

    cp().arg("-L").arg(e.p("link")).arg(e.p("dst")).assert().success();

    assert!(!is_symlink(&e.p("dst")));
    assert_eq!(content(&e.p("dst")), "real content");
}

#[test]
fn copy_dir_without_recursive() {
    let e = Env::new();
    e.dir("mydir");

    cp().arg(e.p("mydir"))
        .arg(e.p("dst"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("omitting directory"));
}

#[test]
fn copy_verbose() {
    let e = Env::new();
    e.file("src", "x");

    cp().arg("-v")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success()
        .stderr(predicates::str::contains("->"));
}

#[test]
fn copy_remove_destination() {
    let e = Env::new();
    e.file("src", "new");
    e.file_mode("dst", "old", 0o444);

    cp().arg("--remove-destination")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_attributes_only() {
    let e = Env::new();
    e.file_mode("src", "has content", 0o755);

    cp().arg("--attributes-only")
        .arg("--preserve=mode")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("dst")), 0);
    assert_eq!(mode(&e.p("dst")), 0o755);
}
