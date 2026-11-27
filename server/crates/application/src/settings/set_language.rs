use domain::{Event, Language};
use ports::RepositoryError;

use crate::Deps;

/// Chooses the language, for all devices.
pub struct SetLanguage(Deps);

impl SetLanguage {
    pub fn new(deps: Deps) -> Self {
        SetLanguage(deps)
    }

    /// Publishes [`Event::SettingsChanged`] when the stored value changes
    /// (unset counts as different from any language). Setting the stored
    /// language again writes and publishes nothing. A stored value that
    /// cannot be read (for example an unknown code) is replaced; a failed
    /// write returns the error and publishes nothing.
    pub async fn execute(&self, language: Language) -> Result<(), RepositoryError> {
        if matches!(self.0.settings.language().await, Ok(Some(stored)) if stored == language) {
            return Ok(());
        }
        self.0
            .settings
            .set_language(language, self.0.clock.now())
            .await?;
        self.0.events.publish(Event::SettingsChanged);
        Ok(())
    }
}
