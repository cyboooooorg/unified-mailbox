//! The server configuration, read once at startup.
//!
//! `Config::from_vars` turns a map of variables (`.env` overlaid by the
//! process environment) into typed values for the crates that need them.
//! An unset variable, or one that is empty once trimmed, takes its
//! default. Every invalid variable is reported, not only the first.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::PathBuf;

use domain::{SpamConfidence, SpamPolicy, TagKey};
use sqlite::StoreConfig;
use tracing_subscriber::EnvFilter;

const DEFAULT_DATA_DIR: &str = "./data";
const DATABASE_FILE: &str = "unified-mailbox.db";
const KEY_FILE_NAME: &str = "secret.key";
const DEFAULT_SPAM_THRESHOLD: u8 = 90;
const DEFAULT_LOG_FILTER: &str = "info";

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
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => {
                let value = value.to_owned();
                self.fail(name, &value, "a boolean (true/false, 1/0, yes/no, on/off)");
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
        for value in ["true", "TRUE", "1", "yes", "On"] {
            let config = Config::from_vars(&vars(&[("DRY_RUN", value)])).unwrap();
            assert!(config.dry_run, "{value}");
        }
        for value in ["false", "0", "no", "OFF"] {
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
        for value in ["-1", "101", "abc", "9.5"] {
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
