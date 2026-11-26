//! The bus that carries events to the connected clients.

use domain::Event;
use futures_core::stream::BoxStream;

/// What a subscriber receives.
#[derive(Clone, Debug)]
pub enum BusMessage {
    Event(Event),
    /// The subscriber was too slow and missed events: it must resync.
    Lagged,
}

/// A broadcast of events: publishing never blocks and never fails, a
/// subscriber that falls behind gets [`BusMessage::Lagged`].
pub trait EventBus: Send + Sync {
    fn publish(&self, event: Event);

    fn subscribe(&self) -> BoxStream<'static, BusMessage>;
}
