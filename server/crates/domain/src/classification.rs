//! What Claude says about a mail.
//!
//! The classification is data: the code decides every action from it
//! (see `decision`), the model never triggers one.

use crate::tag::TagKey;

/// The category stored for a mail: the tag of its classification, or
/// `Unknown` when the classifier refused to classify it. A
/// [`Classification`] always names a tag.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Tag(TagKey),
    Unknown,
}

impl Category {
    pub fn is_tag(&self, key: &str) -> bool {
        matches!(self, Self::Tag(tag) if tag.as_str() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Priority {
    High,
    Normal,
    Low,
}

impl Priority {
    /// Stable code, used in storage and in the API.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "high" => Some(Self::High),
            "normal" => Some(Self::Normal),
            "low" => Some(Self::Low),
            _ => None,
        }
    }
}

/// How sure Claude is that a mail is spam, from 0 to 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpamConfidence(u8);

impl SpamConfidence {
    pub const MAX: u8 = 100;

    /// Brings any integer into 0..=100: the model may answer outside it.
    pub fn clamped(value: i64) -> Self {
        // The clamp keeps the value within u8.
        Self(value.clamp(0, i64::from(Self::MAX)) as u8)
    }

    pub fn new(value: u8) -> Option<Self> {
        (value <= Self::MAX).then_some(Self(value))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

/// Claude's verdict on one mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classification {
    pub category: TagKey,
    pub priority: Priority,
    pub needs_reply: bool,
    pub is_phishing: bool,
    pub spam_confidence: SpamConfidence,
    /// One line, written by Claude in the user's language.
    pub summary: String,
    /// Why Claude chose the category, in the user's language.
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_is_clamped() {
        assert_eq!(SpamConfidence::clamped(-5).get(), 0);
        assert_eq!(SpamConfidence::clamped(250).get(), 100);
        assert_eq!(SpamConfidence::clamped(42).get(), 42);
        assert_eq!(SpamConfidence::new(101), None);
        assert_eq!(SpamConfidence::new(100).map(SpamConfidence::get), Some(100));
    }

    #[test]
    fn priority_codes_round_trip() {
        for p in [Priority::High, Priority::Normal, Priority::Low] {
            assert_eq!(Priority::from_code(p.as_code()), Some(p));
        }
        assert_eq!(Priority::from_code("haute"), None);
    }

    #[test]
    fn category_matches_tag_key() {
        let c = Category::Tag(TagKey::from_stored("spam").unwrap());
        assert!(c.is_tag("spam") && !c.is_tag("work"));
        assert!(!Category::Unknown.is_tag("spam"));
    }
}
