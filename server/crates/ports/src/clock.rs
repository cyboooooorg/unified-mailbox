//! The current time, injected so that use cases can be tested at a fixed
//! instant.

use chrono::{DateTime, Utc};

/// Gives the current time.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
