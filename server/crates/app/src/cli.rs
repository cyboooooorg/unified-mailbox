//! The command line: no argument serves, `pair-code` prints a code.

use std::ffi::OsString;

pub const USAGE: &str = "usage: unified-mailbox [pair-code]

  (no argument)  serve the API on LISTEN_ADDR
  pair-code      print a one-time code to pair a new device, then exit";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Serve,
    PairCode,
}

/// Parses the arguments, `argv[0]` excluded. The error is a message to
/// print above [`USAGE`].
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let args: Vec<OsString> = args.into_iter().collect();
    match args.as_slice() {
        [] => Ok(Command::Serve),
        [arg] if arg == "pair-code" => Ok(Command::PairCode),
        [arg, ..] => Err(format!("unexpected argument {arg:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_argument_serves() {
        assert_eq!(parse_strs(&[]), Ok(Command::Serve));
    }

    #[test]
    fn pair_code() {
        assert_eq!(parse_strs(&["pair-code"]), Ok(Command::PairCode));
    }

    #[test]
    fn anything_else_is_an_error() {
        for args in [&["pair-code", "x"][..], &["serve"], &["--help"]] {
            assert!(parse_strs(args).is_err(), "{args:?}");
        }
    }
}
