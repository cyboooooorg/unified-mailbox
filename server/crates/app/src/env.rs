//! Builds the map of variables `Config::from_vars` reads: `.env` first,
//! the process environment over it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::{fmt, fs, io};

use crate::config::{Config, ConfigError, ConfigErrors, KNOWN_VARIABLES};

/// Why `.env` could not be read. It never quotes the file's content: a
/// line that fails to parse may hold a secret.
#[derive(Debug)]
pub enum DotenvError {
    Io(io::Error),
    /// The line number, when it could be found.
    Malformed(Option<usize>),
}

impl fmt::Display for DotenvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DotenvError::Io(err) => write!(f, "cannot read: {err}"),
            DotenvError::Malformed(Some(line)) => write!(f, "malformed line {line}"),
            DotenvError::Malformed(None) => write!(f, "malformed"),
        }
    }
}

/// Reads the `.env` file at `path`. A missing file gives no variables; an
/// unreadable or malformed one is an error.
pub fn read_dotenv(path: &Path) -> Result<Vec<(String, String)>, DotenvError> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(DotenvError::Io(err)),
    };
    dotenvy::from_read_iter(content.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| match err {
            dotenvy::Error::LineParse(text, _) => DotenvError::Malformed(line_of(&content, &text)),
            dotenvy::Error::Io(err) => DotenvError::Io(err),
            _ => DotenvError::Malformed(None),
        })
}

/// The 1-based number of the first line holding `text`, which dotenvy
/// reports as the rest of the line from where parsing failed.
fn line_of(content: &str, text: &str) -> Option<usize> {
    let first = text.lines().next().filter(|first| !first.is_empty())?;
    content
        .lines()
        .position(|line| line.contains(first))
        .map(|index| index + 1)
}

/// Lays the process environment over the `.env` variables: a real
/// variable wins. An entry whose name or value is not valid UTF-8 is
/// skipped, unless its name is in [`KNOWN_VARIABLES`]: then it is an
/// error that never quotes the value, and the `.env` value of that name
/// is dropped so that it does not stand in for the real one.
pub fn overlay(
    file: impl IntoIterator<Item = (String, String)>,
    process: impl IntoIterator<Item = (OsString, OsString)>,
) -> (HashMap<String, String>, Vec<ConfigError>) {
    let mut vars: HashMap<String, String> = file.into_iter().collect();
    let mut errors = Vec::new();
    for (name, value) in process {
        let Ok(name) = name.into_string() else {
            continue;
        };
        match value.into_string() {
            Ok(value) => {
                vars.insert(name, value);
            }
            Err(_) => {
                if let Some(known) = KNOWN_VARIABLES.iter().find(|known| **known == name) {
                    vars.remove(&name);
                    errors.push(ConfigError {
                        variable: known,
                        message: "value is not valid UTF-8".to_owned(),
                    });
                }
            }
        }
    }
    (vars, errors)
}

/// The configuration from the `.env` variables and the process
/// environment, or every error: the encoding errors of [`overlay`]
/// first, then those of [`Config::from_vars`].
pub fn load(
    file: impl IntoIterator<Item = (String, String)>,
    process: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<Config, ConfigErrors> {
    let (vars, mut errors) = overlay(file, process);
    match Config::from_vars(&vars) {
        Ok(config) if errors.is_empty() => Ok(config),
        Ok(_) => Err(ConfigErrors(errors)),
        Err(ConfigErrors(more)) => {
            errors.extend(more);
            Err(ConfigErrors(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use super::*;

    fn os(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs.iter().map(|(k, v)| (k.into(), v.into())).collect()
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn process_beats_file() {
        let (vars, errors) = overlay(owned(&[("DRY_RUN", "false")]), os(&[("DRY_RUN", "true")]));
        assert_eq!(vars["DRY_RUN"], "true");
        assert!(errors.is_empty());
    }

    #[test]
    fn file_only_kept() {
        let (vars, _) = overlay(owned(&[("SPAM_THRESHOLD", "80")]), os(&[("PATH", "/bin")]));
        assert_eq!(vars["SPAM_THRESHOLD"], "80");
        assert_eq!(vars["PATH"], "/bin");
    }

    /// Bytes that are not UTF-8, holding an invented secret.
    #[cfg(unix)]
    fn not_utf8() -> OsString {
        use std::os::unix::ffi::OsStringExt;

        let mut bytes = b"invented-0000".to_vec();
        bytes.push(0xff);
        OsString::from_vec(bytes)
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_known_value_is_an_error_without_the_value() {
        let process = vec![(OsString::from("DATA_DIR"), not_utf8())];
        let (vars, errors) = overlay(owned(&[("DATA_DIR", "/srv/from-dotenv")]), process);
        assert!(!vars.contains_key("DATA_DIR"), "the .env value stands in");
        assert_eq!(errors.len(), 1);
        let message = errors[0].to_string();
        assert_eq!(message, "DATA_DIR: value is not valid UTF-8");
        assert!(!message.contains("invented-0000"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn every_known_variable_not_utf8_is_reported_in_order() {
        let process = KNOWN_VARIABLES
            .iter()
            .map(|name| (OsString::from(name), not_utf8()));
        let errors = load(Vec::new(), process).unwrap_err().0;
        let variables: Vec<_> = errors.iter().map(|e| e.variable).collect();
        assert_eq!(variables, KNOWN_VARIABLES.to_vec());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_unknown_entries_skipped() {
        use std::os::unix::ffi::OsStringExt;

        let process = vec![
            (OsString::from("SOME_OTHER_TOOL"), not_utf8()),
            (OsString::from_vec(vec![0xfe]), OsString::from("x")),
            (OsString::from("DATA_DIR"), OsString::from("/srv/um")),
        ];
        let (vars, errors) = overlay(owned(&[("DRY_RUN", "false")]), process);
        assert!(errors.is_empty());
        assert_eq!(vars["DRY_RUN"], "false");
        assert_eq!(vars["DATA_DIR"], "/srv/um");
        assert_eq!(vars.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn load_lists_encoding_errors_first_then_parse_errors() {
        let process = vec![
            (OsString::from("DRY_RUN"), OsString::from("maybe")),
            (OsString::from("KEY_FILE"), not_utf8()),
        ];
        let errors = load(Vec::new(), process).unwrap_err().0;
        let variables: Vec<_> = errors.iter().map(|e| e.variable).collect();
        assert_eq!(variables, vec!["KEY_FILE", "DRY_RUN"]);
    }

    #[cfg(unix)]
    #[test]
    fn load_fails_on_an_encoding_error_alone() {
        let process = vec![(OsString::from("RUST_LOG"), not_utf8())];
        let errors = load(Vec::new(), process).unwrap_err().0;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].variable, "RUST_LOG");
    }

    #[test]
    fn load_without_errors_gives_the_config() {
        let config = load(Vec::new(), os(&[("DRY_RUN", "false")])).unwrap();
        assert!(!config.dry_run);
    }

    #[test]
    fn missing_dotenv_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_dotenv(&dir.path().join(".env")).unwrap(), vec![]);
    }

    #[test]
    fn malformed_dotenv_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(&path, "NOT VALID\n").unwrap();
        assert!(read_dotenv(&path).is_err());
    }

    #[test]
    fn malformed_dotenv_hides_the_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(
            &path,
            "# comment\nDRY_RUN=false\nSOME_TOKEN=invented-0000 oops\n",
        )
        .unwrap();
        let message = read_dotenv(&path).unwrap_err().to_string();
        assert!(!message.contains("invented-0000"), "{message}");
        assert!(message.contains("line 3"), "{message}");
    }

    #[test]
    fn dotenv_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(&path, "# comment\nDRY_RUN=false\n").unwrap();
        assert_eq!(read_dotenv(&path).unwrap(), owned(&[("DRY_RUN", "false")]));
    }
}
