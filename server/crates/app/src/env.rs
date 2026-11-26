//! Builds the map of variables `Config::from_vars` reads: `.env` first,
//! the process environment over it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::{fmt, fs, io};

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
/// variable wins. Entries that are not valid UTF-8 are skipped, since no
/// variable the server reads can be one.
pub fn overlay(
    file: impl IntoIterator<Item = (String, String)>,
    process: impl IntoIterator<Item = (OsString, OsString)>,
) -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = file.into_iter().collect();
    for (name, value) in process {
        if let (Ok(name), Ok(value)) = (name.into_string(), value.into_string()) {
            vars.insert(name, value);
        }
    }
    vars
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
        let vars = overlay(owned(&[("DRY_RUN", "false")]), os(&[("DRY_RUN", "true")]));
        assert_eq!(vars["DRY_RUN"], "true");
    }

    #[test]
    fn file_only_kept() {
        let vars = overlay(owned(&[("SPAM_THRESHOLD", "80")]), os(&[("PATH", "/bin")]));
        assert_eq!(vars["SPAM_THRESHOLD"], "80");
        assert_eq!(vars["PATH"], "/bin");
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_skipped() {
        use std::os::unix::ffi::OsStringExt;

        let process = vec![
            (OsString::from("DRY_RUN"), OsString::from_vec(vec![0xff])),
            (OsString::from_vec(vec![0xfe]), OsString::from("x")),
            (OsString::from("DATA_DIR"), OsString::from("/srv/um")),
        ];
        let vars = overlay(owned(&[("DRY_RUN", "false")]), process);
        assert_eq!(vars["DRY_RUN"], "false");
        assert_eq!(vars["DATA_DIR"], "/srv/um");
        assert_eq!(vars.len(), 2);
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
