use std::io::IsTerminal;

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
