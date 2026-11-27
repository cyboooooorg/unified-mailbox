//! Settings use cases: the language is one value for the whole server,
//! shared by all devices.

mod effective_language;
mod get_language;
mod set_language;

pub use effective_language::*;
pub use get_language::*;
pub use set_language::*;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};
    use domain::{Event, Language};

    use super::*;
    use crate::Deps;
    use crate::testing::{
        FixedClock, InMemoryDevices, InMemoryJobs, InMemorySettings, RecordingBus, SeqSecrets,
    };

    struct Fixture {
        deps: Deps,
        settings: Arc<InMemorySettings>,
        bus: Arc<RecordingBus>,
    }

    fn fixture() -> Fixture {
        let settings = Arc::new(InMemorySettings::new());
        let bus = Arc::new(RecordingBus::new());
        let deps = Deps {
            repo: Arc::new(InMemoryDevices::new()),
            clock: Arc::new(FixedClock::new(
                Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            )),
            secrets: Arc::new(SeqSecrets::new()),
            jobs: Arc::new(InMemoryJobs::new()),
            events: bus.clone(),
            settings: settings.clone(),
            job_ends: Arc::new(crate::UnsavedJobEnds::new()),
        };
        Fixture {
            deps,
            settings,
            bus,
        }
    }

    impl Fixture {
        async fn set(&self, language: Language) {
            SetLanguage::new(self.deps.clone())
                .execute(language)
                .await
                .unwrap();
        }

        async fn get(&self) -> Option<Language> {
            GetLanguage::new(self.deps.clone()).execute().await.unwrap()
        }

        async fn effective(&self) -> Language {
            EffectiveLanguage::new(self.deps.clone())
                .execute()
                .await
                .unwrap()
        }
    }

    #[tokio::test]
    async fn get_returns_none_when_unset() {
        assert_eq!(fixture().get().await, None);
    }

    #[tokio::test]
    async fn set_stores_and_publishes() {
        let f = fixture();
        f.set(Language::Fr).await;
        assert_eq!(f.get().await, Some(Language::Fr));
        assert!(matches!(
            f.bus.events().as_slice(),
            [Event::SettingsChanged]
        ));
    }

    #[tokio::test]
    async fn set_same_value_publishes_nothing() {
        let f = fixture();
        f.set(Language::Fr).await;
        f.set(Language::Fr).await;
        assert_eq!(f.bus.events().len(), 1);
        assert_eq!(f.settings.set_count(), 1);
    }

    #[tokio::test]
    async fn effective_falls_back_to_default() {
        assert_eq!(fixture().effective().await, Language::En);
    }

    #[tokio::test]
    async fn effective_returns_the_stored_language() {
        let f = fixture();
        f.set(Language::Fr).await;
        assert_eq!(f.effective().await, Language::Fr);
    }

    #[tokio::test]
    async fn set_overwrites_when_the_read_fails() {
        let f = fixture();
        f.settings.fail_reads();
        SetLanguage::new(f.deps.clone())
            .execute(Language::Fr)
            .await
            .unwrap();
        assert_eq!(f.settings.set_count(), 1);
        assert!(matches!(
            f.bus.events().as_slice(),
            [Event::SettingsChanged]
        ));
    }

    #[tokio::test]
    async fn set_fails_without_publishing_when_the_write_fails() {
        let f = fixture();
        f.settings.fail_writes();
        let result = SetLanguage::new(f.deps.clone()).execute(Language::Fr).await;
        assert!(result.is_err());
        assert!(f.bus.events().is_empty());
        assert_eq!(f.settings.set_count(), 0);
    }
}
