use std::path::PathBuf;

use crate::cli::{Cli, ReflinkMode, SparseMode, UpdateMode};

/// Resolved copy options from CLI flags.
#[derive(Debug, Clone)]
pub struct CopyOptions {
    pub recursive: bool,
    pub force: bool,
    pub interactive: bool,
    pub no_clobber: bool,
    pub verbose: bool,
    pub debug: bool,
    pub progress: bool,
    pub hard_link: bool,
    pub symbolic_link: bool,
    pub attributes_only: bool,
    pub remove_destination: bool,
    pub strip_trailing_slashes: bool,
    pub one_file_system: bool,
    pub parents: bool,
    pub no_target_directory: bool,
    pub target_directory: Option<PathBuf>,

    // Dereference behavior
    pub dereference: Dereference,

    // Preservation
    pub preserve_mode: bool,
    pub preserve_ownership: bool,
    pub preserve_timestamps: bool,
    pub preserve_links: bool,
    pub preserve_xattr: bool,
    pub preserve_acl: bool,

    // Reflink
    pub reflink: ReflinkMode,

    // Sparse
    pub sparse: SparseMode,

    // Update
    pub update: Option<UpdateMode>,

    // Backup
    pub backup: BackupMode,
    pub backup_suffix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dereference {
    /// Never follow symlinks (-P, default for -R)
    Never,
    /// Follow symlinks given on command line only (-H)
    CommandLine,
    /// Always follow symlinks (-L)
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupMode {
    None,
    Numbered,
    Existing,
    Simple,
}

impl CopyOptions {
    pub fn from_cli(cli: &Cli) -> Self {
        let debug = cli.debug;
        let verbose = cli.verbose || debug;

        // Resolve dereference: last specified wins, default depends on -R
        let dereference = if cli.dereference {
            Dereference::Always
        } else if cli.no_dereference || cli.no_deref_preserve_links {
            Dereference::Never
        } else if cli.dereference_args {
            Dereference::CommandLine
        } else if cli.recursive {
            Dereference::Never
        } else {
            Dereference::CommandLine
        };

        // Resolve preservation
        let archive = cli.archive;
        let mut preserve_mode = archive || cli.preserve_default;
        let mut preserve_ownership = archive || cli.preserve_default;
        let mut preserve_timestamps = archive || cli.preserve_default;
        let mut preserve_links = archive || cli.no_deref_preserve_links;
        let mut preserve_xattr = archive;
        let mut preserve_acl = false;
        let mut _preserve_context = archive;
        let mut _preserve_all = archive;

        if let Some(ref attrs) = cli.preserve {
            for attr in attrs {
                match attr.as_str() {
                    "mode" => preserve_mode = true,
                    "ownership" => preserve_ownership = true,
                    "timestamps" => preserve_timestamps = true,
                    "links" => preserve_links = true,
                    "xattr" => preserve_xattr = true,
                    "acl" => preserve_acl = true,
                    "context" => _preserve_context = true,
                    "all" => {
                        preserve_mode = true;
                        preserve_ownership = true;
                        preserve_timestamps = true;
                        preserve_links = true;
                        preserve_xattr = true;
                        preserve_acl = true;
                        _preserve_context = true;
                        _preserve_all = true;
                    }
                    _ => {}
                }
            }
        }

        if let Some(ref attrs) = cli.no_preserve {
            for attr in attrs {
                match attr.as_str() {
                    "mode" => preserve_mode = false,
                    "ownership" => preserve_ownership = false,
                    "timestamps" => preserve_timestamps = false,
                    "links" => preserve_links = false,
                    "xattr" => preserve_xattr = false,
                    "acl" => preserve_acl = false,
                    "context" => _preserve_context = false,
                    "all" => {
                        preserve_mode = false;
                        preserve_ownership = false;
                        preserve_timestamps = false;
                        preserve_links = false;
                        preserve_xattr = false;
                        preserve_acl = false;
                        _preserve_context = false;
                        _preserve_all = false;
                    }
                    _ => {}
                }
            }
        }

        // Resolve reflink
        let reflink = cli.reflink.unwrap_or(ReflinkMode::Auto);

        // Resolve sparse
        let sparse = cli.sparse.unwrap_or(SparseMode::Auto);

        // Resolve backup
        let backup = resolve_backup(cli);
        let backup_suffix = cli
            .suffix
            .clone()
            .or_else(|| std::env::var("SIMPLE_BACKUP_SUFFIX").ok())
            .unwrap_or_else(|| "~".to_string());

        Self {
            recursive: cli.recursive || archive,
            force: cli.force,
            interactive: cli.interactive,
            no_clobber: cli.no_clobber && !cli.interactive,
            verbose,
            debug,
            progress: cli.progress,
            hard_link: cli.hard_link,
            symbolic_link: cli.symbolic_link,
            attributes_only: cli.attributes_only,
            remove_destination: cli.remove_destination,
            strip_trailing_slashes: cli.strip_trailing_slashes,
            one_file_system: cli.one_file_system,
            parents: cli.parents,
            no_target_directory: cli.no_target_directory,
            target_directory: cli.target_directory.clone(),
            dereference,
            preserve_mode,
            preserve_ownership,
            preserve_timestamps,
            preserve_links,
            preserve_xattr,
            preserve_acl,
            reflink,
            sparse,
            update: cli.update,
            backup,
            backup_suffix,
        }
    }
}

fn resolve_backup(cli: &Cli) -> BackupMode {
    if let Some(ref ctrl) = cli.backup {
        parse_backup_control(ctrl)
    } else if cli.simple_backup {
        // Check VERSION_CONTROL env
        if let Ok(vc) = std::env::var("VERSION_CONTROL") {
            parse_backup_control(&vc)
        } else {
            BackupMode::Simple
        }
    } else {
        BackupMode::None
    }
}

fn parse_backup_control(s: &str) -> BackupMode {
    match s {
        "none" | "off" => BackupMode::None,
        "numbered" | "t" => BackupMode::Numbered,
        "existing" | "nil" => BackupMode::Existing,
        "simple" | "never" => BackupMode::Simple,
        _ => BackupMode::Existing,
    }
}
