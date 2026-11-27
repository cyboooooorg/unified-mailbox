# Jobs and server-sent events design

Issue: #14. Related: #13 (HTTP skeleton), #15 (client skeleton), #18
(sync loop), #31 (history indexing), #50 (history processing with
Claude), #62 (internationalization).

## Intent

Long operations run as jobs: the command that starts one returns a job
id at once, progress goes out on a server-sent event stream, and
`GET /api/v1/jobs/{id}` tells where a job stands, even after a restart.
The same stream will carry sync status and new-mail events. This issue
builds the infrastructure only: no feature of phase 0 starts a job yet;
#18, #31 and #50 are the first producers.

## Goals

- A job record in SQLite, with status and progress, readable through the
  API after a restart.
- A progress handle that use cases update as they go, throttled.
- An in-memory event bus and `GET /api/v1/events`, authenticated with
  the device's bearer token like every other route.
- The events described in the contract, so the client generates their
  types.
- Streams that end on shutdown and when the device is revoked.

## Non-goals

- A feature that starts a job (#18, #31, #50 and later).
- Resuming a job after a restart: #50 resumes its own runs from its own
  tables, as Mailbot's backfill does.
- Cancelling or pausing a job: #50, the first job that needs it.
- The client's stream reader: #15.
- Replaying missed events (`Last-Event-ID`).

## Anti-goals

- A token or any secret in a URL.
- A stream that keeps the server from shutting down.
- A revoked device that keeps receiving events.
- A dropped event that leaves the client wrong without telling it.
- A domain type serialized straight to the client.

## Mailbot reference

From `app/events.py`, `app/main.py:348-366`, `app/backfill.py` and
`tests/test_backfill.py`:

- `EventBus` (`events.py:6-45`): one queue of 100 events per subscriber.
  Events are small dicts with a `type`; they are hints ("something
  changed, refetch"), not state. No id, no replay, no snapshot on
  connect. A full queue drops the event for that subscriber silently.
- `Progress` (`events.py:48-81`): a job id (`uuid4().hex[:8]`) that no
  route returns, a label, `done`, `failed`, `total`, `finished`. Publishes
  on creation, then at most every 0.3 s; `finish()` always publishes.
  Nothing is published when `total < 2`. The client drops a progress
  toast after 60 s without news.
- `/api/events`: no authentication, `retry: 3000` first, `data: <json>`
  per event without `event:` or `id:`, a `: ping` comment every 15 s,
  `Cache-Control: no-cache`. On SIGTERM `bus.close()` ends every stream,
  otherwise uvicorn waits for them forever. The client reloads its data
  when it reconnects: the only catch-up.
- No job table: each long operation keeps its own state in memory and
  loses it on restart, except the backfill, persisted in `backfills`,
  `backfill_items` and `backfill_batches`. Its runner rereads the active
  run and its pending items; a mail already classified is skipped
  (`fresh()`), so the resume is idempotent.
- No test covers the bus, `Progress` or the stream; only
  `test_backfill.py` covers the resume.

Deliberate deviations:

- The job id is returned by the route that starts the job and the job is
  persisted, so a client that reconnects, or a server that restarts,
  still knows how it ended.
- Progress is published whatever the total; the client decides not to
  show a toast for a single mail.
- A subscriber that falls behind receives a `resync` event instead of
  losing events silently, then its stream ends: the client reconnects
  and reloads.
- The stream requires the bearer token, and ends as soon as its device
  is revoked.

## Approaches

### Authenticating the stream

- **`fetch` with a streaming body (chosen).** The client sends the
  bearer token as on every route; the stream goes through the same
  extractor. The client parses the stream itself (a small parser or
  `@microsoft/fetch-event-source`) and handles reconnection; it works in
  the Capacitor WebViews.
- **A short-lived ticket in the URL for `EventSource`.** Rejected:
  `EventSource` reconnects to the same URL, so with a ticket that has
  expired; the client would need its own reconnection anyway, plus a
  route and a ticket store on the server.

### Jobs across a restart

- **Persist the record, no generic resume (chosen).** A `jobs` table
  holds status and progress; at startup a job left `running` becomes
  `interrupted`. A job kind that must resume (#50) does it from its own
  tables.
- **In memory only, as Mailbot.** Rejected: after a restart the client
  gets a 404 for a job it was following, and never learns how it ended.
- **A generic resumable framework with a cursor per job.** Rejected for
  now: only #50 needs a resume, and its state (items, batches, spend)
  does not fit a generic cursor.

## Domain

- `JobId`: an `i64`, as `DeviceId`.
- `JobKind`: a validated string (`[a-z][a-z_]*`, at most 32 characters).
  Each feature defines its constants (`labels_sync`, `history_index`…);
  the client translates it by code (#62). No enum: phase 0 has no kind
  to list.
- `JobStatus`: `running`, `succeeded`, `failed`, `interrupted`. #50 adds
  `paused` and `cancelled` when it needs them.
- `JobProgress`: `done`, `failed` (counters), `total` (optional: a sync
  may not know it).
- `Job`: id, kind, status, progress, `error` (an optional English
  message), `created_at`, `finished_at`.
- `Event`: the enum of what goes on the bus. In this issue: `Job(Job)`,
  sent for progress and for the end (the status tells which), and
  `DeviceRevoked(DeviceId)`. Later issues add variants (sync, account,
  message…).

## Ports

- `JobRepository`: create a running job, save its progress, finish it
  (status, progress, error, `finished_at`), get one, list the running
  ones, mark every running job `interrupted` (returns how many), delete
  finished jobs older than a date.
- `EventBus`: `publish(Event)` (never blocks, never fails: no
  subscriber is not an error) and `subscribe()`, a stream of
  `BusMessage` (`Event(Event)` or `Lagged`). `ports` may depend on
  `futures-core` for the stream type; no I/O crate.

## Application

- `StartJob` (kind, total) creates the record, publishes it and returns
  a `JobHandle`.
- `JobHandle::run(work)`: awaits the work, a future that receives a
  `Progress` and returns `Result<(), JobError>` (an English message).
  `Ok` ends the job `succeeded`, `Err` ends it `failed` with the message,
  and a panic (caught with `catch_unwind` from `futures-util`) ends it
  `failed` with "job panicked". The end is reached by awaiting, never in
  `Drop`, so no port needs a synchronous call and `application` still
  needs no runtime. A job whose task dies otherwise (runtime shut down)
  stays `running` until `RecoverJobs` marks it `interrupted`.
- `Progress` (`&self`, shareable between concurrent workers):
  `step(ok)` (one more done, and one more failed when not ok) and
  `set_total`. It saves and publishes at most every 0.3 s, measured with
  `Clock`; the end always saves and publishes the final counters. A
  failed save is logged and the work goes on: progress is a hint.
- A failed final write (`finish`) is logged, the end is still
  published, and the ended job is kept in memory in `UnsavedJobEnds`
  (shared through `Deps`) until it is written (#75). `GetJob` returns
  the kept end and `ListActiveJobs` leaves the job out, so the API
  agrees with the published end at once. `FlushJobEnds` writes the kept
  ends again and forgets each one that is saved; `app` runs it every
  30 s and once more after the graceful shutdown. No retry limit: one
  entry per ended job, and a lost outcome is never guessed. If the
  process dies before the write succeeds, the outcome is lost and the
  next startup marks the job `interrupted`, as for a job cut by a stop.
- `GetJob`, `ListActiveJobs`.
- `RecoverJobs`, run once at startup: running jobs become `interrupted`
  with `finished_at` set to now (logged with their count); jobs whose
  `finished_at` is older than 30 days are deleted. `app` runs it after
  binding the listener and before serving: a second server started by
  mistake on the same data directory fails to bind and leaves the live
  server's jobs alone.
- `SubscribeEvents`: hands the bus's stream to the route.
- `DeviceIsActive(id)`: whether a device is paired and not revoked,
  read from `DeviceRepository::list_active` (few devices).
- `RevokeDevice` publishes `Event::DeviceRevoked(DeviceId)` after the
  revocation is stored.
- The job repository and the bus join `Deps`, next to the device
  repository, the clock and the secret generator; the tests that build
  `Deps` gain the two fakes.
- Who runs the work (a `tokio::spawn` in the feature's route or
  background task) is outside this issue; `application` gains no
  dependency on `tokio`.

## Storage

In the existing `sqlite` crate, on `Store`, as `DeviceRepository`.
Migration `0003_jobs.sql`: `jobs` (`id` integer primary key, `kind`,
`status`, `done`, `failed`, `total` nullable, `error` nullable,
`created_at`, `finished_at` nullable), and an index on
`status`. Progress writes happen at the 0.3 s throttle at most.

## Event bus

A new adapter crate, `crates/adapters/events` (package `events-memory`),
implements `EventBus` over `tokio::sync::broadcast` with a capacity of
256. A receiver's `Lagged` error becomes `BusMessage::Lagged`. Publishing
with no subscriber drops the event, as in Mailbot.

## HTTP

| Route | Answer |
|---|---|
| `GET /jobs` | `200 [Job]`: the running jobs, oldest first |
| `GET /jobs/{id}` | `200 Job`; `404 not_found` when unknown |
| `GET /events` | `200 text/event-stream` |

All under `/api/v1`, all with the bearer token.

`Job` DTO: `{"id", "kind", "status", "done", "failed", "total",
"error", "created_at", "finished_at"}`; dates in RFC 3339, UTC.

The stream:

- Headers: `Content-Type: text/event-stream`, `X-Accel-Buffering: no`,
  plus the global `Cache-Control: no-store` and `nosniff`.
- It starts with `retry: 3000`.
- Each event is one `data: <json>` line, without `event:` or `id:`. The
  JSON is an `Event` DTO, tagged by `type`:
  - `{"type": "job", "job": Job}`;
  - `{"type": "device_revoked", "device_id"}`: a hint to reload the
    device list. The stream of the revoked device itself ends instead of
    forwarding it, at once; its reconnection gets a 401.
  - `{"type": "resync"}`: the subscriber missed events. The stream
    ends right after it: a missed event may be the caller's own
    `device_revoked`, so the client reconnects (which authenticates it
    again) and reloads what it shows.
- Right after subscribing, the handler checks that the caller's device
  is still active (`DeviceIsActive`, from `application`) and ends the
  stream if not: a revocation between the authentication of the request
  and the subscription published its event before the stream could
  receive it. A storage error at that check answers `500`.
- A `: ping` comment every 15 s.
- Shutdown: a signal shared with `axum::serve`'s graceful shutdown ends
  every stream first, so the server stops at once. It reaches the
  handler through `AppState`.
- The client's rule, written in the API documentation: on every connect
  and reconnect, reload what is shown (and `GET /jobs/{id}` for a job it
  follows); events tell what changed since.

The contract describes `/events` as `text/event-stream` and adds the
`Event` schema (`oneOf` with a `type` discriminator) to the components,
so the client generates its types even though utoipa cannot type a
stream's items.

## Tests

- `domain`: `JobKind` validation (empty, upper case, 32 and 33
  characters).
- `application`, with fakes (repository, bus, clock):
  - the first step publishes; a second step within 0.3 s does not; one
    at 0.3 s does;
  - `Ok` and `Err` always save and publish, even within 0.3 s of a
    step, with the final counters; `Err` carries its message;
  - a panicking work ends the job `failed`;
  - a failed final write still publishes the end, `GetJob` returns the
    ended job and `ListActiveJobs` leaves it out; `FlushJobEnds` keeps
    it while the write still fails, then saves it and forgets it once
    the write works;
  - `RevokeDevice` publishes `DeviceRevoked`;
  - `RecoverJobs` marks running jobs `interrupted` and deletes finished
    ones at 30 days, not at 29.
- `sqlite`: the migration; the repository round trip; `interrupted`
  only touches running jobs.
- `events-memory`: publish without subscriber; two subscribers both
  receive; a subscriber past the capacity gets `Lagged` then the next
  events.
- `http`:
  - `GET /jobs`, `GET /jobs/{id}`, 404, 401 without a token (the
    existing contract walk covers the new routes);
  - the stream: headers, `retry`, a published job event arrives as
    JSON, a lag gives `resync` then the end of the stream, a device
    found inactive right after subscribing gets an ended stream, the
    ping (with a short interval injected in tests), the revoked
    device's stream ends while another device's receives
    `device_revoked`;
  - the contract check; the contract walk learns that `/events` answers
    `text/event-stream`.
- `app`: SIGINT with an open stream stops the server (next to the
  existing shutdown test); a job left running before a restart is
  `interrupted` after it; a server that fails to bind leaves a running
  job `running`.

## Documentation

- README: the jobs and events section (routes, the stream format, the
  reload rule, `interrupted`).
- The crate README of `events-memory`; `docs` comments on the public
  types.

## Follow-ups to record

- #15: the client reads `/events` with `fetch`, reconnects after
  `retry`, reloads on every (re)connect and on `resync`, and gets the
  event types from the contract.
- #50: add `paused` and `cancelled`, the cancel route, and make its job
  kind resume instead of becoming `interrupted`.

## Dependencies

`tokio` gains the `sync` and `time` features (and `test-util` in
dev-dependencies); `http-api` and `events-memory` depend on it.
`futures-util` (and `futures-core` in `ports`) join the workspace;
`tokio-stream` if the stream code needs it. No crate gives `domain` or
`application` a runtime.

## Left to the builder

- The exact form of the stream in `ports` (`BoxStream` or a custom
  trait) and the module split.
- Whether `JobHandle` takes its dependencies by `Arc` or by `Deps`.
- How the shutdown signal is shared between `app` and `http` (a
  `watch` channel or a cancellation token).
