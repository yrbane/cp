// Include the cli module for man page generation.
// We use include! to avoid needing the full crate context.

fn main() {
    // Only generate man page if building docs or on release
    if std::env::var("GENERATE_MAN").is_ok() || std::env::var("PROFILE").as_deref() == Ok("release")
    {
        generate_man_page();
    }
}

fn generate_man_page() {
    use clap::CommandFactory;

    // We need to define a minimal version of the CLI struct here
    // since build scripts can't depend on the crate being built.
    let cmd = build_cli_command();

    let man = clap_mangen::Man::new(cmd);
    let out_dir =
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    let mut buf = Vec::new();
    man.render(&mut buf).expect("Failed to render man page");
    std::fs::write(out_dir.join("cp.1"), buf).expect("Failed to write man page");
}

fn build_cli_command() -> clap::Command {
    clap::Command::new("cp")
        .version(env!("CARGO_PKG_VERSION"))
        .about("copy files and directories")
        .arg(clap::Arg::new("archive").short('a').long("archive").action(clap::ArgAction::SetTrue).help("Same as -dR --preserve=all"))
        .arg(clap::Arg::new("attributes-only").long("attributes-only").action(clap::ArgAction::SetTrue).help("don't copy the file data, just the attributes"))
        .arg(clap::Arg::new("backup").long("backup").value_name("CONTROL").num_args(0..=1).default_missing_value("existing").help("make a backup of each existing destination file"))
        .arg(clap::Arg::new("b").short('b').action(clap::ArgAction::SetTrue).help("like --backup but does not accept an argument"))
        .arg(clap::Arg::new("copy-contents").long("copy-contents").action(clap::ArgAction::SetTrue).help("copy contents of special files when recursive"))
        .arg(clap::Arg::new("d").short('d').action(clap::ArgAction::SetTrue).help("same as --no-dereference --preserve=links"))
        .arg(clap::Arg::new("debug").long("debug").action(clap::ArgAction::SetTrue).help("explain how a file is copied.  Implies -v"))
        .arg(clap::Arg::new("force").short('f').long("force").action(clap::ArgAction::SetTrue).help("if an existing destination file cannot be opened, remove it and try again"))
        .arg(clap::Arg::new("interactive").short('i').long("interactive").action(clap::ArgAction::SetTrue).help("prompt before overwrite"))
        .arg(clap::Arg::new("H").short('H').action(clap::ArgAction::SetTrue).help("follow command-line symbolic links in SOURCE"))
        .arg(clap::Arg::new("link").short('l').long("link").action(clap::ArgAction::SetTrue).help("hard link files instead of copying"))
        .arg(clap::Arg::new("dereference").short('L').long("dereference").action(clap::ArgAction::SetTrue).help("always follow symbolic links in SOURCE"))
        .arg(clap::Arg::new("no-clobber").short('n').long("no-clobber").action(clap::ArgAction::SetTrue).help("do not overwrite an existing file"))
        .arg(clap::Arg::new("no-dereference").short('P').long("no-dereference").action(clap::ArgAction::SetTrue).help("never follow symbolic links in SOURCE"))
        .arg(clap::Arg::new("p").short('p').action(clap::ArgAction::SetTrue).help("same as --preserve=mode,ownership,timestamps"))
        .arg(clap::Arg::new("preserve").long("preserve").value_name("ATTR_LIST").num_args(0..=1).default_missing_value("mode,ownership,timestamps").help("preserve the specified attributes"))
        .arg(clap::Arg::new("no-preserve").long("no-preserve").value_name("ATTR_LIST").help("don't preserve the specified attributes"))
        .arg(clap::Arg::new("parents").long("parents").action(clap::ArgAction::SetTrue).help("use full source file name under DIRECTORY"))
        .arg(clap::Arg::new("recursive").short('R').short_alias('r').long("recursive").action(clap::ArgAction::SetTrue).help("copy directories recursively"))
        .arg(clap::Arg::new("reflink").long("reflink").value_name("WHEN").num_args(0..=1).default_missing_value("always").help("control clone/CoW copies"))
        .arg(clap::Arg::new("remove-destination").long("remove-destination").action(clap::ArgAction::SetTrue).help("remove each existing destination file before attempting to open it"))
        .arg(clap::Arg::new("sparse").long("sparse").value_name("WHEN").help("control creation of sparse files"))
        .arg(clap::Arg::new("strip-trailing-slashes").long("strip-trailing-slashes").action(clap::ArgAction::SetTrue).help("remove any trailing slashes from each SOURCE argument"))
        .arg(clap::Arg::new("symbolic-link").short('s').long("symbolic-link").action(clap::ArgAction::SetTrue).help("make symbolic links instead of copying"))
        .arg(clap::Arg::new("suffix").short('S').long("suffix").value_name("SUFFIX").help("override the usual backup suffix"))
        .arg(clap::Arg::new("target-directory").short('t').long("target-directory").value_name("DIRECTORY").help("copy all SOURCE arguments into DIRECTORY"))
        .arg(clap::Arg::new("no-target-directory").short('T').long("no-target-directory").action(clap::ArgAction::SetTrue).help("treat DEST as a normal file"))
        .arg(clap::Arg::new("update").short('u').long("update").value_name("CONTROL").num_args(0..=1).default_missing_value("older").help("control which existing files are updated"))
        .arg(clap::Arg::new("verbose").short('v').long("verbose").action(clap::ArgAction::SetTrue).help("explain what is being done"))
        .arg(clap::Arg::new("one-file-system").short('x').long("one-file-system").action(clap::ArgAction::SetTrue).help("stay on this file system"))
        .arg(clap::Arg::new("Z").short('Z').action(clap::ArgAction::SetTrue).help("set SELinux security context of destination file to default type"))
        .arg(clap::Arg::new("context").long("context").value_name("CTX").num_args(0..=1).default_missing_value("").help("like -Z, or if CTX is specified then set the SELinux or SMACK security context to CTX"))
        .arg(clap::Arg::new("paths").num_args(1..).required(true))
}
