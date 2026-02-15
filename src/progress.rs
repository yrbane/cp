use std::io::IsTerminal;
use std::sync::atomic::{AtomicU64, Ordering};

use indicatif::{ProgressBar, ProgressStyle};

/// Create a progress bar for a single file copy.
/// Only displays if `enabled` is true AND stderr is a TTY.
pub fn make_file_progress(total: u64, name: &str, enabled: bool) -> ProgressBar {
    if !enabled || !std::io::stderr().is_terminal() || total == 0 {
        return ProgressBar::hidden();
    }

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} {msg}\n  [{elapsed_precise}] [{wide_bar:.cyan/dark_gray}] \
                 {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
            )
            .unwrap()
            .progress_chars("━╸─"),
    );
    pb.set_message(name.to_string());
    pb
}

/// Create a spinner-style progress bar for recursive directory copies.
/// Shows file count as it progresses.
pub fn make_dir_progress(src_name: &str, enabled: bool) -> ProgressBar {
    if !enabled || !std::io::stderr().is_terminal() {
        return ProgressBar::hidden();
    }

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.set_message(format!("Copying {} ...", src_name));
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

/// Thread-safe file counter for directory progress.
pub struct DirProgressCounter {
    pb: ProgressBar,
    count: AtomicU64,
}

impl DirProgressCounter {
    pub fn new(pb: ProgressBar) -> Self {
        Self {
            pb,
            count: AtomicU64::new(0),
        }
    }

    pub fn inc(&self) {
        let n = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        self.pb.set_message(format!("{} files copied", n));
    }

    pub fn finish(&self) {
        let n = self.count.load(Ordering::Relaxed);
        self.pb.finish_with_message(format!("{} files copied", n));
    }
}
