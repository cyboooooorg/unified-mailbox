# events-memory

The in-memory implementation of `ports::EventBus`: it carries the server's
events (job progress, device revocation) to the connected clients, over a
`tokio::sync::broadcast` channel.

## Prerequisites

None: no I/O, no configuration file, no environment variable.

## Usage

```rust
use events_memory::{CAPACITY, MemoryBus};
use ports::EventBus;

let bus = MemoryBus::new(CAPACITY);
let mut stream = bus.subscribe();
bus.publish(event);
```

`publish` never blocks and never fails. `subscribe` returns a stream of
`BusMessage`; dropping it releases the subscriber.

## Behavior

- Events are hints, not a log: an event published while nobody
  subscribes is lost. A client that needs the truth reads the API
  (`GET /api/v1/jobs/{id}`).
- Each subscriber has a buffer of `CAPACITY` (256) events. One that falls
  further behind gets `BusMessage::Lagged` once, then the newest events:
  it must resync from the API.
- `MemoryBus::receiver_count` gives the number of connected subscribers.
