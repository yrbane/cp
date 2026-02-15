use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CpError {
    #[error("cannot stat '{path}': {source}")]
    Stat {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("cannot open '{path}' for reading: {source}")]
    OpenRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("cannot create regular file '{path}': {source}")]
    CreateFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("cannot create directory '{path}': {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to read from '{path}': {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write to '{path}': {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("'{src}' and '{dst}' are the same file")]
    SameFile { src: PathBuf, dst: PathBuf },

    #[error("cannot copy a directory, '{path}', into itself, '{dest}'")]
    CopyIntoSelf { path: PathBuf, dest: PathBuf },

    #[error("-r not specified; omitting directory '{path}'")]
    OmitDirectory { path: PathBuf },

    #[error("missing destination file operand after '{src}'")]
    MissingDestination { src: String },

    #[error("missing file operand")]
    MissingOperand,

    #[error("target '{path}' is not a directory")]
    NotADirectory { path: PathBuf },

    #[error("cannot overwrite non-directory '{dst}' with directory '{src}'")]
    #[allow(dead_code)]
    OverwriteNonDir { src: PathBuf, dst: PathBuf },

    #[error("will not overwrite just-created '{path}' with '{src}'")]
    #[allow(dead_code)]
    WillNotOverwrite { path: PathBuf, src: PathBuf },

    #[error("cannot copy '{src}' to '{dst}': {reason}")]
    Copy {
        src: PathBuf,
        dst: PathBuf,
        reason: String,
    },

    #[error("failed to preserve ownership of '{path}': {source}")]
    Chown { path: PathBuf, source: nix::Error },

    #[error("failed to preserve permissions of '{path}': {source}")]
    Chmod {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to preserve timestamps of '{path}': {source}")]
    Timestamps {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to preserve extended attributes of '{path}': {source}")]
    Xattr {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to preserve ACL of '{path}': {msg}")]
    Acl { path: PathBuf, msg: String },

    #[error("cannot create symbolic link '{dst}': {source}")]
    Symlink {
        dst: PathBuf,
        source: std::io::Error,
    },

    #[error("cannot create hard link '{dst}' => '{src}': {source}")]
    HardLink {
        src: PathBuf,
        dst: PathBuf,
        source: std::io::Error,
    },

    #[error("cannot create special file '{path}': {source}")]
    MkNod { path: PathBuf, source: nix::Error },

    #[error("cannot read symbolic link '{path}': {source}")]
    ReadLink {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("not writing through dangling symlink '{path}'")]
    #[allow(dead_code)]
    DanglingSymlink { path: PathBuf },

    #[error("cannot remove '{path}': {source}")]
    Remove {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to seek in '{path}': {source}")]
    Seek {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{0}")]
    #[allow(dead_code)]
    Other(String),
}

pub type CpResult<T> = Result<T, CpError>;
