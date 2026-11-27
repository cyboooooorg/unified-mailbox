//! Storage of the server-wide settings.

use chrono::{DateTime, Utc};
use domain::Language;

use crate::devices::RepositoryError;

/// Settings chosen by the user, shared by all their devices.
#[async_trait::async_trait]
pub trait SettingsRepository: Send + Sync {
    /// `None` until a language was set.
    async fn language(&self) -> Result<Option<Language>, RepositoryError>;

    /// Replaces the language; `at` is when it was chosen.
    async fn set_language(
        &self,
        language: Language,
        at: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;
}
