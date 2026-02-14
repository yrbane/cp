//! Tests — CopyOptions resolution from CLI flags

mod common;
use common::*;

// ─── Archive implies recursive ───────────────────────────────────────────────

#[test]
fn opts_archive_enables_recursive() {
    let e = Env::new();
    e.file("src/sub/file.txt", "deep");

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst/sub/file.txt")), "deep");
}

// ─── Archive preserves mode ──────────────────────────────────────────────────

#[test]
fn opts_archive_preserves_mode() {
    let e = Env::new();
    e.file_mode("src", "content", 0o751);

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mode(&e.p("dst")), 0o751);
}

// ─── Archive preserves timestamps ────────────────────────────────────────────

#[test]
fn opts_archive_preserves_timestamps() {
    let e = Env::new();
    e.file("src", "content");
    e.set_mtime("src", 1_500_000_000);

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mtime(&e.p("dst")), 1_500_000_000);
}

// ─── Archive preserves symlinks (-P) ─────────────────────────────────────────

#[test]
fn opts_archive_preserves_symlinks() {
    let e = Env::new();
    e.file("src/real.txt", "data");
    e.symlink("real.txt", "src/link.txt");

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/link.txt")));
    assert_eq!(link_target(&e.p("dst/link.txt")).to_str().unwrap(), "real.txt");
}

// ─── --preserve then --no-preserve cancels ───────────────────────────────────

#[test]
fn opts_preserve_then_no_preserve() {
    let e = Env::new();
    e.file_mode("src", "content", 0o751);

    cp().arg("--preserve=mode")
        .arg("--no-preserve=mode")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_ne!(mode(&e.p("dst")), 0o751);
}

// ─── --preserve=all --no-preserve=all → nothing preserved ────────────────────

#[test]
fn opts_no_preserve_all() {
    let e = Env::new();
    e.file_mode("src", "content", 0o751);
    e.set_mtime("src", 1_000_000);

    cp().arg("--preserve=all")
        .arg("--no-preserve=all")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_ne!(mode(&e.p("dst")), 0o751);
    assert_ne!(mtime(&e.p("dst")), 1_000_000);
}

// ─── Default dereference: without -R, symlink CLI → copies content ───────────

#[test]
fn opts_dereference_default_follows() {
    let e = Env::new();
    e.file("target.txt", "real content");
    e.symlink(&e.p("target.txt"), "link.txt");

    cp().arg(e.p("link.txt")).arg(e.p("dst.txt")).assert().success();

    assert!(!is_symlink(&e.p("dst.txt")));
    assert_eq!(content(&e.p("dst.txt")), "real content");
}

// ─── -R default implies -P: symlinks preserved ──────────────────────────────

#[test]
fn opts_dereference_R_default_P() {
    let e = Env::new();
    e.file("src/target", "data");
    e.symlink("target", "src/link");

    cp().arg("-R").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/link")));
}

// ─── -R -L follows all symlinks ──────────────────────────────────────────────

#[test]
fn opts_dereference_L_follows_all() {
    let e = Env::new();
    e.file("src/target", "data");
    e.symlink("target", "src/link");

    cp().arg("-R").arg("-L").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(!is_symlink(&e.p("dst/link")));
    assert_eq!(content(&e.p("dst/link")), "data");
}

// ─── VERSION_CONTROL env + -b → numbered backup ─────────────────────────────

#[test]
fn opts_version_control_env() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");

    cp().env("VERSION_CONTROL", "numbered")
        .arg("-b")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
    assert_eq!(content(&e.p("dst.~1~")), "old");
}

// ─── SIMPLE_BACKUP_SUFFIX env ────────────────────────────────────────────────

#[test]
fn opts_simple_backup_suffix_env() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");

    cp().env("SIMPLE_BACKUP_SUFFIX", ".orig")
        .arg("--backup=simple")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
    assert_eq!(content(&e.p("dst.orig")), "old");
}

// ─── -n overrides -i: no-clobber wins ────────────────────────────────────────

#[test]
fn opts_no_clobber_overrides_interactive() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "keep_me");

    cp().arg("-n").arg("-i").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}
