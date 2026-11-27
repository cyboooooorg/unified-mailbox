use domain::Language;
use ports::RepositoryError;

use crate::Deps;

/// The language in use: the one chosen, else [`Language::DEFAULT`].
pub struct EffectiveLanguage(Deps);

impl EffectiveLanguage {
    pub fn new(deps: Deps) -> Self {
        EffectiveLanguage(deps)
    }

    pub async fn execute(&self) -> Result<Language, RepositoryError> {
        Ok(self
            .0
            .settings
            .language()
            .await?
            .unwrap_or(Language::DEFAULT))
    }
}
