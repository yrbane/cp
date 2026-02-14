use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};

/// Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.
#[derive(Parser, Debug)]
#[command(
    name = "cp",
    version,
    about = "copy files and directories",
    after_help = "\
Mandatory arguments to long options are mandatory for short options too.

By default, sparse SOURCE files are detected by a crude heuristic and the \
corresponding DEST file is made sparse as well.  That is the behavior \
selected by --sparse=auto.  Specify --sparse=always to create a sparse DEST \
file whenever the SOURCE file contains a long enough sequence of zero bytes.  \
Use --sparse=never to inhibit creation of sparse files.

When --reflink[=always] is specified, perform a lightweight copy, where the \
data blocks are copied only when modified.  If this is not possible the copy \
fails, or if --reflink=auto is specified, fall back to a standard copy.  \
Use --reflink=never to ensure a standard copy is performed.

The backup suffix is '~', unless set with --suffix or SIMPLE_BACKUP_SUFFIX.  \
The version control method may be selected via the --backup option or through \
the VERSION_CONTROL environment variable.  Here are the values:

  none, off       never make backups (even if --backup is given)
  numbered, t     make numbered backups
  existing, nil   numbered if numbered backups exist, simple otherwise
  simple, never   always make simple backups"
)]
pub struct Cli {
    /// Same as --archive; preserve all metadata
    #[arg(short = 'a', long = "archive", action = ArgAction::SetTrue)]
    pub archive: bool,

    /// Don't copy file data, only attributes
    #[arg(long = "attributes-only", action = ArgAction::SetTrue)]
    pub attributes_only: bool,

    /// Make a backup of each existing destination file
    #[arg(long = "backup", value_name = "CONTROL", num_args = 0..=1, default_missing_value = "existing", require_equals = true)]
    pub backup: Option<String>,

    /// Like --backup but does not accept an argument
    #[arg(short = 'b', action = ArgAction::SetTrue)]
    pub simple_backup: bool,

    /// Copy contents of special files when recursive
    #[arg(long = "copy-contents", action = ArgAction::SetTrue)]
    pub copy_contents: bool,

    /// Same as --no-dereference --preserve=links
    #[arg(short = 'd', action = ArgAction::SetTrue)]
    pub no_deref_preserve_links: bool,

    /// Explain how a file is copied (implies -v)
    #[arg(long = "debug", action = ArgAction::SetTrue)]
    pub debug: bool,

    /// If an existing destination file cannot be opened, remove it and try again
    #[arg(short = 'f', long = "force", action = ArgAction::SetTrue)]
    pub force: bool,

    /// Prompt before overwrite (overrides -n)
    #[arg(short = 'i', long = "interactive", action = ArgAction::SetTrue)]
    pub interactive: bool,

    /// Follow symlinks in SOURCE (command-line only)
    #[arg(short = 'H', action = ArgAction::SetTrue)]
    pub dereference_args: bool,

    /// Hard link files instead of copying
    #[arg(short = 'l', long = "link", action = ArgAction::SetTrue)]
    pub hard_link: bool,

    /// Always follow symlinks in SOURCE
    #[arg(short = 'L', long = "dereference", action = ArgAction::SetTrue)]
    pub dereference: bool,

    /// Do not overwrite existing files
    #[arg(short = 'n', long = "no-clobber", action = ArgAction::SetTrue)]
    pub no_clobber: bool,

    /// Never follow symlinks in SOURCE
    #[arg(short = 'P', long = "no-dereference", action = ArgAction::SetTrue)]
    pub no_dereference: bool,

    /// Same as --preserve=mode,ownership,timestamps
    #[arg(short = 'p', action = ArgAction::SetTrue)]
    pub preserve_default: bool,

    /// Preserve specified attributes
    #[arg(long = "preserve", value_name = "ATTR_LIST", num_args = 0..=1, default_missing_value = "mode,ownership,timestamps", value_delimiter = ',')]
    pub preserve: Option<Vec<String>>,

    /// Don't preserve the specified attributes
    #[arg(long = "no-preserve", value_name = "ATTR_LIST", value_delimiter = ',')]
    pub no_preserve: Option<Vec<String>>,

    /// Use full source path under DIRECTORY
    #[arg(long = "parents", action = ArgAction::SetTrue)]
    pub parents: bool,

    /// Copy directories recursively
    #[arg(short = 'R', short_alias = 'r', long = "recursive", action = ArgAction::SetTrue)]
    pub recursive: bool,

    /// Control clone/CoW copies
    #[arg(long = "reflink", value_name = "WHEN", num_args = 0..=1, default_missing_value = "always", require_equals = true)]
    pub reflink: Option<ReflinkMode>,

    /// Remove each existing destination file before copy
    #[arg(long = "remove-destination", action = ArgAction::SetTrue)]
    pub remove_destination: bool,

    /// Control creation of sparse files
    #[arg(long = "sparse", value_name = "WHEN")]
    pub sparse: Option<SparseMode>,

    /// Remove trailing slashes from each SOURCE
    #[arg(long = "strip-trailing-slashes", action = ArgAction::SetTrue)]
    pub strip_trailing_slashes: bool,

    /// Create symbolic links instead of copying
    #[arg(short = 's', long = "symbolic-link", action = ArgAction::SetTrue)]
    pub symbolic_link: bool,

    /// Override the usual backup suffix
    #[arg(short = 'S', long = "suffix", value_name = "SUFFIX")]
    pub suffix: Option<String>,

    /// Copy all SOURCE arguments into DIRECTORY
    #[arg(short = 't', long = "target-directory", value_name = "DIRECTORY")]
    pub target_directory: Option<PathBuf>,

    /// Treat DEST as a normal file
    #[arg(short = 'T', long = "no-target-directory", action = ArgAction::SetTrue)]
    pub no_target_directory: bool,

    /// Copy only when SOURCE is newer or DEST is missing
    #[arg(short = 'u', long = "update", value_name = "CONTROL", num_args = 0..=1, default_missing_value = "older", require_equals = true)]
    pub update: Option<UpdateMode>,

    /// Show progress bar during copy
    #[arg(long = "progress", action = ArgAction::SetTrue)]
    pub progress: bool,

    /// Explain what is being done
    #[arg(short = 'v', long = "verbose", action = ArgAction::SetTrue)]
    pub verbose: bool,

    /// Stay on this file system
    #[arg(short = 'x', long = "one-file-system", action = ArgAction::SetTrue)]
    pub one_file_system: bool,

    /// Set SELinux security context of dest to default type
    #[arg(short = 'Z', action = ArgAction::SetTrue)]
    pub selinux_default: bool,

    /// Like -Z, or if CTX is specified, set SELinux/SMACK security context to CTX
    #[arg(long = "context", value_name = "CTX", num_args = 0..=1, default_missing_value = "")]
    pub context: Option<String>,

    /// Keep directory symlinks in DEST during recursive copy
    #[arg(long = "keep-directory-symlink", action = ArgAction::SetTrue)]
    pub keep_directory_symlink: bool,

    /// Source file(s) and destination
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ReflinkMode {
    Always,
    Auto,
    Never,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum SparseMode {
    Always,
    Auto,
    Never,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum UpdateMode {
    /// Copy when source is newer (default for -u)
    Older,
    /// Unconditionally (synonym for no --update)
    All,
    /// Never overwrite
    None,
    /// Like 'none', also skip if sizes match
    #[value(name = "none-fail")]
    NoneFail,
}
