use futures_core::stream::BoxStream;
use ports::BusMessage;

use crate::Deps;

/// Subscribes to the events pushed to the clients.
pub struct SubscribeEvents(Deps);

impl SubscribeEvents {
    pub fn new(deps: Deps) -> Self {
        SubscribeEvents(deps)
    }

    /// Events published from now on. [`BusMessage::Lagged`] means the
    /// subscriber missed some and must resync.
    pub fn execute(&self) -> BoxStream<'static, BusMessage> {
        self.0.events.subscribe()
    }
}
