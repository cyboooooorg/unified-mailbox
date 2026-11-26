//! Device use cases: a paired device holds a bearer token, stored only as
//! its hash; a new device pairs with a one-time code.

use crate::Deps;

mod authenticate_device;
mod create_pairing_code;
mod device_is_active;
mod list_devices;
mod pair_device;
mod revoke_device;
mod token;

pub use authenticate_device::*;
pub use create_pairing_code::*;
pub use device_is_active::*;
pub use list_devices::*;
pub use pair_device::*;
pub use revoke_device::*;
pub use token::*;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeDelta, TimeZone, Utc};
    use domain::{DeviceId, DeviceNameError};

    use super::*;
    use crate::testing::{FixedClock, InMemoryDevices, InMemoryJobs, RecordingBus, SeqSecrets};

    struct Fixture {
        deps: Deps,
        repo: Arc<InMemoryDevices>,
        clock: Arc<FixedClock>,
        bus: Arc<RecordingBus>,
    }

    fn fixture() -> Fixture {
        let repo = Arc::new(InMemoryDevices::new());
        let clock = Arc::new(FixedClock::new(
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        ));
        let bus = Arc::new(RecordingBus::new());
        let deps = Deps {
            repo: repo.clone(),
            clock: clock.clone(),
            secrets: Arc::new(SeqSecrets::new()),
            jobs: Arc::new(InMemoryJobs::new()),
            events: bus.clone(),
        };
        Fixture {
            deps,
            repo,
            clock,
            bus,
        }
    }

    impl Fixture {
        async fn code(&self) -> String {
            let created = CreatePairingCode::new(self.deps.clone())
                .execute(None)
                .await
                .unwrap();
            created.code.to_string().to_lowercase()
        }

        async fn pair(&self, code: &str, name: &str) -> Result<PairedDevice, PairError> {
            PairDevice::new(self.deps.clone()).execute(code, name).await
        }

        async fn pair_new(&self, name: &str) -> PairedDevice {
            let code = self.code().await;
            self.pair(&code, name).await.unwrap()
        }

        async fn authenticate(&self, token: &str) -> Option<DeviceId> {
            AuthenticateDevice::new(self.deps.clone())
                .execute(token)
                .await
                .unwrap()
        }

        async fn revoke(&self, id: DeviceId) -> bool {
            RevokeDevice::new(self.deps.clone())
                .execute(id)
                .await
                .unwrap()
        }
    }

    #[tokio::test]
    async fn pair_then_authenticate() {
        let f = fixture();
        let paired = f.pair_new("Phone").await;
        assert_eq!(paired.device.name.as_str(), "Phone");
        assert_eq!(
            f.authenticate(paired.token.as_str()).await,
            Some(paired.device.id)
        );
    }

    #[tokio::test]
    async fn created_code_expires_after_ttl() {
        let f = fixture();
        let created = CreatePairingCode::new(f.deps.clone())
            .execute(None)
            .await
            .unwrap();
        assert_eq!(
            created.expires_at,
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 10, 0).unwrap()
        );
    }

    #[tokio::test]
    async fn token_shape() {
        let f = fixture();
        let token = f.pair_new("Phone").await.token;
        assert!(token.as_str().starts_with("um_"));
        assert_eq!(token.as_str().len(), 46);
        assert!(looks_like_token(token.as_str()));
        assert!(!looks_like_token("um_short"));
    }

    #[test]
    fn looks_like_token_rejects_wrong_prefix_or_alphabet() {
        let body = "A".repeat(43);
        assert!(looks_like_token(&format!("um_{body}")));
        assert!(!looks_like_token(&format!("xx_{body}")));
        assert!(!looks_like_token(&format!("um_{}+", "A".repeat(42))));
        assert!(!looks_like_token(&format!("um_{body}A")));
    }

    #[test]
    fn hashes_are_lowercase_hex_sha256() {
        // SHA-256 of the empty string and of "abc", well-known vectors.
        assert_eq!(
            hash_token(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let code = domain::PairingCode::parse("04hm-asw9").unwrap();
        assert_eq!(hash_pairing_code(&code), hash_token("04HMASW9"));
    }

    #[tokio::test]
    async fn token_not_stored_in_clear() {
        let f = fixture();
        let token = f.pair_new("Phone").await.token;
        assert_eq!(
            f.repo.stored_token_hashes(),
            vec![hash_token(token.as_str())]
        );
        assert_eq!(format!("{token:?}"), "DeviceToken(..)");
    }

    #[tokio::test]
    async fn code_valid_until_ttl() {
        let f = fixture();
        let code = f.code().await;
        f.clock
            .advance(TimeDelta::minutes(9) + TimeDelta::seconds(59));
        assert!(f.pair(&code, "Phone").await.is_ok());

        let code = f.code().await;
        f.clock.advance(TimeDelta::minutes(10));
        assert!(matches!(
            f.pair(&code, "Phone").await,
            Err(PairError::InvalidCode)
        ));
    }

    #[tokio::test]
    async fn code_single_use() {
        let f = fixture();
        let code = f.code().await;
        f.pair(&code, "Phone").await.unwrap();
        assert!(matches!(
            f.pair(&code, "Tablet").await,
            Err(PairError::InvalidCode)
        ));
    }

    #[tokio::test]
    async fn unknown_and_malformed_code() {
        let f = fixture();
        f.code().await;
        for code in ["0000-0000", "nope"] {
            assert!(
                matches!(f.pair(code, "Phone").await, Err(PairError::InvalidCode)),
                "{code}"
            );
        }
    }

    #[tokio::test]
    async fn invalid_name_wins() {
        let f = fixture();
        assert!(matches!(
            f.pair("nope", "  ").await,
            Err(PairError::InvalidName(DeviceNameError::Empty))
        ));
    }

    #[tokio::test]
    async fn revoked_device_does_not_authenticate() {
        let f = fixture();
        let paired = f.pair_new("Phone").await;
        assert!(f.revoke(paired.device.id).await);
        assert_eq!(f.authenticate(paired.token.as_str()).await, None);
        assert!(!f.revoke(paired.device.id).await);
        assert!(!f.revoke(DeviceId::new(999)).await);
    }

    #[tokio::test]
    async fn revoke_publishes_device_revoked() {
        let f = fixture();
        let paired = f.pair_new("Phone").await;
        f.revoke(paired.device.id).await;
        assert!(matches!(
            f.bus.events().as_slice(),
            [domain::Event::DeviceRevoked(id)] if *id == paired.device.id
        ));
    }

    #[tokio::test]
    async fn revoke_unknown_publishes_nothing() {
        let f = fixture();
        assert!(!f.revoke(DeviceId::new(999)).await);
        assert!(f.bus.events().is_empty());
    }

    #[tokio::test]
    async fn last_seen_throttled() {
        let f = fixture();
        let token = f.pair_new("Phone").await.token;
        f.authenticate(token.as_str()).await;
        f.authenticate(token.as_str()).await;
        assert_eq!(f.repo.touch_count(), 1);
        f.clock.advance(TimeDelta::seconds(59));
        f.authenticate(token.as_str()).await;
        assert_eq!(f.repo.touch_count(), 1);
        f.clock.advance(TimeDelta::seconds(1));
        f.authenticate(token.as_str()).await;
        assert_eq!(f.repo.touch_count(), 2);
    }

    #[tokio::test]
    async fn failed_last_seen_write_does_not_fail_authentication() {
        let f = fixture();
        let paired = f.pair_new("Phone").await;
        f.repo.fail_touch(true);
        assert_eq!(
            f.authenticate(paired.token.as_str()).await,
            Some(paired.device.id)
        );
        assert_eq!(f.repo.touch_count(), 1);
    }

    #[tokio::test]
    async fn list_returns_active_only() {
        let f = fixture();
        let first = f.pair_new("Phone").await;
        let second = f.pair_new("Tablet").await;
        f.revoke(first.device.id).await;
        let listed = ListDevices::new(f.deps.clone()).execute().await.unwrap();
        let ids: Vec<_> = listed.iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![second.device.id]);
    }

    #[tokio::test]
    async fn device_is_active_only_while_paired_and_not_revoked() {
        let f = fixture();
        let first = f.pair_new("Phone").await;
        let second = f.pair_new("Tablet").await;
        f.revoke(first.device.id).await;
        let is_active = DeviceIsActive::new(f.deps.clone());
        assert!(is_active.execute(second.device.id).await.unwrap());
        assert!(!is_active.execute(first.device.id).await.unwrap());
        assert!(!is_active.execute(DeviceId::new(999)).await.unwrap());
        f.repo.fail_reads(true);
        assert!(is_active.execute(second.device.id).await.is_err());
    }

    #[tokio::test]
    async fn malformed_token_skips_lookup() {
        let f = fixture();
        f.pair_new("Phone").await;
        assert_eq!(f.authenticate("Bearer x").await, None);
        assert_eq!(f.repo.touch_count(), 0);
        assert_eq!(f.repo.lookup_count(), 0);
    }

    #[test]
    fn pair_error_messages_hide_secrets() {
        assert_eq!(PairError::InvalidCode.to_string(), "invalid_pairing_code");
    }
}
