//! Server-wide settings.

/// The language of the user: the interface and the text Claude writes
/// for them. Its code is stored and sent on the API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    Fr,
}

impl Language {
    /// Used until the user chooses one, and as the fallback.
    pub const DEFAULT: Language = Language::En;

    /// Every language, in the order the pickers show them.
    pub const ALL: [Language; 2] = [Language::En, Language::Fr];

    /// Stable code, used in storage and in the API.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }

    /// Exact codes only: no case folding, no region (`fr-FR` is unknown).
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::En),
            "fr" => Some(Self::Fr),
            _ => None,
        }
    }

    /// The name in English, for the prompt sent to Claude.
    pub fn english_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Fr => "French",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.as_code()), Some(language));
        }
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(Language::En.as_code(), "en");
        assert_eq!(Language::Fr.as_code(), "fr");
    }

    #[test]
    fn unknown_codes_are_rejected() {
        for code in ["de", "", "FR", "fr-FR"] {
            assert_eq!(Language::from_code(code), None, "{code:?}");
        }
    }

    #[test]
    fn default_is_english() {
        assert_eq!(Language::DEFAULT, Language::En);
    }

    #[test]
    fn english_names() {
        assert_eq!(Language::En.english_name(), "English");
        assert_eq!(Language::Fr.english_name(), "French");
    }
}
