//! In-memory [`EventBus`] over a tokio broadcast channel.
//!
//! Events are hints: one published with no subscriber is dropped, and a
//! subscriber that falls more than the capacity behind gets
//! [`BusMessage::Lagged`] and must resync.

use domain::Event;
use futures_core::stream::BoxStream;
use ports::{BusMessage, EventBus};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// Events kept for a slow subscriber before it lags.
pub const CAPACITY: usize = 256;

#[derive(Clone, Debug)]
pub struct MemoryBus {
    sender: broadcast::Sender<Event>,
}

impl MemoryBus {
    /// `capacity` must be at least 1.
    pub fn new(capacity: usize) -> MemoryBus {
        MemoryBus {
            sender: broadcast::channel(capacity).0,
        }
    }

    /// How many subscribers are connected.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl EventBus for MemoryBus {
    fn publish(&self, event: Event) {
        // No subscriber is not an error: events are hints.
        let _ = self.sender.send(event);
    }

    fn subscribe(&self) -> BoxStream<'static, BusMessage> {
        Box::pin(
            BroadcastStream::new(self.sender.subscribe()).map(|item| match item {
                Ok(event) => BusMessage::Event(event),
                Err(BroadcastStreamRecvError::Lagged(_)) => BusMessage::Lagged,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use domain::{DeviceId, Event};
    use futures_util::StreamExt;
    use ports::{BusMessage, EventBus};

    use super::*;

    fn revoked(id: i64) -> Event {
        Event::DeviceRevoked(DeviceId::new(id))
    }

    fn id_of(message: BusMessage) -> i64 {
        match message {
            BusMessage::Event(Event::DeviceRevoked(id)) => id.get(),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn publish_without_subscriber_is_fine() {
        let bus = MemoryBus::new(CAPACITY);
        bus.publish(revoked(1));
        assert_eq!(bus.receiver_count(), 0);
    }

    #[tokio::test]
    async fn two_subscribers_both_receive() {
        let bus = MemoryBus::new(CAPACITY);
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.publish(revoked(7));
        assert_eq!(id_of(a.next().await.unwrap()), 7);
        assert_eq!(id_of(b.next().await.unwrap()), 7);
    }

    #[tokio::test]
    async fn lagging_subscriber_gets_lagged_then_next_events() {
        let bus = MemoryBus::new(2);
        let mut stream = bus.subscribe();
        for id in 1..=3 {
            bus.publish(revoked(id));
        }
        assert!(matches!(stream.next().await.unwrap(), BusMessage::Lagged));
        assert_eq!(id_of(stream.next().await.unwrap()), 2);
        assert_eq!(id_of(stream.next().await.unwrap()), 3);
    }

    #[tokio::test]
    async fn dropping_the_stream_releases_the_receiver() {
        let bus = MemoryBus::new(CAPACITY);
        let stream = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);
        drop(stream);
        assert_eq!(bus.receiver_count(), 0);
    }
}
