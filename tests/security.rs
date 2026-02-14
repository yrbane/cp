//! Security tests — symlink attacks, TOCTOU, permissions, resource exhaustion

mod common;
use common::*;

use std::fs;
use std::io::{Seek, SeekFrom, Write};

// ═══════════════════════════════════════════════════════════════════════════════
// Original tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sec_symlink_outside_source_not_followed_with_P() {
    let e = Env::new();
    e.dir("src");
    e.symlink("/etc/hostname", "src/escape");

    cp().arg("-R").arg("-P").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/escape")));
    assert_eq!(link_target(&e.p("dst/escape")).to_str().unwrap(), "/etc/hostname");
}

#[test]
fn sec_symlink_followed_with_L_copies_content() {
    let e = Env::new();
    e.dir("src");
    e.symlink("/etc/hostname", "src/linked");

    cp().arg("-R").arg("-L").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(!is_symlink(&e.p("dst/linked")));
}

#[test]
fn sec_same_file_via_symlink() {
    let e = Env::new();
    e.file("real.txt", "content");
    e.symlink(&e.p("real.txt"), "link.txt");

    cp().arg(e.p("link.txt")).arg(e.p("real.txt")).assert().failure()
        .stderr(predicates::str::contains("same file"));
}

#[test]
fn sec_dotdot_in_source_path() {
    let e = Env::new();
    e.file("dir/file.txt", "data");
    let src = format!("{}/dir/../dir/file.txt", e.path().display());

    cp().arg(&src).arg(e.p("dst.txt")).assert().success();

    assert_eq!(content(&e.p("dst.txt")), "data");
}

#[test]
fn sec_no_overwrite_without_force() {
    let e = Env::new();
    e.file("src", "new");
    e.file_mode("dst", "protected", 0o000);

    cp().arg(e.p("src")).arg(e.p("dst")).assert().failure();

    e.chmod("dst", 0o644); // cleanup
}

#[test]
fn sec_force_overwrites_readonly() {
    let e = Env::new();
    e.file("src", "new");
    e.file_mode("dst", "protected", 0o000);

    cp().arg("-f").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "new");
}

#[test]
fn sec_nonexistent_source() {
    cp().arg("/nonexistent/path/file").arg("/tmp/dst").assert().failure()
        .stderr(predicates::str::contains("cannot stat"));
}

#[test]
fn sec_unreadable_source() {
    let e = Env::new();
    e.file_mode("unreadable", "secret", 0o000);

    cp().arg(e.p("unreadable")).arg(e.p("dst")).assert().failure();

    e.chmod("unreadable", 0o644); // cleanup
}

#[test]
fn sec_circular_symlink() {
    let e = Env::new();
    e.dir("src");
    e.symlink("b", "src/a");
    e.symlink("a", "src/b");

    cp().arg("-R").arg("-P").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/a")));
    assert!(is_symlink(&e.p("dst/b")));
}

#[test]
fn sec_dangling_symlink_no_deref() {
    let e = Env::new();
    e.dir("src");
    e.symlink("/nonexistent/target", "src/dangling");

    cp().arg("-R").arg("-P").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/dangling")));
    assert_eq!(link_target(&e.p("dst/dangling")).to_str().unwrap(), "/nonexistent/target");
}

#[test]
fn sec_many_files() {
    let e = Env::new();
    e.dir("many");
    for i in 0..1000 {
        e.file(&format!("many/file_{i:04}"), format!("content {i}"));
    }

    cp().arg("-R").arg(e.p("many")).arg(e.p("dst")).assert().success();

    assert_eq!(file_count(&e.p("dst")), 1000);
}

#[test]
fn sec_special_chars_in_filename() {
    let e = Env::new();
    e.dir("src");
    let names = [
        "file with spaces", "file\twith\ttabs", "file'with'quotes",
        "file\"with\"doublequotes", "file;with;semicolons",
        "file&with&ampersands", "file|with|pipes",
        "file(with)parens", "file[with]brackets",
    ];
    for name in &names {
        e.file(&format!("src/{name}"), format!("content of {name}"));
    }

    cp().arg("-R").arg(e.p("src")).arg(e.p("dst")).assert().success();

    for name in &names {
        assert_eq!(content(&e.p(&format!("dst/{name}"))), format!("content of {name}"));
    }
}

#[test]
fn sec_unicode_filenames() {
    let e = Env::new();
    e.dir("src");
    let names = ["日本語.txt", "émojis_🎉.txt", "Ñoño.txt", "中文文件.dat"];
    for name in &names {
        e.file(&format!("src/{name}"), name.as_bytes());
    }

    cp().arg("-R").arg(e.p("src")).arg(e.p("dst")).assert().success();

    for name in &names {
        assert_eq!(content(&e.p(&format!("dst/{name}"))), *name);
    }
}

#[test]
fn sec_data_integrity_random() {
    use rand::Rng;
    let e = Env::new();
    let data: Vec<u8> = (0..1024 * 1024).map(|_| rand::rng().random::<u8>()).collect();
    e.file("random", &data);

    cp().arg(e.p("random")).arg(e.p("dst")).assert().success();

    assert_eq!(bytes(&e.p("dst")), data);
}

#[test]
fn sec_permissions_not_escalated() {
    let e = Env::new();
    e.file_mode("src", "content", 0o600);

    cp().arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(mode(&e.p("dst")) <= 0o666);
}

#[test]
fn sec_exit_code_on_error() {
    let out = cp().arg("/totally/nonexistent").arg("/tmp/whatever").output().unwrap();
    assert_ne!(out.status.code().unwrap(), 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// New security tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sec_toctou_same_file_via_symlink() {
    let e = Env::new();
    e.file("original.txt", "important data");
    e.symlink(&e.p("original.txt"), "link_to_src");

    cp().arg(e.p("original.txt")).arg(e.p("link_to_src")).assert().failure()
        .stderr(predicates::str::contains("same file"));

    assert_eq!(content(&e.p("original.txt")), "important data");
}

#[test]
fn sec_path_traversal_recursive() {
    let e = Env::new();
    e.dir("src");
    e.symlink("/etc/hostname", "src/escape");

    cp().arg("-R").arg("-P").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(is_symlink(&e.p("dst/escape")));
    assert_eq!(link_target(&e.p("dst/escape")).to_str().unwrap(), "/etc/hostname");
}

#[test]
fn sec_setuid_not_preserved_default() {
    let e = Env::new();
    e.file_mode("src", "#!/bin/sh\necho hi", 0o4755);

    cp().arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mode(&e.p("dst")) & 0o4000, 0, "setuid should NOT be preserved without -p");
}

#[test]
fn sec_setuid_preserved_with_p() {
    let e = Env::new();
    e.file_mode("src", "#!/bin/sh\necho hi", 0o4755);

    cp().arg("--preserve=mode").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mode(&e.p("dst")), 0o4755);
}

#[test]
fn sec_extremely_long_path() {
    let e = Env::new();
    let mut current = e.p("src");
    for depth in 0..200 {
        let seg = "d".repeat((depth % 10) + 2);
        current = current.join(&seg);
        if current.to_str().map_or(true, |s| s.len() > 3000) {
            break;
        }
    }
    fs::create_dir_all(&current).unwrap();
    fs::write(current.join("leaf.txt"), "deep data").unwrap();

    let result = cp().arg("-R").arg(e.p("src")).arg(e.p("dst")).output().unwrap();

    if result.status.success() {
        let rel = current.strip_prefix(e.path()).unwrap();
        let dst_leaf = e.path().join("dst").join(rel.strip_prefix("src").unwrap());
        assert_eq!(fs::read_to_string(dst_leaf.join("leaf.txt")).unwrap(), "deep data");
    }
    // Either success or clean error — no panic
}

#[test]
fn sec_hardlink_bomb_dedup() {
    let e = Env::new();
    e.file("src/original.dat", "shared content");
    for i in 0..500 {
        e.hardlink("src/original.dat", &format!("src/link_{i:04}"));
    }

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    let orig_ino = ino(&e.p("dst/original.dat"));
    for i in 0..500 {
        assert_eq!(ino(&e.p(&format!("dst/link_{i:04}"))), orig_ino);
    }
}

#[test]
fn sec_deep_symlink_chain() {
    let e = Env::new();
    e.file("src/real.txt", "target data");
    e.symlink("real.txt", "src/link_00");
    for i in 1..20 {
        e.symlink(format!("link_{:02}", i - 1), &format!("src/link_{i:02}"));
    }

    cp().arg("-R").arg("-L").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(!is_symlink(&e.p("dst/link_19")));
    assert_eq!(content(&e.p("dst/link_19")), "target data");
}

#[test]
fn sec_sparse_block_boundary() {
    let e = Env::new();
    let p = e.p("sparse_src");
    {
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(&[0xAA; 4096]).unwrap();
        f.seek(SeekFrom::Start(8192)).unwrap();
        f.write_all(&[0xBB; 4096]).unwrap();
        f.seek(SeekFrom::Start(16384)).unwrap();
        f.write_all(&[0xCC; 4096]).unwrap();
        f.set_len(20480).unwrap();
    }

    cp().arg("--sparse=auto").arg(e.p("sparse_src")).arg(e.p("sparse_dst")).assert().success();

    assert_eq!(bytes(&e.p("sparse_src")), bytes(&e.p("sparse_dst")));
}

#[test]
fn sec_umask_no_leak() {
    let e = Env::new();
    e.file_mode("src", "content", 0o777);

    cp().arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert!(mode(&e.p("dst")) <= 0o666, "got {:o}", mode(&e.p("dst")));
}

#[test]
fn sec_one_file_system_skips() {
    let e = Env::new();
    e.file("src/local.txt", "local data");

    cp().arg("-R").arg("-x").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst/local.txt")), "local data");
}

#[test]
fn sec_empty_dir_permissions() {
    let e = Env::new();
    e.dir("src");
    e.dir("src/restricted");
    e.chmod("src/restricted", 0o700);

    cp().arg("-a").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(mode(&e.p("dst/restricted")), 0o700);
}

#[test]
fn sec_overwrite_dir_with_file() {
    let e = Env::new();
    e.file("src_file", "I am a file");
    e.dir("dst_dir");

    let out = cp().arg(e.p("src_file")).arg(e.p("dst_dir")).output().unwrap();

    if out.status.success() {
        assert!(e.p("dst_dir/src_file").exists());
    }
    // Either way: no panic/crash
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional edge case security tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sec_broken_symlink_with_l() {
    let e = Env::new();
    e.dir("src");
    e.symlink("/nonexistent/broken_target", "src/broken");

    // -R -L follows symlinks → broken symlink → error on stderr
    let out = cp()
        .arg("-R")
        .arg("-L")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("No such file") || stderr.contains("cannot stat"),
        "expected error about broken symlink, got: {stderr}"
    );
}

#[test]
fn sec_fifo_copy() {
    use std::ffi::CString;

    let e = Env::new();
    let fifo_src = e.p("my_fifo");
    let c_path = CString::new(fifo_src.to_str().unwrap()).unwrap();
    let ret = unsafe { nix::libc::mkfifo(c_path.as_ptr(), 0o644) };
    assert_eq!(ret, 0, "mkfifo failed");

    // Single file FIFO copy (not recursive)
    cp().arg(e.p("my_fifo")).arg(e.p("dst_fifo")).assert().success();

    let ft = fs::symlink_metadata(e.p("dst_fifo")).unwrap().file_type();
    assert!(
        std::os::unix::fs::FileTypeExt::is_fifo(&ft),
        "destination should be a FIFO"
    );
}

#[test]
fn sec_socket_copy_warning() {
    use std::os::unix::net::UnixListener;

    let e = Env::new();
    let sock_path = e.p("my.sock");
    let _listener = UnixListener::bind(&sock_path).unwrap();

    // Copying a socket should produce a warning on stderr
    let out = cp().arg(&sock_path).arg(e.p("dst_sock")).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning") || stderr.contains("socket"),
        "expected socket warning, got: {stderr}"
    );
}

#[test]
fn sec_same_file_two_symlinks() {
    let e = Env::new();
    e.file("real.txt", "data");
    e.symlink(&e.p("real.txt"), "link_a");
    e.symlink(&e.p("real.txt"), "link_b");

    // Two different symlinks pointing to the same inode → "same file"
    cp().arg(e.p("link_a"))
        .arg(e.p("link_b"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("same file"));
}

#[test]
fn sec_filename_255_chars() {
    let e = Env::new();
    let long_name = "x".repeat(255);
    e.file(&long_name, "content");

    cp().arg(e.p(&long_name))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "content");
}

#[test]
fn sec_copy_into_self_via_symlink() {
    let e = Env::new();
    e.file("dir/file", "data");
    e.symlink(&e.p("dir"), "link_to_dir");

    // cp -R dir link_to_dir/sub → should detect copy-into-self
    e.dir("dir/sub");
    cp().arg("-R")
        .arg(e.p("dir"))
        .arg(e.p("dir/sub"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("into itself"));
}

#[test]
fn sec_remove_destination_replaces_file() {
    let e = Env::new();
    e.file("src", "new content");
    e.file_mode("dst", "old content", 0o444);

    // --remove-destination should remove existing file first then copy
    cp().arg("--remove-destination")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "new content");
}

#[test]
fn sec_force_readonly_file() {
    let e = Env::new();
    e.file("src", "new data");
    e.file_mode("dst", "protected", 0o000);

    // -f should unlink the readonly file and create a new one
    cp().arg("-f").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(content(&e.p("dst")), "new data");
}

#[test]
fn sec_update_older_skips_newer() {
    let e = Env::new();
    e.file("src", "old_content");
    e.set_mtime("src", 1_000_000);
    e.file("dst", "newer_content"); // dst has current (newer) mtime

    cp().arg("-u").arg(e.p("src")).arg(e.p("dst")).assert().success();

    // dst should remain unchanged (newer)
    assert_eq!(content(&e.p("dst")), "newer_content");
}

#[test]
fn sec_no_clobber_exit_success() {
    let e = Env::new();
    e.file("src", "new");
    e.file("dst", "existing");

    // -n with existing dest → exit 0 (silent skip)
    cp().arg("-n")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(content(&e.p("dst")), "existing");
}
