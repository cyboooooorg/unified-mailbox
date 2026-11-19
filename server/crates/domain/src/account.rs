//! Mail accounts and the providers that host them.

use thiserror::Error;

/// Identifier of a mail account in Unified Mailbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(i64);

impl AccountId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

/// The kind of mailbox an account connects to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Gmail,
    Outlook,
    /// Any IMAP mailbox, Proton Mail through Proton Mail Bridge included.
    Imap,
}

impl ProviderKind {
    /// Stable code, used in storage and in the API.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Outlook => "outlook",
            Self::Imap => "imap",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "gmail" => Some(Self::Gmail),
            "outlook" => Some(Self::Outlook),
            "imap" => Some(Self::Imap),
            _ => None,
        }
    }
}

/// A mail address, trimmed and lowercased.
///
/// Validation is deliberately loose: one `@`, a non-empty local part and
/// domain, no whitespace. The provider is the authority on whether the
/// address can receive mail.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmailAddress(String);

impl EmailAddress {
    pub fn parse(raw: &str) -> Result<Self, AccountError> {
        let address = raw.trim().to_lowercase();
        let valid = match address.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty()
                    && !domain.is_empty()
                    && !domain.contains('@')
                    && !address.chars().any(char::is_whitespace)
            }
            None => false,
        };
        if valid {
            Ok(Self(address))
        } else {
            Err(AccountError::InvalidEmail)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The part after the `@`.
    pub fn domain(&self) -> &str {
        self.0.split_once('@').map_or("", |(_, domain)| domain)
    }
}

/// Errors of the account types. Variants are codes: the client
/// translates them.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AccountError {
    #[error("invalid_email")]
    InvalidEmail,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_trimmed_and_lowercased() {
        assert_eq!(
            EmailAddress::parse("  Alice@Example.COM ")
                .unwrap()
                .as_str(),
            "alice@example.com"
        );
    }

    #[test]
    fn email_domain() {
        assert_eq!(
            EmailAddress::parse("a@mail.example.org").unwrap().domain(),
            "mail.example.org"
        );
    }

    #[test]
    fn email_rejects_invalid() {
        for bad in [
            "",
            "alice",
            "@example.com",
            "alice@",
            "a@b@c.com",
            "a b@example.com",
        ] {
            assert_eq!(
                EmailAddress::parse(bad),
                Err(AccountError::InvalidEmail),
                "{bad}"
            );
        }
    }

    #[test]
    fn provider_codes_round_trip() {
        for p in [
            ProviderKind::Gmail,
            ProviderKind::Outlook,
            ProviderKind::Imap,
        ] {
            assert_eq!(ProviderKind::from_code(p.as_code()), Some(p));
        }
        assert_eq!(ProviderKind::from_code("yahoo"), None);
    }
}
