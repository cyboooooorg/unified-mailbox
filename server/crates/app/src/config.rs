//! The server configuration, read once at startup.
//!
//! `Config::from_vars` turns a map of variables (`.env` overlaid by the
//! process environment) into typed values for the crates that need them.
//! An unset variable, or one that is empty once trimmed, takes its
//! default. Every invalid variable is reported, not only the first.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use domain::{SpamConfidence, SpamPolicy, TagKey};
use sqlite::StoreConfig;
use tracing_subscriber::EnvFilter;

const DEFAULT_DATA_DIR: &str = "./data";
const DATABASE_FILE: &str = "unified-mailbox.db";
const KEY_FILE_NAME: &str = "secret.key";
const DEFAULT_SPAM_THRESHOLD: u8 = 90;
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_LISTEN_ADDR: &str = "127.0.0.1:8000";

/// Everything the server reads from its configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// Holds the database, and the key file unless `KEY_FILE` moves it.
    pub data_dir: PathBuf,
    pub store: StoreConfig,
    pub spam: SpamPolicy,
    pub dry_run: bool,
    /// A `tracing-subscriber` `EnvFilter` directive, already validated.
    pub log_filter: String,
    /// Where the HTTP API listens.
    pub listen_addr: SocketAddr,
    /// The built web client, served outside `/api`; `None` serves the API only.
    pub client_dir: Option<PathBuf>,
}

/// One invalid variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub variable: &'static str,
    /// Quotes the value and says what was expected.
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.variable, self.message)
    }
}

/// Every invalid variable, in the order they were checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigErrors(pub Vec<ConfigError>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, error) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigErrors {}

impl Config {
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Config, ConfigErrors> {
        let mut parser = Parser {
            vars,
            errors: Vec::new(),
        };

        let data_dir = parser
            .get("DATA_DIR")
            .map_or_else(|| PathBuf::from(DEFAULT_DATA_DIR), PathBuf::from);
        let key_file = parser
            .get("KEY_FILE")
            .map_or_else(|| data_dir.join(KEY_FILE_NAME), PathBuf::from);
        let dry_run = parser.bool("DRY_RUN", true);
        let threshold = parser.threshold("SPAM_THRESHOLD");
        let protect_known_senders = parser.bool("PROTECT_KNOWN_SENDERS", true);
        let archive = parser.tag_keys("ARCHIVE_CATEGORIES");
        let log_filter = parser.log_filter("RUST_LOG");
        let listen_addr = parser.socket_addr("LISTEN_ADDR", DEFAULT_LISTEN_ADDR);
        let client_dir = parser.client_dir("CLIENT_DIR");

        if !parser.errors.is_empty() {
            return Err(ConfigErrors(parser.errors));
        }
        Ok(Config {
            store: StoreConfig {
                database: data_dir.join(DATABASE_FILE),
                key_file,
            },
            data_dir,
            spam: SpamPolicy {
                threshold,
                archive,
                protect_known_senders,
            },
            dry_run,
            log_filter,
            listen_addr,
            client_dir,
        })
    }
}

/// Reads variables, collecting errors and returning a fallback so that
/// parsing goes on after an invalid value.
struct Parser<'a> {
    vars: &'a HashMap<String, String>,
    errors: Vec<ConfigError>,
}

impl Parser<'_> {
    /// The trimmed value, or `None` when unset or empty.
    fn get(&self, name: &str) -> Option<&str> {
        self.vars
            .get(name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    fn fail(&mut self, variable: &'static str, value: &str, expected: &str) {
        self.errors.push(ConfigError {
            variable,
            message: format!("{value:?} is not {expected}"),
        });
    }

    fn bool(&mut self, name: &'static str, default: bool) -> bool {
        let Some(value) = self.get(name) else {
            return default;
        };
        match value.to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" | "y" | "on" => true,
            "false" | "f" | "0" | "no" | "n" | "off" => false,
            _ => {
                let value = value.to_owned();
                self.fail(
                    name,
                    &value,
                    "a boolean (true/false, t/f, 1/0, yes/no, y/n, on/off)",
                );
                default
            }
        }
    }

    fn threshold(&mut self, name: &'static str) -> SpamConfidence {
        let default = SpamConfidence::new(DEFAULT_SPAM_THRESHOLD).expect("default within 0..=100");
        let Some(value) = self.get(name) else {
            return default;
        };
        let parsed = value
            .parse::<i64>()
            .ok()
            .and_then(|n| u8::try_from(n).ok())
            .and_then(SpamConfidence::new);
        match parsed {
            Some(threshold) => threshold,
            None => {
                let value = value.to_owned();
                self.fail(name, &value, "an integer from 0 to 100");
                default
            }
        }
    }

    /// A comma-separated list of tag keys: items trimmed and lower-cased,
    /// empty items dropped, duplicates collapsed.
    fn tag_keys(&mut self, name: &'static str) -> BTreeSet<TagKey> {
        let Some(value) = self.get(name) else {
            return BTreeSet::new();
        };
        let mut keys = BTreeSet::new();
        let mut invalid = Vec::new();
        for item in value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            match TagKey::from_stored(&item.to_lowercase()) {
                Ok(key) => {
                    keys.insert(key);
                }
                Err(_) => invalid.push(item.to_owned()),
            }
        }
        for item in invalid {
            self.fail(
                name,
                &item,
                "a tag key (lower-case letters and digits, parts joined by '-')",
            );
        }
        keys
    }

    /// A socket address with a port, such as `127.0.0.1:8000` or `[::1]:8000`.
    fn socket_addr(&mut self, name: &'static str, default: &str) -> SocketAddr {
        let fallback: SocketAddr = default.parse().expect("default is a socket address");
        let Some(value) = self.get(name) else {
            return fallback;
        };
        if let Ok(addr) = value.parse() {
            return addr;
        }
        let value = value.to_owned();
        self.fail(
            name,
            &value,
            "a socket address with a port, such as 127.0.0.1:8000 or [::1]:8000",
        );
        fallback
    }

    /// A directory holding the built web client, with an `index.html`.
    fn client_dir(&mut self, name: &'static str) -> Option<PathBuf> {
        let value = self.get(name)?.to_owned();
        let path = PathBuf::from(&value);
        let problem = match std::fs::metadata(&path) {
            Err(_) => "does not exist",
            Ok(meta) if !meta.is_dir() => "is not a directory",
            Ok(_) if !path.join("index.html").is_file() => "has no index.html",
            Ok(_) => return Some(path),
        };
        self.errors.push(ConfigError {
            variable: name,
            message: format!("{value:?} {problem}"),
        });
        None
    }

    fn log_filter(&mut self, name: &'static str) -> String {
        let Some(value) = self.get(name) else {
            return DEFAULT_LOG_FILTER.to_owned();
        };
        let value = value.to_owned();
        if let Err(err) = EnvFilter::try_new(&value) {
            self.fail(name, &value, &format!("a valid log filter ({err})"));
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::path::Path;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn errors(pairs: &[(&str, &str)]) -> Vec<ConfigError> {
        Config::from_vars(&vars(pairs))
            .expect_err("expected configuration errors")
            .0
    }

    fn assert_defaults(config: &Config) {
        assert_eq!(config.data_dir, Path::new("./data"));
        assert_eq!(
            config.store.database,
            Path::new("./data/unified-mailbox.db")
        );
        assert_eq!(config.store.key_file, Path::new("./data/secret.key"));
        assert!(config.dry_run);
        assert_eq!(config.spam.threshold.get(), 90);
        assert!(config.spam.protect_known_senders);
        assert!(config.spam.archive.is_empty());
        assert_eq!(config.log_filter, "info");
        assert_eq!(config.listen_addr, "127.0.0.1:8000".parse().unwrap());
        assert_eq!(config.client_dir, None);
    }

    #[test]
    fn client_dir_unset_or_empty_is_none() {
        assert_eq!(Config::from_vars(&vars(&[])).unwrap().client_dir, None);
        let config = Config::from_vars(&vars(&[("CLIENT_DIR", "  ")])).unwrap();
        assert_eq!(config.client_dir, None);
    }

    #[test]
    fn client_dir_valid() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html></html>").unwrap();
        let value = format!("  {}  ", dir.path().display());
        let config = Config::from_vars(&vars(&[("CLIENT_DIR", &value)])).unwrap();
        assert_eq!(config.client_dir.as_deref(), Some(dir.path()));
    }

    fn assert_client_dir_error(path: &Path, problem: &str) {
        let value = path.display().to_string();
        let errs = errors(&[("CLIENT_DIR", &value)]);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].variable, "CLIENT_DIR");
        assert!(errs[0].message.contains(problem), "{}", errs[0]);
        assert!(errs[0].message.contains(&value), "{}", errs[0]);
    }

    #[test]
    fn client_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_client_dir_error(&dir.path().join("absent"), "does not exist");
    }

    #[test]
    fn client_dir_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("index.html");
        std::fs::write(&file, "").unwrap();
        assert_client_dir_error(&file, "is not a directory");
    }

    #[test]
    fn client_dir_without_index() {
        let dir = tempfile::tempdir().unwrap();
        assert_client_dir_error(dir.path(), "has no index.html");
    }

    #[test]
    fn listen_addr_parsed() {
        let config = Config::from_vars(&vars(&[("LISTEN_ADDR", " [::1]:9000 ")])).unwrap();
        assert_eq!(config.listen_addr, "[::1]:9000".parse().unwrap());
    }

    #[test]
    fn listen_addr_invalid() {
        for value in [
            "localhost",
            "127.0.0.1",
            "localhost:8000",
            "127.0.0.1:99999",
        ] {
            let errs = errors(&[("LISTEN_ADDR", value)]);
            assert_eq!(errs.len(), 1, "{value}");
            assert_eq!(errs[0].variable, "LISTEN_ADDR");
            assert!(errs[0].to_string().contains(value), "{}", errs[0]);
        }
    }

    #[test]
    fn defaults_when_unset() {
        assert_defaults(&Config::from_vars(&vars(&[])).unwrap());
    }

    #[test]
    fn defaults_when_empty() {
        let config = Config::from_vars(&vars(&[
            ("DATA_DIR", "  "),
            ("KEY_FILE", ""),
            ("DRY_RUN", " "),
            ("SPAM_THRESHOLD", ""),
            ("PROTECT_KNOWN_SENDERS", "  "),
            ("ARCHIVE_CATEGORIES", " "),
            ("RUST_LOG", "  "),
            ("LISTEN_ADDR", " "),
        ]))
        .unwrap();
        assert_defaults(&config);
    }

    #[test]
    fn unknown_variables_ignored() {
        let config = Config::from_vars(&vars(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/someone"),
            ("dry_run", "false"),
        ]))
        .unwrap();
        assert!(config.dry_run);
    }

    #[test]
    fn bool_forms() {
        for value in ["true", "TRUE", "1", "yes", "On", "t", "Y"] {
            let config = Config::from_vars(&vars(&[("DRY_RUN", value)])).unwrap();
            assert!(config.dry_run, "{value}");
        }
        for value in ["false", "0", "no", "OFF", "f", "N"] {
            let config = Config::from_vars(&vars(&[("DRY_RUN", value)])).unwrap();
            assert!(!config.dry_run, "{value}");
        }
        let errs = errors(&[("DRY_RUN", "maybe")]);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].variable, "DRY_RUN");
        assert!(errs[0].to_string().contains("maybe"));
    }

    #[test]
    fn protect_known_senders_parsed() {
        let config = Config::from_vars(&vars(&[("PROTECT_KNOWN_SENDERS", "no")])).unwrap();
        assert!(!config.spam.protect_known_senders);
        assert_eq!(
            errors(&[("PROTECT_KNOWN_SENDERS", "nope")])[0].variable,
            "PROTECT_KNOWN_SENDERS"
        );
    }

    #[test]
    fn threshold_bounds() {
        for (value, expected) in [("0", 0), ("100", 100), (" 95 ", 95)] {
            let config = Config::from_vars(&vars(&[("SPAM_THRESHOLD", value)])).unwrap();
            assert_eq!(config.spam.threshold.get(), expected);
        }
        for value in [
            "-1",
            "101",
            "256",
            "-256",
            "99999999999999999999",
            "abc",
            "9.5",
        ] {
            let errs = errors(&[("SPAM_THRESHOLD", value)]);
            assert_eq!(errs.len(), 1, "{value}");
            assert_eq!(errs[0].variable, "SPAM_THRESHOLD");
            assert!(errs[0].to_string().contains(value), "{}", errs[0]);
        }
    }

    #[test]
    fn archive_categories_normalised() {
        let config = Config::from_vars(&vars(&[(
            "ARCHIVE_CATEGORIES",
            " Newsletter , promotion,,newsletter ",
        )]))
        .unwrap();
        let keys: BTreeSet<&str> = config.spam.archive.iter().map(|k| k.as_str()).collect();
        assert_eq!(keys, BTreeSet::from(["newsletter", "promotion"]));
        for value in ["news letter", "news_letter"] {
            let errs = errors(&[("ARCHIVE_CATEGORIES", value)]);
            assert_eq!(errs.len(), 1, "{value}");
            assert_eq!(errs[0].variable, "ARCHIVE_CATEGORIES");
        }
    }

    #[test]
    fn archive_categories_each_bad_item_reported() {
        let errs = errors(&[("ARCHIVE_CATEGORIES", "newsletter,a b,c_d")]);
        assert_eq!(errs.len(), 2);
        assert!(errs[0].to_string().contains("a b"), "{}", errs[0]);
        assert!(errs[1].to_string().contains("c_d"), "{}", errs[1]);
    }

    #[test]
    fn key_file() {
        let config = Config::from_vars(&vars(&[("DATA_DIR", "/srv/um")])).unwrap();
        assert_eq!(config.data_dir, Path::new("/srv/um"));
        assert_eq!(config.store.key_file, Path::new("/srv/um/secret.key"));
        assert_eq!(
            config.store.database,
            Path::new("/srv/um/unified-mailbox.db")
        );

        let config = Config::from_vars(&vars(&[
            ("DATA_DIR", "/srv/um"),
            ("KEY_FILE", "/keys/um.key"),
        ]))
        .unwrap();
        assert_eq!(config.store.key_file, Path::new("/keys/um.key"));
        assert_eq!(
            config.store.database,
            Path::new("/srv/um/unified-mailbox.db")
        );
    }

    #[test]
    fn invalid_log_filter() {
        let errs = errors(&[("RUST_LOG", "sqlite=loud")]);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].variable, "RUST_LOG");

        let config = Config::from_vars(&vars(&[("RUST_LOG", "sqlite=debug")])).unwrap();
        assert_eq!(config.log_filter, "sqlite=debug");
    }

    #[test]
    fn all_errors_reported() {
        let err = Config::from_vars(&vars(&[
            ("DRY_RUN", "maybe"),
            ("SPAM_THRESHOLD", "101"),
            ("ARCHIVE_CATEGORIES", "a b"),
        ]))
        .unwrap_err();
        let names: Vec<&str> = err.0.iter().map(|e| e.variable).collect();
        assert_eq!(names, ["DRY_RUN", "SPAM_THRESHOLD", "ARCHIVE_CATEGORIES"]);
        assert_eq!(err.to_string().lines().count(), 3);
    }
}
