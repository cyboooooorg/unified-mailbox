//! Mail metadata and where a mail sits in its mailbox.

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::account::{AccountId, EmailAddress};

/// The provider's own identifier of a mail. Outlook changes it on every
/// move; Gmail and IMAP keep it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderMailId(String);

impl ProviderMailId {
    pub fn new(id: impl Into<String>) -> Result<Self, MailError> {
        let id = id.into();
        if id.is_empty() {
            return Err(MailError::EmptyProviderId);
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The provider's identifier of a conversation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThreadId(String);

impl ThreadId {
    pub fn new(id: impl Into<String>) -> Result<Self, MailError> {
        let id = id.into();
        if id.is_empty() {
            return Err(MailError::EmptyThreadId);
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A user folder, as levels: `Clients/Acme` is `["Clients", "Acme"]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FolderPath(Vec<String>);

impl FolderPath {
    /// Parses levels separated by `/`; each level is trimmed and must not
    /// be empty.
    pub fn parse(raw: &str) -> Result<Self, MailError> {
        if raw.trim().is_empty() {
            return Err(MailError::EmptyFolderPath);
        }
        raw.split('/')
            .map(|level| match level.trim() {
                "" => Err(MailError::EmptyFolderLevel),
                level => Ok(level.to_owned()),
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    pub fn levels(&self) -> &[String] {
        &self.0
    }

    pub fn to_path_string(&self) -> String {
        self.0.join("/")
    }
}

/// Where a mail currently is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Location {
    Inbox,
    Archive,
    Junk,
    Trash,
    Sent,
    Folder(FolderPath),
}

/// What Unified Mailbox knows about a mail, without its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailMeta {
    pub account: AccountId,
    pub provider_id: ProviderMailId,
    pub thread: Option<ThreadId>,
    pub received_at: DateTime<Utc>,
    /// `None` when the From header holds no address that parses, which is
    /// common in spam: such a mail must still be sorted.
    pub sender: Option<EmailAddress>,
    pub sender_name: Option<String>,
    /// The recipient addresses that parse; group syntax such as
    /// `undisclosed-recipients:;` yields none.
    pub recipients: Vec<EmailAddress>,
    pub subject: String,
    /// The sender's domain passed DMARC. A sender is "known" only then.
    pub dmarc_passed: bool,
    /// `None` when the provider did not say.
    pub is_read: Option<bool>,
    /// A locked mail is never moved by a rule or a classification.
    pub locked: bool,
    pub location: Location,
}

/// Errors of the mail types. Variants are codes: the client translates
/// them.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MailError {
    #[error("empty_provider_id")]
    EmptyProviderId,
    #[error("empty_thread_id")]
    EmptyThreadId,
    #[error("empty_folder_path")]
    EmptyFolderPath,
    #[error("empty_folder_level")]
    EmptyFolderLevel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_path_trims_levels() {
        let p = FolderPath::parse(" Clients / Acme ").unwrap();
        assert_eq!(p.levels(), ["Clients", "Acme"]);
        assert_eq!(p.to_path_string(), "Clients/Acme");
    }

    #[test]
    fn folder_path_rejects_empty() {
        assert_eq!(FolderPath::parse(""), Err(MailError::EmptyFolderPath));
        assert_eq!(FolderPath::parse("a//b"), Err(MailError::EmptyFolderLevel));
        assert_eq!(FolderPath::parse("a/ "), Err(MailError::EmptyFolderLevel));
    }

    #[test]
    fn mail_without_a_parsable_sender_is_representable() {
        let mail = MailMeta {
            account: AccountId::new(1),
            provider_id: ProviderMailId::new("m1").unwrap(),
            thread: None,
            received_at: DateTime::<Utc>::UNIX_EPOCH,
            sender: None,
            sender_name: Some("Totally Legit Bank".to_owned()),
            recipients: Vec::new(),
            subject: "You won".to_owned(),
            dmarc_passed: false,
            is_read: None,
            locked: false,
            location: Location::Inbox,
        };
        assert_eq!(mail.sender, None);
    }

    #[test]
    fn ids_reject_empty() {
        assert_eq!(ProviderMailId::new(""), Err(MailError::EmptyProviderId));
        assert_eq!(ThreadId::new(""), Err(MailError::EmptyThreadId));
    }
}
