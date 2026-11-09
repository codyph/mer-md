use anyhow::{Result, bail};
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) const HELP: &str = "mer-md — Turn Mermaid in Markdown into SVGs. Offline and portable.

Usage: mer-md [PATH] [--restore]

  PATH        Markdown file or project directory (default: .)
  --restore   Restore Mermaid code blocks
  -h, --help  Show help
";

#[derive(Debug)]
pub(crate) struct Options {
    pub path: PathBuf,
    pub restore: bool,
}

pub(crate) fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>> {
    let mut options = Options {
        path: PathBuf::from("."),
        restore: false,
    };
    let mut has_path = false;
    let mut positional_only = false;
    for arg in args {
        if !positional_only {
            match arg.to_str() {
                Some("--") => {
                    positional_only = true;
                    continue;
                }
                Some("-h" | "--help") => return Ok(None),
                Some("--restore") => {
                    options.restore = true;
                    continue;
                }
                Some(flag) if flag.starts_with('-') => bail!("unknown option {flag:?}"),
                _ => {}
            }
        }
        if has_path {
            bail!("choose one file or project directory");
        }
        options.path = PathBuf::from(arg);
        has_path = true;
    }
    Ok(Some(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_rendering_the_current_directory() {
        let options = parse_args([]).unwrap().unwrap();
        assert_eq!(options.path, PathBuf::from("."));
        assert!(!options.restore);
    }

    #[test]
    fn restore_accepts_a_path_in_either_order() {
        for args in [["--restore", "project"], ["project", "--restore"]] {
            let options = parse_args(args.map(OsString::from)).unwrap().unwrap();
            assert_eq!(options.path, PathBuf::from("project"));
            assert!(options.restore);
        }
        assert!(
            parse_args(["--restore"].map(OsString::from))
                .unwrap()
                .unwrap()
                .restore
        );
    }

    #[test]
    fn accepts_one_path_and_an_option_terminator() {
        assert!(parse_args(["a.md", "b.md"].map(OsString::from)).is_err());
        let options = parse_args(["--", "-file.md"].map(OsString::from))
            .unwrap()
            .unwrap();
        assert_eq!(options.path, PathBuf::from("-file.md"));
    }
}
