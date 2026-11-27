use domain::Language;
use ports::RepositoryError;

use crate::Deps;

/// Reads the language the user chose.
pub struct GetLanguage(Deps);

impl GetLanguage {
    pub fn new(deps: Deps) -> Self {
        GetLanguage(deps)
    }

    /// `None` until a language was set; see [`crate::EffectiveLanguage`]
    /// for the one in use.
    pub async fn execute(&self) -> Result<Option<Language>, RepositoryError> {
        self.0.settings.language().await
    }
}
