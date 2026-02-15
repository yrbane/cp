#![no_main]

use libfuzzer_sys::fuzz_target;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

fuzz_target!(|data: &[u8]| {
    // Fuzz path handling utilities with arbitrary byte sequences
    let os = OsStr::from_bytes(data);
    let p = Path::new(os);

    // strip_trailing_slashes: should never panic
    let _ = cp::util::strip_trailing_slashes(p);

    // build_dest_path: should never panic
    let dest = Path::new("/tmp/fuzz_dst");
    let _ = cp::util::build_dest_path(p, dest, false, false);
    let _ = cp::util::build_dest_path(p, dest, true, false);
    let _ = cp::util::build_dest_path(p, dest, false, true);
    let _ = cp::util::build_dest_path(p, dest, true, true);

    // resolve_target with fuzzed paths: should return Err, not panic
    if data.len() > 2 {
        let split = data.len() / 2;
        let p1 = PathBuf::from(OsStr::from_bytes(&data[..split]));
        let p2 = PathBuf::from(OsStr::from_bytes(&data[split..]));
        let paths = vec![p1, p2];
        let _ = cp::util::resolve_target(&paths, &None, false);
    }
});
