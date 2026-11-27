//! Tags: the categories Claude sorts mails into, with a stable key.

use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

/// Keys of the tags created on first start. The client shows a translated
/// name for each until the user renames it.
pub const DEFAULT_TAG_KEYS: [&str; 8] = [
    "important",
    "personal",
    "work",
    "transaction",
    "notification",
    "newsletter",
    "promotion",
    "spam",
];

/// Key of the spam tag, which can be renamed but never deleted.
const SPAM_KEY: &str = "spam";

/// The stable identifier of a tag: lowercase ASCII words joined by `-`.
///
/// It is derived from the name once, then never changes: renaming a tag
/// keeps its key, so stored classifications and provider labels stay
/// valid.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TagKey(String);

impl TagKey {
    /// Reserved for `Category::Unknown` (the classifier refused the
    /// mail); never a tag.
    pub const RESERVED: &str = "unknown";
    pub const MAX_LEN: usize = 30;

    /// Derives a new key from a tag name.
    ///
    /// Accents are stripped (NFKD), other non-ASCII characters dropped,
    /// runs of anything but `[a-z0-9]` become `-`, and the result is cut
    /// to 30 characters. An empty result becomes `tag`. When the key is
    /// reserved or `taken` says it is used, `-2`, `-3`… is appended.
    pub fn derive(name: &str, taken: &impl Fn(&str) -> bool) -> TagKey {
        let mut base = truncate(&slug(name), Self::MAX_LEN).to_owned();
        if base.is_empty() {
            base = "tag".to_owned();
        }
        if base != Self::RESERVED && !taken(&base) {
            return Self(base);
        }
        (2u32..)
            .map(|n| {
                let suffix = format!("-{n}");
                let stem = truncate(&base, Self::MAX_LEN - suffix.len());
                format!("{stem}{suffix}")
            })
            .find(|candidate| !taken(candidate))
            .map(Self)
            .expect("an unused suffix exists")
    }

    /// Rebuilds a key read from storage.
    pub fn from_stored(key: &str) -> Result<Self, TagError> {
        let well_formed = key.len() <= Self::MAX_LEN
            && key.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            });
        if !well_formed {
            return Err(TagError::InvalidKey);
        }
        if key == Self::RESERVED {
            return Err(TagError::ReservedKey);
        }
        Ok(Self(key.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lowercase ASCII, runs of other characters replaced by `-`, no `-` at
/// either end.
fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in name.nfkd().filter(char::is_ascii) {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c);
        } else {
            pending_dash = true;
        }
    }
    out
}

/// Cuts an ASCII slug to `max` characters, without a trailing `-`.
fn truncate(slug: &str, max: usize) -> &str {
    slug[..slug.len().min(max)].trim_end_matches('-')
}

/// The name shown for a tag: trimmed, 1 to 40 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagName(String);

impl TagName {
    pub const MAX_LEN: usize = 40;

    pub fn parse(raw: &str) -> Result<Self, TagError> {
        let name = raw.trim();
        match name.chars().count() {
            0 => Err(TagError::NameEmpty),
            n if n > Self::MAX_LEN => Err(TagError::NameTooLong { max: Self::MAX_LEN }),
            _ => Ok(Self(name.to_owned())),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Two names clash when they are equal ignoring case. Accents count:
    /// `Été` and `Ete` are different names.
    pub fn same_as(&self, other: &TagName) -> bool {
        self.0.to_lowercase() == other.0.to_lowercase()
    }
}

/// What a tag is for, given to Claude: trimmed, 1 to 500 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagDescription(String);

impl TagDescription {
    pub const MAX_LEN: usize = 500;

    pub fn parse(raw: &str) -> Result<Self, TagError> {
        let description = raw.trim();
        match description.chars().count() {
            0 => Err(TagError::DescriptionEmpty),
            n if n > Self::MAX_LEN => Err(TagError::DescriptionTooLong { max: Self::MAX_LEN }),
            _ => Ok(Self(description.to_owned())),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A tag colour, `#rrggbb`, stored lowercase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Color(String);

impl Color {
    pub fn parse(raw: &str) -> Result<Self, TagError> {
        match raw.strip_prefix('#') {
            Some(hex) if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                Ok(Self(raw.to_ascii_lowercase()))
            }
            _ => Err(TagError::InvalidColor),
        }
    }

    /// The colour of a tag created without one.
    pub fn default_color() -> Self {
        Self("#868e96".to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub key: TagKey,
    pub name: TagName,
    pub description: TagDescription,
    pub color: Color,
}

impl Tag {
    /// The spam tag can be renamed but not deleted: spam handling relies
    /// on it.
    pub fn can_delete(&self) -> bool {
        self.key.as_str() != SPAM_KEY
    }
}

/// Errors of the tag types. Variants are codes with fields: the client
/// translates them.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TagError {
    #[error("tag_name_empty")]
    NameEmpty,
    #[error("tag_name_too_long")]
    NameTooLong { max: usize },
    #[error("tag_description_empty")]
    DescriptionEmpty,
    #[error("tag_description_too_long")]
    DescriptionTooLong { max: usize },
    #[error("invalid_color")]
    InvalidColor,
    #[error("invalid_tag_key")]
    InvalidKey,
    #[error("reserved_tag_key")]
    ReservedKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> bool {
        false
    }

    fn tag(key: &str) -> Tag {
        Tag {
            key: TagKey::from_stored(key).unwrap(),
            name: TagName::parse(key).unwrap(),
            description: TagDescription::parse("A tag").unwrap(),
            color: Color::default_color(),
        }
    }

    #[test]
    fn key_slugs_accents_and_punctuation() {
        assert_eq!(
            TagKey::derive("Équipe & Clients!", &none).as_str(),
            "equipe-clients"
        );
    }

    #[test]
    fn key_falls_back_to_tag() {
        assert_eq!(TagKey::derive("日本", &none).as_str(), "tag");
        assert_eq!(TagKey::derive("🎉", &|k| k == "tag").as_str(), "tag-2");
    }

    #[test]
    fn key_appends_suffix_on_collision() {
        let taken = |k: &str| k == "work" || k == "work-2";
        assert_eq!(TagKey::derive("Work", &taken).as_str(), "work-3");
    }

    #[test]
    fn key_never_reserved() {
        assert_eq!(TagKey::derive("Unknown!", &none).as_str(), "unknown-2");
    }

    #[test]
    fn key_truncates_without_trailing_dash() {
        let k = TagKey::derive("abcdefghijklmnopqrstuvwxyz abcd efgh", &none);
        assert!(k.as_str().len() <= 30 && !k.as_str().ends_with('-'));
        let full = "a".repeat(30);
        let k2 = TagKey::derive(&"a".repeat(40), &|k| k == full);
        assert_eq!(k2.as_str(), format!("{}-2", "a".repeat(28)));
    }

    #[test]
    fn key_suffix_never_leaves_a_dash_before_it() {
        // Truncating "ab…-c" for the suffix must not produce "ab…--2".
        let name = format!("{} c", "b".repeat(28));
        let base = TagKey::derive(&name, &none);
        assert_eq!(base.as_str(), format!("{}-c", "b".repeat(28)));
        let next = TagKey::derive(&name, &|k| k == base.as_str());
        assert_eq!(next.as_str(), format!("{}-2", "b".repeat(28)));
    }

    #[test]
    fn key_cut_landing_on_a_dash_is_trimmed() {
        // Cut at 30 keeps "a…a-": the trailing dash must go.
        let name = format!("{} x", "a".repeat(29));
        assert_eq!(TagKey::derive(&name, &none).as_str(), "a".repeat(29));
    }

    #[test]
    fn key_cut_for_suffix_landing_on_a_dash_is_trimmed() {
        // Cut at 28 for "-2" keeps "b…b-": the dash must go before the suffix.
        let name = format!("{} cc", "b".repeat(27));
        let base = TagKey::derive(&name, &none);
        assert_eq!(base.as_str(), format!("{}-cc", "b".repeat(27)));
        let next = TagKey::derive(&name, &|k| k == base.as_str());
        assert_eq!(next.as_str(), format!("{}-2", "b".repeat(27)));
    }

    #[test]
    fn stored_key_rejects_reserved_and_bad() {
        assert_eq!(TagKey::from_stored("unknown"), Err(TagError::ReservedKey));
        assert_eq!(TagKey::from_stored("Bad Key"), Err(TagError::InvalidKey));
        assert_eq!(TagKey::from_stored("a-"), Err(TagError::InvalidKey));
        assert_eq!(
            TagKey::from_stored(&"a".repeat(31)),
            Err(TagError::InvalidKey)
        );
        assert!(TagKey::from_stored("work-2").is_ok());
    }

    #[test]
    fn name_rules() {
        assert_eq!(TagName::parse("  ").unwrap_err(), TagError::NameEmpty);
        assert_eq!(
            TagName::parse(&"x".repeat(41)).unwrap_err(),
            TagError::NameTooLong { max: 40 }
        );
        assert_eq!(
            TagName::parse(&"é".repeat(40))
                .unwrap()
                .as_str()
                .chars()
                .count(),
            40
        );
        assert!(
            TagName::parse("Travail")
                .unwrap()
                .same_as(&TagName::parse("TRAVAIL").unwrap())
        );
        assert!(
            !TagName::parse("Été")
                .unwrap()
                .same_as(&TagName::parse("Ete").unwrap())
        );
    }

    #[test]
    fn description_rules() {
        assert_eq!(
            TagDescription::parse(" ").unwrap_err(),
            TagError::DescriptionEmpty
        );
        assert_eq!(
            TagDescription::parse(&"x".repeat(501)).unwrap_err(),
            TagError::DescriptionTooLong { max: 500 }
        );
    }

    #[test]
    fn color_is_lowercased() {
        assert_eq!(Color::parse("#AABBCC").unwrap().as_str(), "#aabbcc");
        assert_eq!(Color::parse("aabbcc").unwrap_err(), TagError::InvalidColor);
        assert_eq!(Color::parse("#abcdeg").unwrap_err(), TagError::InvalidColor);
        assert_eq!(Color::default_color().as_str(), "#868e96");
    }

    #[test]
    fn spam_cannot_be_deleted() {
        assert!(!tag("spam").can_delete());
        assert!(tag("work").can_delete());
    }

    #[test]
    fn default_keys_are_valid_stored_keys() {
        for key in DEFAULT_TAG_KEYS {
            assert!(TagKey::from_stored(key).is_ok(), "{key}");
        }
    }
}
