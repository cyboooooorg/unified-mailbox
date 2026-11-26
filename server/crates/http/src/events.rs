//! The event stream: server-sent events telling clients what changed.

use std::convert::Infallible;
use std::future::ready;
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderName;
use axum::response::IntoResponse;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use domain::{DeviceId, Event};
use futures_util::{StreamExt, stream};
use ports::BusMessage;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentDevice;
use crate::error::ErrorBody;
use crate::jobs::JobDto;
use crate::state::AppState;

/// How long a client waits before reconnecting, sent first on the stream.
const RETRY: Duration = Duration::from_millis(3000);

/// One event of the stream, the JSON of a `data:` line, tagged by `type`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventDto {
    /// A job was created, progressed or finished.
    Job { job: JobDto },
    /// A device was revoked: a hint to reload the device list.
    DeviceRevoked { device_id: i64 },
    /// The subscriber missed events: reload what is shown.
    Resync,
}

/// The event to forward for one bus message; `None` when the stream must
/// end, because the caller's own device was revoked.
fn forward(message: BusMessage, caller: DeviceId) -> Option<EventDto> {
    match message {
        BusMessage::Event(Event::Job(job)) => Some(EventDto::Job { job: job.into() }),
        BusMessage::Event(Event::DeviceRevoked(id)) if id == caller => None,
        BusMessage::Event(Event::DeviceRevoked(id)) => Some(EventDto::DeviceRevoked {
            device_id: id.get(),
        }),
        BusMessage::Lagged => Some(EventDto::Resync),
    }
}

fn sse_event(event: &EventDto) -> SseEvent {
    // An `EventDto` always serializes: no map with non-string keys, no
    // failing `Serialize` impl.
    SseEvent::default()
        .json_data(event)
        .expect("EventDto serializes to JSON")
}

/// Streams the server's events as server-sent events.
///
/// The stream starts with `retry: 3000`, then sends each event as one
/// `data:` line holding an `EventDto` as JSON, and a `: ping` comment when
/// idle. It ends when the caller's device is revoked (its reconnection then
/// gets a 401) and when the server shuts down.
///
/// The client reads it with `fetch` and the bearer token (`EventSource`
/// cannot send the header). On every connect and reconnect, and on every
/// `resync` event, it reloads what it shows (and `GET /jobs/{id}` for a job
/// it follows): events only tell what changed since.
#[utoipa::path(get, path = "/events", operation_id = "stream_events",
    responses(
        (status = 200,
            description = "A stream of server-sent events: `retry: 3000` first, then one `data:` line per event holding an `EventDto` as JSON, and a `: ping` comment when idle",
            content_type = "text/event-stream", body = String),
        (status = 401, description = "Missing or invalid token", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    ))]
async fn stream_events(
    CurrentDevice(caller): CurrentDevice,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Subscribed before the response is sent, so the `retry` frame proves
    // the subscription exists.
    let messages = state.subscribe_events.execute();
    let events = messages
        .map(move |message| forward(message, caller))
        .take_while(|event| ready(event.is_some()))
        .filter_map(|event| ready(event.as_ref().map(sse_event)));
    let mut shutdown = state.shutdown.clone();
    // A dropped sender counts as a shutdown too.
    let shut_down = async move {
        let _ = shutdown.wait_for(|stopping| *stopping).await;
    };
    let body = stream::once(ready(SseEvent::default().retry(RETRY)))
        .chain(events)
        .take_until(shut_down)
        .map(Ok::<_, Infallible>);
    (
        [(HeaderName::from_static("x-accel-buffering"), "no")],
        Sse::new(body).keep_alive(KeepAlive::new().interval(state.ping_interval).text("ping")),
    )
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stream_events))
}
