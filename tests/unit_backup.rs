//! Tests — backup.rs

mod common;
use common::*;

#[test]
fn backup_simple_creates_tilde_file() {
    let e = Env::new();
    e.file("src.txt", "new");
    e.file("file.txt", "original");

    // Same file → failure
    cp().arg("--backup=simple").arg(e.p("file.txt")).arg(e.p("file.txt")).assert().failure();

    // Different source → success
    cp().arg("--backup=simple").arg(e.p("src.txt")).arg(e.p("file.txt")).assert().success();

    assert_eq!(content(&e.p("file.txt")), "new");
    assert_eq!(content(&e.p("file.txt~")), "original");
}

#[test]
fn backup_numbered_creates_dotN() {
    let e = Env::new();
    e.file("src.txt", "v1");
    e.file("file.txt", "v0");

    cp().arg("--backup=numbered").arg(e.p("src.txt")).arg(e.p("file.txt")).assert().success();

    assert_eq!(content(&e.p("file.txt.~1~")), "v0");
    assert_eq!(content(&e.p("file.txt")), "v1");

    // Second backup
    e.file("src.txt", "v2");
    cp().arg("--backup=numbered").arg(e.p("src.txt")).arg(e.p("file.txt")).assert().success();

    assert_eq!(content(&e.p("file.txt.~2~")), "v1");
    assert_eq!(content(&e.p("file.txt")), "v2");
}

#[test]
fn backup_existing_uses_simple_when_no_numbered() {
    let e = Env::new();
    e.file("src.txt", "new");
    e.file("file.txt", "old");

    cp().arg("--backup=existing").arg(e.p("src.txt")).arg(e.p("file.txt")).assert().success();

    assert!(e.p("file.txt~").exists());
}

#[test]
fn backup_existing_uses_numbered_when_numbered_exist() {
    let e = Env::new();
    e.file("src.txt", "new");
    e.file("file.txt", "old");
    e.file("file.txt.~1~", "ancient");

    cp().arg("--backup=existing").arg(e.p("src.txt")).arg(e.p("file.txt")).assert().success();

    assert!(e.p("file.txt.~2~").exists());
}

#[test]
fn backup_custom_suffix() {
    let e = Env::new();
    e.file("src.txt", "new");
    e.file("file.txt", "old");

    cp().arg("--backup=simple")
        .arg("-S").arg(".bak")
        .arg(e.p("src.txt"))
        .arg(e.p("file.txt"))
        .assert()
        .success();

    assert!(e.p("file.txt.bak").exists());
}

#[test]
fn backup_short_b_flag() {
    let e = Env::new();
    e.file("src.txt", "new");
    e.file("file.txt", "old");

    cp().arg("-b").arg(e.p("src.txt")).arg(e.p("file.txt")).assert().success();

    assert!(e.p("file.txt~").exists());
}

#[test]
fn backup_no_backup_when_dest_missing() {
    let e = Env::new();
    e.file("src.txt", "content");

    cp().arg("--backup=simple").arg(e.p("src.txt")).arg(e.p("new_file.txt")).assert().success();

    assert!(!e.p("new_file.txt~").exists());
    assert_eq!(content(&e.p("new_file.txt")), "content");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Edge case tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn backup_existing_detects_numbered() {
    let e = Env::new();
    e.file("src.txt", "v3");
    e.file("file.txt", "v2");
    e.file("file.txt.~1~", "v1"); // numbered backup exists

    cp().arg("--backup=existing")
        .arg(e.p("src.txt"))
        .arg(e.p("file.txt"))
        .assert()
        .success();

    // Should create .~2~ (numbered) because .~1~ already exists
    assert_eq!(content(&e.p("file.txt")), "v3");
    assert_eq!(content(&e.p("file.txt.~2~")), "v2");
    assert_eq!(content(&e.p("file.txt.~1~")), "v1");
}

#[test]
fn backup_existing_falls_back_simple() {
    let e = Env::new();
    e.file("src.txt", "new");
    e.file("file.txt", "old");
    // No numbered backups exist

    cp().arg("--backup=existing")
        .arg(e.p("src.txt"))
        .arg(e.p("file.txt"))
        .assert()
        .success();

    // Should fall back to simple (tilde) backup
    assert_eq!(content(&e.p("file.txt")), "new");
    assert_eq!(content(&e.p("file.txt~")), "old");
    assert!(!e.p("file.txt.~1~").exists());
}

#[test]
fn backup_multiple_numbered_sequence() {
    let e = Env::new();
    e.file("file.txt", "v0");

    // Three successive copies → .~1~, .~2~, .~3~
    for i in 1..=3 {
        e.file("src.txt", format!("v{i}"));
        cp().arg("--backup=numbered")
            .arg(e.p("src.txt"))
            .arg(e.p("file.txt"))
            .assert()
            .success();
    }

    assert_eq!(content(&e.p("file.txt")), "v3");
    assert_eq!(content(&e.p("file.txt.~1~")), "v0");
    assert_eq!(content(&e.p("file.txt.~2~")), "v1");
    assert_eq!(content(&e.p("file.txt.~3~")), "v2");
}

#[test]
fn backup_control_aliases() {
    // "t" = numbered
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");
    cp().arg("--backup=t").arg(e.p("src")).arg(e.p("dst")).assert().success();
    assert!(e.p("dst.~1~").exists());

    // "nil" = existing (no numbered → simple)
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");
    cp().arg("--backup=nil").arg(e.p("src")).arg(e.p("dst")).assert().success();
    assert!(e.p("dst~").exists());

    // "never" = simple
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");
    cp().arg("--backup=never").arg(e.p("src")).arg(e.p("dst")).assert().success();
    assert!(e.p("dst~").exists());

    // "off" = none
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");
    cp().arg("--backup=off").arg(e.p("src")).arg(e.p("dst")).assert().success();
    assert!(!e.p("dst~").exists());
    assert!(!e.p("dst.~1~").exists());
}
