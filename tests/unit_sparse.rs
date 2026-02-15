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

    cp().arg("--sparse=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    let (src_sz, dst_sz) = (file_size(&e.p("src")), file_size(&e.p("dst")));
    assert_eq!(src_sz, dst_sz);
    assert!(blocks(&e.p("dst")) <= blocks(&e.p("src")) + 16);

    let dst_data = bytes(&e.p("dst"));
    assert!(dst_data[..1024 * 1024].iter().all(|&b| b == 0));
    assert!(
        dst_data[1024 * 1024..1024 * 1024 + 4096]
            .iter()
            .all(|&b| b == 0xAA)
    );
}

#[test]
fn sparse_always_creates_holes_from_zeros() {
    let e = Env::new();
    e.file("zeroed", vec![0u8; 1024 * 1024]);

    cp().arg("--sparse=always")
        .arg(e.p("zeroed"))
        .arg(e.p("dst"))
        .assert()
        .success();

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

    cp().arg("--sparse=never")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

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

    cp().arg("--sparse=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(bytes(&e.p("src")), bytes(&e.p("dst")));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Edge case tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sparse_trailing_hole() {
    let e = Env::new();
    // Data at the beginning, hole (zeros) trailing to the end
    sparse_file(&e, "src", &[(0, &[0xDD; 4096])], 512 * 1024);

    cp().arg("--sparse=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("src")), file_size(&e.p("dst")));
    // Dst should use fewer or equal blocks (trailing hole)
    assert!(blocks(&e.p("dst")) <= blocks(&e.p("src")) + 16);
    // Data region integrity
    let dst_bytes = bytes(&e.p("dst"));
    assert!(dst_bytes[..4096].iter().all(|&b| b == 0xDD));
}

#[test]
fn sparse_leading_hole() {
    let e = Env::new();
    // Hole at the beginning, data at the end
    let total = 512 * 1024u64;
    let data_offset = total - 4096;
    sparse_file(&e, "src", &[(data_offset, &[0xEE; 4096])], total);

    cp().arg("--sparse=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("src")), file_size(&e.p("dst")));
    let dst_bytes = bytes(&e.p("dst"));
    // Leading zeros (hole)
    assert!(dst_bytes[..data_offset as usize].iter().all(|&b| b == 0));
    // Trailing data
    assert!(dst_bytes[data_offset as usize..].iter().all(|&b| b == 0xEE));
}

#[test]
fn sparse_fragmented() {
    let e = Env::new();
    // Alternating 64KB data / 64KB hole × 10
    let chunk = 64 * 1024u64;
    let regions: Vec<(u64, Vec<u8>)> = (0..10)
        .map(|i| (i * 2 * chunk, vec![(i + 1) as u8; chunk as usize]))
        .collect();
    let region_refs: Vec<(u64, &[u8])> = regions.iter().map(|(o, d)| (*o, d.as_slice())).collect();
    let total = 20 * chunk;
    sparse_file(&e, "src", &region_refs, total);

    cp().arg("--sparse=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("src")), file_size(&e.p("dst")));
    assert_eq!(bytes(&e.p("src")), bytes(&e.p("dst")));
}

#[test]
fn sparse_below_threshold() {
    let e = Env::new();
    // 31KB file (< SPARSE_THRESHOLD of 32KB) → no sparse detection
    let data = vec![0u8; 31 * 1024];
    e.file("src", &data);

    cp().arg("--sparse=auto")
        .arg("--debug")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success()
        // Should NOT use sparse method, but copy_file_range
        .stderr(predicates::str::contains("copy_file_range"));

    assert_eq!(bytes(&e.p("dst")), data);
}

#[test]
fn sparse_at_threshold_boundary() {
    let e = Env::new();
    // Exactly 32KB (= SPARSE_THRESHOLD) with a hole → sparse detection should activate
    sparse_file(&e, "src", &[(0, &[0xFF; 4096])], 32 * 1024);

    cp().arg("--sparse=auto")
        .arg(e.p("src"))
        .arg(e.p("dst"))
        .assert()
        .success();

    assert_eq!(file_size(&e.p("src")), file_size(&e.p("dst")));
    assert_eq!(bytes(&e.p("src")), bytes(&e.p("dst")));
}
