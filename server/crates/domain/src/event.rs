//! Events the server pushes to the connected clients.

use crate::{DeviceId, Job};

/// Something that happened and that clients may want to know at once.
#[derive(Clone, Debug)]
pub enum Event {
    /// A job started, progressed or ended.
    Job(Job),
    /// A device was revoked: its streams must close.
    DeviceRevoked(DeviceId),
}
