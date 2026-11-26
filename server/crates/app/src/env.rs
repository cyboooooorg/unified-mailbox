//! Builds the map of variables `Config::from_vars` reads: `.env` first,
//! the process environment over it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

/// Reads the `.env` file at `path`. A missing file gives no variables; an
/// unreadable or malformed one is an error.
pub fn read_dotenv(path: &Path) -> Result<Vec<(String, String)>, dotenvy::Error> {
    match dotenvy::from_path_iter(path) {
        Ok(iter) => iter.collect(),
        Err(err) if err.not_found() => Ok(Vec::new()),
        Err(err) => Err(err),
    }
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
    fn dotenv_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(&path, "# comment\nDRY_RUN=false\n").unwrap();
        assert_eq!(read_dotenv(&path).unwrap(), owned(&[("DRY_RUN", "false")]));
    }
}
