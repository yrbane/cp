mod backup;
mod cli;
mod copy;
mod dir;
mod engine;
mod error;
mod metadata;
mod options;
mod progress;
mod sparse;
mod util;

use std::path::{Path, PathBuf};
use std::process;

use clap::Parser;

use crate::cli::Cli;
use crate::error::CpError;
use crate::options::CopyOptions;

fn main() {
    let cli = Cli::parse();
    let opts = CopyOptions::from_cli(&cli);

    let exit_code = run(&cli, &opts);
    process::exit(exit_code);
}

fn run(cli: &Cli, opts: &CopyOptions) -> i32 {
    // Resolve sources and destination
    let paths: Vec<PathBuf> = if opts.strip_trailing_slashes {
        cli.paths
            .iter()
            .map(|p| util::strip_trailing_slashes(p))
            .collect()
    } else {
        cli.paths.clone()
    };

    let (sources, dest) =
        match util::resolve_target(&paths, &opts.target_directory, opts.no_target_directory) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("cp: {}", e);
                return 1;
            }
        };

    let dest_is_dir = dest.is_dir();
    let multiple_sources = sources.len() > 1;

    // Multiple sources require dest to be a directory
    if multiple_sources && !dest_is_dir && !opts.no_target_directory {
        eprintln!("cp: target '{}': Not a directory", dest.display());
        return 1;
    }

    let mut exit_code = 0;

    for source in &sources {
        if let Err(e) = copy_source(source, &dest, dest_is_dir, opts) {
            eprintln!("cp: {}", e);
            exit_code = 1;
        }
    }

    exit_code
}

fn copy_source(
    source: &Path,
    dest: &Path,
    dest_is_dir: bool,
    opts: &CopyOptions,
) -> Result<(), CpError> {
    // Check source exists
    let follow = util::should_follow_symlink(source, opts.dereference, true);
    let src_meta = util::get_metadata(source, follow).map_err(|e| CpError::Stat {
        path: source.to_path_buf(),
        source: e,
    })?;

    let is_dir = src_meta.is_dir();

    if is_dir && !opts.recursive {
        return Err(CpError::OmitDirectory {
            path: source.to_path_buf(),
        });
    }

    let target = util::build_dest_path(source, dest, dest_is_dir, opts.parents);

    if is_dir {
        // Check we're not copying into self
        if let Ok(canon_src) = std::fs::canonicalize(source)
            && let Ok(canon_dst) = std::fs::canonicalize(&target)
            && canon_dst.starts_with(&canon_src)
        {
            return Err(CpError::CopyIntoSelf {
                path: source.to_path_buf(),
                dest: target.clone(),
            });
        }

        dir::copy_directory(source, &target, opts)?;

        if opts.verbose {
            eprintln!("'{}' -> '{}'", source.display(), target.display());
        }
    } else {
        // Ensure parent directory exists for --parents
        if opts.parents
            && let Some(parent) = target.parent()
        {
            std::fs::create_dir_all(parent).map_err(|e| CpError::CreateDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let pb = progress::make_file_progress(
            src_meta.len(),
            &source.display().to_string(),
            opts.progress,
        );
        copy::copy_single(source, &target, opts, true, &pb)?;
        pb.finish_and_clear();
    }

    Ok(())
}
