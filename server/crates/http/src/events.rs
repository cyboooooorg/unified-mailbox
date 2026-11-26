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
use crate::error::{ApiError, ErrorBody};
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
    /// The subscriber missed events: reload what is shown. The stream ends
    /// right after it; reconnect.
    Resync,
}

/// What the stream does with one bus message.
#[derive(Debug)]
enum Next {
    /// Send the event and go on.
    Send(EventDto),
    /// Send the event, then end the stream.
    Last(EventDto),
    /// End the stream without sending anything.
    End,
}

/// What to do with one bus message for the stream of `caller`.
///
/// The stream ends on the caller's own revocation (its reconnection gets a
/// 401), and after `resync` on a lag: a lost event may be that revocation,
/// and the reconnection authenticates the caller again.
fn forward(message: BusMessage, caller: DeviceId) -> Next {
    match message {
        BusMessage::Event(Event::Job(job)) => Next::Send(EventDto::Job { job: job.into() }),
        BusMessage::Event(Event::DeviceRevoked(id)) if id == caller => Next::End,
        BusMessage::Event(Event::DeviceRevoked(id)) => Next::Send(EventDto::DeviceRevoked {
            device_id: id.get(),
        }),
        BusMessage::Lagged => Next::Last(EventDto::Resync),
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
/// gets a 401), after a `resync` event, and when the server shuts down.
///
/// The client reads it with `fetch` and the bearer token (`EventSource`
/// cannot send the header). On every connect and reconnect, and on every
/// `resync` event, it reloads what it shows (and `GET /jobs/{id}` for a job
/// it follows): events only tell what changed since. After `resync` the
/// stream ends: the client reconnects.
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
) -> Result<impl IntoResponse, ApiError> {
    // Subscribed before the response is sent, so the `retry` frame proves
    // the subscription exists.
    let messages = state.subscribe_events.execute();
    // A revocation between the authentication of this request and the
    // subscription published its event before the stream could get it:
    // check again, now that later ones cannot be missed.
    let active = state.device_is_active.execute(caller).await?;
    let messages = if active {
        messages
    } else {
        stream::empty().boxed()
    };
    let events = messages.scan(false, move |ended, message| {
        if *ended {
            return ready(None);
        }
        let event = match forward(message, caller) {
            Next::Send(event) => event,
            Next::Last(event) => {
                *ended = true;
                event
            }
            Next::End => return ready(None),
        };
        ready(Some(sse_event(&event)))
    });
    let mut shutdown = state.shutdown.clone();
    // A dropped sender counts as a shutdown too.
    let shut_down = async move {
        let _ = shutdown.wait_for(|stopping| *stopping).await;
    };
    let body = stream::once(ready(SseEvent::default().retry(RETRY)))
        .chain(events)
        .take_until(shut_down)
        .map(Ok::<_, Infallible>);
    Ok((
        [(HeaderName::from_static("x-accel-buffering"), "no")],
        Sse::new(body).keep_alive(KeepAlive::new().interval(state.ping_interval).text("ping")),
    ))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stream_events))
}
