//! Tests — sparse file handling (sparse.rs)

mod common;
use common::*;

use std::io::{Seek, SeekFrom, Write};

/// Create a sparse file by seeking past holes.
fn sparse_file(e: &Env, rel: &str, regions: &[(u64, &[u8])], total: u64) {
    let p = e.p(rel);
    Env::ensure_parent_pub(&p);
    let mut f = std::fs::File::create(&p).unwrap();
    for &(offset, data) in regions {
        f.seek(SeekFrom::Start(offset)).unwrap();
        f.write_all(data).unwrap();
    }
    if total > 0 {
        f.set_len(total).unwrap();
    }
}

#[test]
fn sparse_auto_preserves_holes() {
    let e = Env::new();
    // 1MB hole, then 4KB of data
    sparse_file(&e, "src", &[(1024 * 1024, &[0xAA; 4096])], 0);

    cp().arg("--sparse=auto").arg(e.p("src")).arg(e.p("dst")).assert().success();

    let (src_sz, dst_sz) = (file_size(&e.p("src")), file_size(&e.p("dst")));
    assert_eq!(src_sz, dst_sz);
    assert!(blocks(&e.p("dst")) <= blocks(&e.p("src")) + 16);

    let dst_data = bytes(&e.p("dst"));
    assert!(dst_data[..1024 * 1024].iter().all(|&b| b == 0));
    assert!(dst_data[1024 * 1024..1024 * 1024 + 4096].iter().all(|&b| b == 0xAA));
}

#[test]
fn sparse_always_creates_holes_from_zeros() {
    let e = Env::new();
    e.file("zeroed", &vec![0u8; 1024 * 1024]);

    cp().arg("--sparse=always").arg(e.p("zeroed")).arg(e.p("dst")).assert().success();

    assert_eq!(file_size(&e.p("zeroed")), file_size(&e.p("dst")));
    assert!(
        blocks(&e.p("dst")) < blocks(&e.p("zeroed")),
        "sparse dst ({}) should use fewer blocks than non-sparse src ({})",
        blocks(&e.p("dst")),
        blocks(&e.p("zeroed"))
    );
}

#[test]
fn sparse_never_copies_full() {
    let e = Env::new();
    sparse_file(&e, "src", &[(512 * 1024, &[0xBB; 4096])], 0);

    cp().arg("--sparse=never").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(bytes(&e.p("src")), bytes(&e.p("dst")));
}

#[test]
fn sparse_multiple_regions() {
    let e = Env::new();
    sparse_file(
        &e,
        "src",
        &[
            (0, &[0x11; 4096]),
            (1024 * 1024, &[0x22; 4096]),
            (2 * 1024 * 1024, &[0x33; 4096]),
        ],
        0,
    );

    cp().arg("--sparse=auto").arg(e.p("src")).arg(e.p("dst")).assert().success();

    assert_eq!(bytes(&e.p("src")), bytes(&e.p("dst")));
}
