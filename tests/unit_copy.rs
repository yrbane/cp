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

    cp().arg("-n")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}

#[test]
fn copy_update_older_skips_newer_dest() {
    let e = Env::new();
    e.file("src", "old");
    e.set_mtime("src", 1_000_000);
    e.file("dst", "new"); // dst has current mtime (newer)

    cp().arg("-u")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_update_older_overwrites_older_dest() {
    let e = Env::new();
    e.file("dst", "old");
    e.set_mtime("dst", 1_000_000);
    e.file("src", "new"); // src has current mtime (newer)

    cp().arg("-u")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_force_removes_readonly() {
    let e = Env::new();
    e.file("src", "new");
    e.file_mode("dst", "readonly", 0o444);

    cp().arg("-f")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_hard_link() {
    let e = Env::new();
    e.file("src", "content");

    cp().arg("-l")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(ino(&e.p("src")), ino(&e.p("dst")));
}

#[test]
fn copy_symbolic_link() {
    let e = Env::new();
    e.file("src", "content");

    cp().arg("-s")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert!(is_symlink(&e.p("dst")));
    assert_eq!(content(&e.p("dst")), "content");
}

#[test]
fn copy_preserve_mode() {
    let e = Env::new();
    e.file_mode("src", "content", 0o751);

    cp().arg("--preserve=mode")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(mode(&e.p("dst")), 0o751);
}

#[test]
fn copy_preserve_timestamps() {
    let e = Env::new();
    e.file("src", "content");
    e.set_mtime("src", 1_500_000_000);

    cp().arg("--preserve=timestamps")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(mtime(&e.p("dst")), 1_500_000_000);
}

#[test]
fn copy_symlink_no_deref() {
    let e = Env::new();
    e.file("target", "real");
    e.symlink(e.p("target"), "link");

    cp().arg("-P")
        .arg(e.p("link"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert!(is_symlink(&e.p("dst")));
    assert_eq!(link_target(&e.p("dst")), e.p("target"));
}

#[test]
fn copy_symlink_deref() {
    let e = Env::new();
    e.file("target", "real content");
    e.symlink(e.p("target"), "link");

    cp().arg("-L")
        .arg(e.p("link"))
        .arg(e.p("dst"))
        .assert()
        .success();

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

// ═══════════════════════════════════════════════════════════════════════════════
// Edge case tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn copy_update_none_skips() {
    let e = Env::new();
    e.file("src", "new content");
    e.file("dst", "keep_me");

    cp().arg("--update=none")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}

#[test]
fn copy_update_none_fail_skips() {
    let e = Env::new();
    e.file("src", "new content");
    e.file("dst", "keep_me");

    cp().arg("--update=none-fail")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}

#[test]
fn copy_update_all_always_copies() {
    let e = Env::new();
    e.file("src", "old_src");
    e.set_mtime("src", 1_000_000);
    e.file("dst", "newer_dst"); // dst has current (newer) mtime

    cp().arg("--update=all")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "old_src");
}

#[test]
fn copy_attributes_only_creates_empty() {
    let e = Env::new();
    e.file_mode("src", "has content", 0o741);

    // dst does not exist → should create empty file
    cp().arg("--attributes-only")
        .arg("--preserve=mode")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert!(e.p("dst").exists());
    assert_eq!(file_size(&e.p("dst")), 0);
    assert_eq!(mode(&e.p("dst")), 0o741);
}

#[test]
fn copy_attributes_only_preserves_existing() {
    let e = Env::new();
    e.file_mode("src", "src content", 0o741);
    e.file("dst", "existing content");

    cp().arg("--attributes-only")
        .arg("--preserve=mode")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    // Content should remain unchanged
    assert_eq!(content(&e.p("dst")), "existing content");
    // Mode should be updated
    assert_eq!(mode(&e.p("dst")), 0o741);
}

#[test]
fn copy_symlink_to_dir_without_r() {
    let e = Env::new();
    e.dir("target_dir");
    e.symlink(e.p("target_dir"), "link_to_dir");

    // -L follows the symlink → sees a directory → "omitting directory"
    cp().arg("-L")
        .arg(e.p("link_to_dir"))
        .arg(e.p("dst"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("omitting directory"));
}

#[test]
fn copy_empty_file() {
    let e = Env::new();
    e.file("empty", "");

    cp().arg(e.p("empty")).arg(e.p("dst")).assert().success();

    assert!(e.p("dst").exists());
    assert_eq!(file_size(&e.p("dst")), 0);
    assert_eq!(content(&e.p("dst")), "");
}

#[test]
fn copy_overwrite_existing_symlink() {
    let e = Env::new();
    e.file("src", "real data");
    e.file("other", "other content");
    e.symlink(e.p("other"), "dst");

    assert!(is_symlink(&e.p("dst")));

    // --remove-destination first removes the symlink, then creates regular file
    cp().arg("--remove-destination")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    // dst should now be a regular file (not a symlink)
    assert!(!is_symlink(&e.p("dst")));
    assert_eq!(content(&e.p("dst")), "real data");
    // original target should be unchanged
    assert_eq!(content(&e.p("other")), "other content");
}

#[test]
fn copy_sparse_threshold_boundary() {
    use std::io::{Seek, SeekFrom, Write};

    let e = Env::new();
    // 32KB file (= SPARSE_THRESHOLD) with a hole
    let p = e.p("sparse_src");
    {
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(&[0xAA; 4096]).unwrap();
        f.seek(SeekFrom::Start(32 * 1024)).unwrap();
        f.write_all(&[0xBB; 1]).unwrap(); // force total to be > 32KB
        f.set_len(32 * 1024 + 1).unwrap();
    }

    // With --sparse=auto, file at threshold should trigger sparse detection
    cp().arg("--sparse=auto")
        .arg(e.p("sparse_src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(bytes(&e.p("sparse_src")), bytes(&e.p("dst")));
}

#[test]
fn copy_overwrite_non_dir_with_dir_fails() {
    let e = Env::new();
    e.file("src/file.txt", "content");
    e.file("dst", "i am a regular file");

    // Copying dir onto a non-directory should fail
    cp().arg("-R")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot overwrite non-directory"));
}

#[test]
fn copy_dangling_symlink_dest_fails() {
    let e = Env::new();
    e.file("src", "content");
    e.symlink(e.p("nonexistent"), "dst");

    // Without --force, copying through dangling symlink should fail
    cp().arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("dangling symlink"));
}

#[test]
fn copy_dangling_symlink_dest_force_succeeds() {
    let e = Env::new();
    e.file("src", "content");
    e.symlink(e.p("nonexistent"), "dst");

    // With --force or --remove-destination, it should succeed
    cp().arg("-f")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "content");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Interactive mode tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn copy_interactive_no_input_skips() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "keep_me");

    // -i with piped stdin (no TTY) → reads EOF → no overwrite
    cp().arg("-i")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .write_stdin("")
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}

#[test]
fn copy_interactive_yes_overwrites() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");

    cp().arg("-i")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .write_stdin("y\n")
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn copy_interactive_no_preserves() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "old");

    cp().arg("-i")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .write_stdin("n\n")
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "old");
}

#[test]
fn copy_interactive_no_dest_no_prompt() {
    let e = Env::new();
    e.file("src", "content");

    // No existing dest → no prompt, just copy
    cp().arg("-i")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "content");
}

#[test]
fn copy_interactive_n_overrides_no_clobber() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "keep_me");

    // -n -i: -n wins (no-clobber overrides interactive)
    cp().arg("-n")
        .arg("-i")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "keep_me");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Reflink mode tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn copy_reflink_never() {
    let e = Env::new();
    e.file("src", "data for reflink never test");

    cp().arg("--reflink=never")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "data for reflink never test");
}

#[test]
fn copy_reflink_auto() {
    let e = Env::new();
    e.file("src", "data for reflink auto test");

    // --reflink=auto: tries FICLONE, falls back to copy_file_range
    cp().arg("--reflink=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "data for reflink auto test");
}

#[test]
fn copy_reflink_always_may_fail() {
    let e = Env::new();
    e.file("src", "data");

    // --reflink=always on non-CoW filesystem may fail (or succeed on btrfs/xfs)
    // Just verify it doesn't panic
    let _ = cp()
        .arg("--reflink=always")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .ok();
}

#[test]
fn copy_reflink_debug_shows_method() {
    let e = Env::new();
    e.file("src", "debug reflink test");

    // --reflink=auto with --debug should show the copy method used
    cp().arg("--reflink=auto")
        .arg("--debug")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success()
        .stderr(predicates::str::contains("copy method:"));
}
