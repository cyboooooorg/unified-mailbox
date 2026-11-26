# HTTP skeleton design

Issue: #13. Related: #12 (configuration), #14 (jobs and SSE), #15 (client
skeleton), #26 (audit log), #58 (iOS and Android apps).

## Intent

The server becomes a running HTTP server. Clients reach it over Tailscale
and authenticate each request with a bearer token of their own, one per
device, revocable one by one. The routes and DTOs live in an `http` crate
that also generates `api/openapi.json`, the only thing the client knows
of the server.

## Goals

- An `http` crate (axum) serving `/api/v1`, with its own DTOs and one
  error format.
- `api/openapi.json` generated from that crate, and a check that fails
  when the committed file is stale.
- Per-device bearer tokens: pairing with a one-time code, listing,
  revocation.
- The first ports (`DeviceRepository`, `Clock`, `SecretGenerator`) and use
  cases (`application`), and their SQLite implementation.
- `LISTEN_ADDR` in the configuration; `main` serves until SIGTERM or
  SIGINT.
- A `pair-code` subcommand of the server binary, for the first device and
  for recovery.

## Non-goals

- Jobs and SSE (#14), the client (#15), CORS for the mobile apps (#58),
  the audit entries of pairing and revocation (#26).
- Serving the web client's static files: the issue that builds the client
  decides it.
- Rate limiting the pairing route: a code has about 40 bits and lives ten
  minutes, behind Tailscale.

## Anti-goals

- A route other than `/health` and `/pairing` reachable without a valid
  token.
- A token or pairing code stored in clear, or written to a log.
- A domain type serialized straight to the client.
- A contract that can drift from the router.

## Mailbot reference

Mailbot has no authentication (`app/main.py:118-137`): it listens on
`127.0.0.1:8000` and a middleware rejects a `Host` outside
`ALLOWED_HOSTS` (421) and a non-safe request whose `Origin` differs from
the host (403). Errors are FastAPI's `{"detail": "<French message>"}`.
Routes are flat under `/api/...`, without a version or a health route. No
test covers the HTTP layer, so no edge case is carried over.

Deliberate deviations:

- No Host or Origin check (`ALLOWED_HOSTS` was dropped in #12). A browser
  never sends an `Authorization` header by itself, so neither a cross-site
  request nor DNS rebinding gets anywhere without a token.
- A versioned prefix, a health route, and errors carrying a stable code
  with an English message; the client translates by code.

## Approaches

OpenAPI generator, chosen by the user:

- **utoipa 5 with utoipa-axum (chosen).** The most used, OpenAPI 3.1.
  `OpenApiRouter` with `routes!` declares each handler once for both the
  router and the document, so they cannot diverge.
- aide: axum-native on schemars, less used, its API changes more often.
- Contract written by hand: the most readable source of truth, but Rust
  server generators fit axum badly and code and contract can drift.

Pairing, chosen by the user:

- **A one-time pairing code (chosen)**, created either by an already
  paired device or by the `pair-code` subcommand on the server. The new
  device trades it for its token. Eight characters are easy to type on a
  phone; a 43-character token is not.
- A code printed in the logs at startup when no device exists: rejected,
  the code would end up in journald.
- A bootstrap token in `.env`: rejected, a permanent secret.

## Routes

All under `/api/v1`. Bodies are JSON.

| Route | Token | Answer |
|---|---|---|
| `GET /health` | no | `200 {"status": "ok", "version": "<crate version>"}` |
| `POST /pairing` | no | body `{"code", "name"}`; `201 {"device_id", "token"}` |
| `GET /devices` | yes | `200 [{"id", "name", "created_at", "last_seen_at", "current"}]` |
| `POST /devices/pairing-codes` | yes | `201 {"code", "expires_at"}` |
| `DELETE /devices/{id}` | yes | `204`; `404` when unknown or already revoked |

- `GET /devices` lists the devices not revoked, oldest first. `current`
  is true for the device that made the request.
- A device may revoke itself; its next request gets a 401.
- `AuthenticateDevice` returns the caller's `DeviceId`; the
  authentication layer puts it in the request extensions. `current` is
  computed in the DTO conversion; `CreatePairingCode` takes it as
  `created_by`.
- `POST /pairing` with an unknown, used or expired code answers 400
  `invalid_pairing_code`, the same for the three cases.
- Dates are RFC 3339 strings in UTC.
- The token is returned once, in the pairing answer, and never again.

## Tokens and pairing codes

- Token: 32 random bytes, base64url without padding, prefixed `um_` so
  that secret scanners recognize it. Stored as its SHA-256 hash (hex):
  its entropy makes a slow hash useless. Looked up by hash.
- Pairing code: 8 characters of Crockford base32 (about 40 bits), shown
  as `XXXX-XXXX`. Valid ten minutes, single use. Input is normalized
  before hashing: hyphens and spaces removed, upper-cased, `O` read as
  `0`, `I` and `L` read as `1`. Stored as its SHA-256 hash.
- Redeeming a code and creating the device happen in one transaction; a
  code redeemed twice at once yields one device.
- Device name: trimmed, 1 to 64 characters, no control character.
  Duplicates are allowed (two phones may both be "Phone").
- `last_seen_at` is written at most once a minute per device, to avoid a
  write per request. Stateless: the token lookup returns the stored
  `last_seen_at`, and `AuthenticateDevice` touches it only when it is a
  minute old or more. Every authenticated route goes through it.
- The lookup returns active devices only (`revoked_at IS NULL` in the
  same query), so a revoked token fails at its next request.
- The server has one user: any paired device may list, pair and revoke
  any device, itself included.
- Revocation sets `revoked_at`; the row stays.
- Expired and used codes are deleted when a new code is created.

## Errors

Every error, including axum's own rejections and unknown routes, has the
shape:

```json
{ "error": { "code": "not_found", "message": "device not found" } }
```

| Status | Code | When |
|---|---|---|
| 400 | `invalid_pairing_code` | pairing with an unknown, used or expired code |
| 401 | `unauthorized` | missing, malformed, unknown or revoked token; with `WWW-Authenticate: Bearer` |
| 404 | `not_found` | unknown route or resource |
| 405 | `method_not_allowed` | known route, wrong method |
| 413 | `payload_too_large` | body over axum's default limit |
| 415 | `unsupported_media_type` | body that is not `application/json` |
| 422 | `invalid_request` | malformed JSON (axum's 400 syntax error is remapped) or invalid field (name length…) |
| 500 | `internal` | anything else; the message says nothing more, the cause is logged |

The codes are part of the contract (an enum in `openapi.json`).

## Layers

- `domain`: `DeviceId`, `DeviceName` (the validation above), `Device`
  (id, name, created, last seen, revoked), `PairingCode` (format and
  normalization). No I/O.
- `ports`: `DeviceRepository` (create a pairing code, redeem a code and
  create the device, find an active device by token hash, touch
  `last_seen_at`, list, revoke), `Clock`, `SecretGenerator` (random
  bytes). The hash is computed in `application`.
- `application`: `CreatePairingCode`, `PairDevice`, `AuthenticateDevice`,
  `ListDevices`, `RevokeDevice`. A system clock and an OS random generator
  live in `app` (or a small adapter); tests use fakes.
- `sqlite`: migration `0002_devices.sql` with `devices` (`id`, `name`,
  `token_hash` unique, `created_at`, `last_seen_at`, `revoked_at`) and
  `pairing_codes` (`code_hash` primary key, `created_at`, `expires_at`,
  `created_by` nullable device id, `used_at`), and the repository.
- `http` (directory `crates/http`, package `http-api`: a package
  named `http` would clash with the `http` crate axum depends on): the
  router, the DTOs and their conversions, the error type, an
  authentication layer that calls `AuthenticateDevice` and puts the
  device id in the request extensions, and `openapi()`. It depends on
  `application`, never on `sqlite`.
- `app`: `LISTEN_ADDR`, the wiring, the server, the subcommand.

## Server

- `LISTEN_ADDR`: a socket address, default `127.0.0.1:8000` (Mailbot's
  port). Behind Tailscale, `tailscale serve` proxies to it, or it is set
  to the tailnet address; in Docker, `0.0.0.0:8000` with the port
  published on the tailnet interface only. An invalid value is reported
  with the other configuration errors.
- Graceful shutdown on SIGTERM and SIGINT, then the store is closed.
- `tower-http` `TraceLayer` logs method, path, status and latency, never
  headers.
- Every API response carries `Cache-Control: no-store` and
  `X-Content-Type-Options: nosniff`.

## Subcommand

`unified-mailbox pair-code` reads the configuration, opens the store,
creates a pairing code (no `created_by`), prints it with its expiry on
stdout and exits. It prepares the directories and opens the store exactly
as the server does, so on a fresh install it creates the database, runs
the migrations and creates the key file: pairing the first device before
the server ever started works. It runs alongside a running server (SQLite in WAL mode).
Without an argument the binary serves, as today. Any other argument is an
error that prints the usage.

## Contract

- `http` exposes `openapi() -> utoipa::openapi::OpenApi`, built from the
  same `OpenApiRouter` as the server. It declares a global `bearer`
  security scheme; `/health` and `/pairing` override it with none.
- `mise run openapi` writes `api/openapi.json` (pretty-printed, trailing
  newline), through an entry point of `http` (a binary or an example),
  since `http` cannot depend on `app`. The check reads the file at
  `CARGO_MANIFEST_DIR/../../../api/openapi.json`.
- A test in `http` compares the generated document with the committed
  file and fails, naming `mise run openapi`, when they differ. `mise run
  ci` therefore catches a stale contract.

## Tests

- `domain`: name validation (empty after trim, 64 and 65 characters,
  control character), code normalization (lower case, hyphen, spaces,
  `O`/`I`/`L`).
- `application`, with fakes: a code expires at ten minutes exactly; a used
  code is refused; the token is never stored in clear; a revoked device
  no longer authenticates; `last_seen_at` is not written twice within a
  minute.
- `sqlite`: the migration, the repository, the double redemption of one
  code.
- `http`, against the router with fake use cases or an in-memory store:
  every route; a missing, malformed, unknown and revoked token gives 401
  with `WWW-Authenticate`; `current`; self-revocation; each error code of
  the table, including unknown route and malformed JSON; the security
  headers; the contract check.
- `app`: `LISTEN_ADDR` default and invalid value; argument parsing of the
  subcommand.

## Documentation

- README: `LISTEN_ADDR` in the Configuration table, pairing a device (the
  subcommand, then the route), Tailscale exposure, `mise run openapi`.
- `.env.example`: `LISTEN_ADDR`.

## Follow-ups to record

- #14: `EventSource` cannot send an `Authorization` header; the client
  reads the stream with `fetch`, or the server issues a short-lived
  ticket for the URL.
- The mail HTML view and the image proxy: an `iframe src` or `img src`
  sends no header either; they need short-lived signed URLs.
- #58: the mobile apps have another origin than the server and need CORS
  without credentials.

## Left to the builder

- The exact crate versions and the module split inside `http`.
- Whether the system clock and the random generator live in `app` or in a
  small adapter crate.
- How the double-redemption guard is written in SQL (conditional update or
  delete returning a row).
