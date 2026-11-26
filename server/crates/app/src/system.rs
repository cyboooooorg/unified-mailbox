//! The real clock and the operating system's randomness.

use chrono::{DateTime, Utc};
use ports::{Clock, SecretGenerator};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct OsSecrets;

impl SecretGenerator for OsSecrets {
    /// Panics when the OS has no entropy: no token may be issued then.
    fn fill(&self, buf: &mut [u8]) {
        getrandom::fill(buf).expect("the OS random number generator failed");
    }
}
