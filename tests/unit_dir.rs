//! Tests — recursive directory copy (dir.rs)

mod common;
use common::*;

#[test]
fn dir_basic_recursive() {
    let e = Env::new();
    e.file("src/f1.txt", "one");
    e.file("src/a/f2.txt", "two");
    e.file("src/a/b/f3.txt", "three");

    cp().arg("-R").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst/f1.txt")), "one");
    assert_eq!(content(&e.p("dst/a/f2.txt")), "two");
    assert_eq!(content(&e.p("dst/a/b/f3.txt")), "three");
}

#[test]
fn dir_recursive_into_existing() {
    let e = Env::new();
    e.file("src/file.txt", "content");
    e.dir("dst");

    cp().arg("-R").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst/src/file.txt")), "content");
}

#[test]
fn dir_archive_preserves_symlinks() {
    let e = Env::new();
    e.file("src/real.txt", "content");
    e.symlink("real.txt", "src/link.txt");

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/link.txt")));
    assert_eq!(link_target(&e.p("dst/link.txt")).to_str().unwrap(), "real.txt");
}

#[test]
fn dir_parents_replicates_path() {
    let e = Env::new();
    let base = e.file("base/sub/file.txt", "content");
    e.dir("dest");

    cp().arg("--parents").arg(&base).arg(e.p("dest")).assert().success();

    let expected = e.p("dest").join(base.strip_prefix("/").unwrap());
    assert!(expected.exists(), "file should exist at: {}", expected.display());
}

#[test]
fn dir_no_target_directory() {
    let e = Env::new();
    e.file("src/sub/file", "content");

    cp().arg("-RT").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst/sub/file")), "content");
}

#[test]
fn dir_preserve_hard_links() {
    let e = Env::new();
    e.file("src/a", "shared");
    e.hardlink("src/a", "src/b");
    assert_eq!(ino(&e.p("src/a")), ino(&e.p("src/b")));

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(ino(&e.p("dst/a")), ino(&e.p("dst/b")));
}

#[test]
fn dir_copy_into_self() {
    let e = Env::new();
    e.file("dir/file", "x");
    e.dir("dir/sub");

    cp().arg("-R")
        .arg(e.p("dir"))
        .arg(e.p("dir/sub"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("into itself"));
}

#[test]
fn dir_empty_directory() {
    let e = Env::new();
    e.dir("empty");

    cp().arg("-R").arg(e.p("empty")).arg(e.p("dst")).assert().success();

    assert!(e.p("dst").is_dir());
}

#[test]
fn dir_target_directory_flag() {
    let e = Env::new();
    e.file("a.txt", "a");
    e.file("b.txt", "b");
    e.dir("dest");

    cp().arg("-t").arg(e.p("dest")).arg(e.p("a.txt")).arg(e.p("b.txt")).assert().success();

    assert_eq!(content(&e.p("dest/a.txt")), "a");
    assert_eq!(content(&e.p("dest/b.txt")), "b");
}

#[test]
fn dir_deep_nesting() {
    let e = Env::new();
    e.file("deep/a/b/c/d/e/f/g/h/leaf.txt", "deep");

    cp().arg("-R").arg(e.p("deep")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst/a/b/c/d/e/f/g/h/leaf.txt")), "deep");
}
