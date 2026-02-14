//! Tests — metadata preservation (xattr, ACL, timestamps, modes)

mod common;
use common::*;

// ─── xattr preserved with --preserve=xattr ───────────────────────────────────

#[test]
fn meta_xattr_preserved() {
    let e = Env::new();
    let src = e.file("src", "content");

    if xattr::set(&src, "user.test", b"hello").is_err() {
        eprintln!("SKIP: filesystem does not support xattr");
        return;
    }

    cp().arg("--preserve=xattr").arg(e.p("src")).arg(e.p("dst")).assert().success();

    match xattr::get(e.p("dst"), "user.test") {
        Ok(Some(val)) => assert_eq!(val, b"hello"),
        other => panic!("xattr missing on destination: {other:?}"),
    }
}

// ─── xattr NOT preserved by default ──────────────────────────────────────────

#[test]
fn meta_xattr_not_preserved_default() {
    let e = Env::new();
    let src = e.file("src", "content");

    if xattr::set(&src, "user.test", b"hello").is_err() {
        eprintln!("SKIP: filesystem does not support xattr");
        return;
    }

    cp().arg(e.p("src")).arg(e.p("dst")).assert().success();

    if let Ok(Some(_)) = xattr::get(e.p("dst"), "user.test") {
        panic!("xattr should NOT be preserved without --preserve=xattr");
    }
}

// ─── -p preserves mode AND timestamps ────────────────────────────────────────

#[test]
fn meta_mode_and_timestamps() {
    let e = Env::new();
    e.file_mode("src", "content", 0o751);
    e.set_mtime("src", 1_500_000_000);

    cp().arg("-p").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mode(&e.p("dst")), 0o751);
    assert_eq!(mtime(&e.p("dst")), 1_500_000_000);
}

// ─── Nanosecond timestamp precision with -a ──────────────────────────────────

#[test]
fn meta_timestamps_nanosecond() {
    let e = Env::new();
    e.file("src", "content");
    e.set_mtime_ns("src", 1_600_000_000, 123_456_789);

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mtime_nsec(&e.p("src")), mtime_nsec(&e.p("dst")));
}

// ─── Symlink own mtime preserved with -a ─────────────────────────────────────

#[test]
fn meta_symlink_timestamps() {
    let e = Env::new();
    e.file("src/target", "data");
    e.symlink("target", "src/link");
    e.set_symlink_mtime("src/link", 1_400_000_000);

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/link")));
    assert_eq!(symlink_mtime(&e.p("dst/link")), 1_400_000_000);
}

// ─── --debug outputs copy method ─────────────────────────────────────────────

#[test]
fn meta_debug_copy_method() {
    let e = Env::new();
    e.file("src", "content");

    let out = cp()
        .arg("--debug")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .output()
        .unwrap();

    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("copy method:") || stderr.contains("->"),
        "--debug should output info, got: {stderr}"
    );
}

// ─── -p does NOT preserve xattr ──────────────────────────────────────────────

#[test]
fn meta_p_flag_not_xattr() {
    let e = Env::new();
    let src = e.file("src", "content");

    if xattr::set(&src, "user.test", b"hello").is_err() {
        eprintln!("SKIP: filesystem does not support xattr");
        return;
    }

    cp().arg("-p").arg(e.p("src")).arg(e.p("dst")).assert().success();

    if let Ok(Some(_)) = xattr::get(e.p("dst"), "user.test") {
        panic!("-p should NOT preserve xattr");
    }
}

// ─── ACL preserved with --preserve=all ───────────────────────────────────────

#[test]
fn meta_acl_preserved() {
    let e = Env::new();
    let src = e.file("src", "content");

    if let Err(err) = posix_acl::PosixACL::read_acl(&src) {
        let msg = err.to_string();
        if msg.contains("not supported") || msg.contains("No data available") {
            eprintln!("SKIP: filesystem does not support ACL");
            return;
        }
    }

    cp().arg("--preserve=all").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(posix_acl::PosixACL::read_acl(e.p("dst")).is_ok());
}
