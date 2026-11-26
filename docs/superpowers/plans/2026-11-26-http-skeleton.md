# HTTP Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the server into a running axum server under `/api/v1`,
with per-device bearer tokens paired by one-time code and
`api/openapi.json` generated from the `http` crate.

**Architecture:** `domain` gets the device types, `ports` the first
traits (`DeviceRepository`, `Clock`, `SecretGenerator`), `application`
one use case per action, `sqlite` the repository, a new `http` crate the
routes, DTOs, errors and contract, `app` the wiring, `LISTEN_ADDR` and
the `pair-code` subcommand.

**Tech Stack:** Rust 2024, axum 0.8, utoipa 5 + utoipa-axum 0.2,
tower-http 0.6, serde, sqlx 0.8 (SQLite), async-trait, sha2, base64.

**Spec:** `docs/superpowers/specs/2026-11-26-http-skeleton-design.md`
(issue #13). Read it before any task: values below come from it.

## Global Constraints

- Every route under `/api/v1`. Only `GET /health` and `POST /pairing`
  work without a token.
- Error body, everywhere: `{"error": {"code": "<code>", "message": "<english>"}}`.
  Codes: `invalid_pairing_code` (400), `unauthorized` (401, with
  `WWW-Authenticate: Bearer`), `not_found` (404), `method_not_allowed`
  (405), `payload_too_large` (413), `unsupported_media_type` (415),
  `invalid_request` (422), `internal` (500, message says nothing more).
- Token: `um_` + base64url without padding of 32 random bytes (46
  characters). Stored only as lowercase hex SHA-256 of the full token
  string.
- Pairing code: 8 Crockford base32 characters (alphabet
  `0123456789ABCDEFGHJKMNPQRSTVWXYZ`), shown `XXXX-XXXX`, valid while
  `now < created_at + 10 minutes`, single use, stored only as hex
  SHA-256 of the 8 normalized characters.
- Device name: trimmed, 1 to 64 characters (`char`s, not bytes), no
  control character; duplicates allowed.
- `last_seen_at` written only when unset or at least 1 minute old.
- Dates on the wire: RFC 3339, UTC.
- No token or pairing code in a log line, a `Debug` output or an error
  message.
- `domain`, `ports`, `application` stay free of I/O crates: the
  `layering` test in `app` must keep passing.
- Every API response, errors included, carries `Cache-Control: no-store`
  and `X-Content-Type-Options: nosniff`.
- `LISTEN_ADDR` default `127.0.0.1:8000`.
- Each task ends with `mise run ci` green and a Conventional Commit
  (scope `domain`, `sqlite`, `http`, `api`, `app`), ending with the
  session's `Co-Authored-By` and `Claude-Session` trailers.

## Review Focus

- `Authorization` variants: `bearer` in lower case, several spaces,
  `Basic …`, `Bearer` with nothing after, a token one character short:
  each a 401, never a 500 (Task 6).
- A name of 64 accented characters (`é` × 64) is accepted though it is
  128 bytes; 65 is refused (Task 1).
- A code typed as `abcd efgh`, `ABCD-EFGH` or with `o`/`i`/`l` pairs with
  the code shown (Task 1, Task 6).
- `DELETE /api/v1/devices/abc` and `/devices/99999999999999999999` give
  404 `not_found`, not 422 or 500 (Task 6).
- `LISTEN_ADDR` on a port already in use: stderr names `LISTEN_ADDR`,
  exit status 1 (Task 7).

---

### Task 1: Device types in `domain`

**Files:**
- Create: `server/crates/domain/src/device.rs`
- Modify: `server/crates/domain/src/lib.rs` (module, re-export, module
  list in the doc comment)

**Interfaces:**
- Produces:
  - `pub struct DeviceId(i64)` (`Copy, Clone, Debug, PartialEq, Eq,
    Hash`) with `DeviceId::new(i64) -> DeviceId` and `get(self) -> i64`.
  - `pub struct DeviceName(String)` with `DeviceName::new(&str) ->
    Result<DeviceName, DeviceNameError>`, `as_str(&self) -> &str`,
    `pub const MAX_CHARS: usize = 64`.
  - `pub enum DeviceNameError { Empty, TooLong { max: usize },
    ControlCharacter }` (`thiserror`, messages are codes:
    `device_name_empty`, `device_name_too_long`,
    `device_name_control_character`).
  - `pub struct Device { pub id: DeviceId, pub name: DeviceName, pub
    created_at: DateTime<Utc>, pub last_seen_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>> }`.
  - `pub struct PairingCode(String)` holding the 8 normalized
    characters, with `PairingCode::from_bytes([u8; 5]) -> PairingCode`
    (40 bits, most significant first, 5 bits per character),
    `PairingCode::parse(&str) -> Result<PairingCode,
    InvalidPairingCode>`, `as_str(&self) -> &str` (8 chars, no hyphen),
    `Display` writing `XXXX-XXXX`. Its `Debug` prints
    `PairingCode(..)`, never the value.
  - `pub struct InvalidPairingCode;` (unit error, code
    `invalid_pairing_code`).
  - `pub const PAIRING_CODE_TTL: TimeDelta` = 10 minutes,
    `pub const LAST_SEEN_RESOLUTION: TimeDelta` = 1 minute.

- [ ] **Step 1: Write the failing tests** (in `device.rs`,
  `#[cfg(test)]`):

```rust
#[test] fn name_is_trimmed() {
    assert_eq!(DeviceName::new("  Phone ").unwrap().as_str(), "Phone");
}
#[test] fn name_empty_after_trim() {
    assert_eq!(DeviceName::new(" \t "), Err(DeviceNameError::Empty));
}
#[test] fn name_counts_chars_not_bytes() {
    assert!(DeviceName::new(&"é".repeat(64)).is_ok());
    assert_eq!(DeviceName::new(&"é".repeat(65)), Err(DeviceNameError::TooLong { max: 64 }));
}
#[test] fn name_rejects_control_character() {
    assert_eq!(DeviceName::new("Pho\u{7}ne"), Err(DeviceNameError::ControlCharacter));
}
#[test] fn duplicate_names_are_values() {
    assert_eq!(DeviceName::new("Phone"), DeviceName::new("Phone"));
}
#[test] fn code_from_bytes_uses_crockford() {
    assert_eq!(PairingCode::from_bytes([0; 5]).as_str(), "00000000");
    assert_eq!(PairingCode::from_bytes([0xff; 5]).as_str(), "ZZZZZZZZ");
    // 0x0123456789 = 00000 00100 10001 10100 01010 11001 11100 01001
    assert_eq!(PairingCode::from_bytes([0x01, 0x23, 0x45, 0x67, 0x89]).as_str(), "04HMASW9");
}
#[test] fn code_displays_with_hyphen() {
    assert_eq!(PairingCode::from_bytes([0x01, 0x23, 0x45, 0x67, 0x89]).to_string(), "04HM-ASW9");
}
#[test] fn code_parse_normalizes() {
    for input in ["04HM-ASW9", "04hmasw9", " 04hm asw9 ", "O4HM-ASW9"] {
        assert_eq!(PairingCode::parse(input).unwrap().as_str(), "04HMASW9", "{input}");
    }
    assert_eq!(PairingCode::parse("1ILl-0000").unwrap().as_str(), "11110000");
}
#[test] fn code_parse_rejects() {
    for input in ["", "04HM-ASW", "04HM-ASW90", "04HM-ASWU", "04HM_ASW9"] {
        assert_eq!(PairingCode::parse(input), Err(InvalidPairingCode), "{input}");
    }
}
#[test] fn code_debug_hides_value() {
    assert_eq!(format!("{:?}", PairingCode::from_bytes([0; 5])), "PairingCode(..)");
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cd server && cargo test -p domain device`
Expected: compile errors (types missing).

- [ ] **Step 3: Implement `device.rs`**

Normalization in `parse`: drop ASCII whitespace and `-`, upper-case,
map `O`→`0`, `I` and `L`→`1`, then require exactly 8 characters of the
alphabet (`U` is not in it). Control characters: `char::is_control`.

- [ ] **Step 4: Run them and see them pass**

Run: `cd server && cargo test -p domain device`
Expected: PASS.

- [ ] **Step 5: `mise run ci`, then commit**

```bash
git add server/crates/domain
git commit -m "feat(domain): add devices, device names and pairing codes"
```

---

### Task 2: Ports and test doubles

**Files:**
- Modify: `server/crates/ports/src/lib.rs`, `server/crates/ports/Cargo.toml`
- Create: `server/crates/ports/src/clock.rs`, `server/crates/ports/src/secrets.rs`,
  `server/crates/ports/src/devices.rs`
- Create: `server/crates/application/src/testing.rs` (behind the
  `testing` feature)
- Modify: `server/crates/application/Cargo.toml`, `server/Cargo.toml`
  (workspace deps `async-trait = "0.1"`)

**Interfaces:**
- Consumes: Task 1 types.
- Produces (in `ports`, re-exported at the crate root):

```rust
pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub trait SecretGenerator: Send + Sync { fn fill(&self, buf: &mut [u8]); }

pub struct NewPairingCode {
    pub code_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub created_by: Option<DeviceId>,
}
pub struct NewDevice { pub name: DeviceName, pub token_hash: String, pub created_at: DateTime<Utc> }

/// Opaque storage failure: the HTTP layer maps it to 500.
#[derive(Debug, thiserror::Error)]
#[error("repository: {0}")]
pub struct RepositoryError(pub Box<dyn std::error::Error + Send + Sync>);

#[async_trait::async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Also deletes the codes that are used or expired at `code.created_at`.
    async fn insert_pairing_code(&self, code: NewPairingCode) -> Result<(), RepositoryError>;
    /// In one transaction: marks the code used if it exists, is unused and
    /// `now < expires_at`, then inserts the device. `None` otherwise.
    async fn redeem_pairing_code(&self, code_hash: &str, now: DateTime<Utc>, device: NewDevice)
        -> Result<Option<Device>, RepositoryError>;
    /// Active (not revoked) devices only.
    async fn find_active_by_token_hash(&self, token_hash: &str) -> Result<Option<Device>, RepositoryError>;
    async fn touch_last_seen(&self, id: DeviceId, at: DateTime<Utc>) -> Result<(), RepositoryError>;
    /// Active devices, oldest `created_at` first, then by id.
    async fn list_active(&self) -> Result<Vec<Device>, RepositoryError>;
    /// `false` when the device is unknown or already revoked.
    async fn revoke(&self, id: DeviceId, at: DateTime<Utc>) -> Result<bool, RepositoryError>;
}
```

- Produces (in `application::testing`, feature `testing`, also enabled
  for the crate's own tests through `[dev-dependencies] application = {
  workspace = true, features = ["testing"] }` or `cfg(any(test,
  feature = "testing"))`):
  - `FixedClock` with `new(DateTime<Utc>)`, `set(&self, DateTime<Utc>)`,
    `advance(&self, TimeDelta)` (interior `Mutex`).
  - `SeqSecrets` filling each call with a counter byte (`0x01`, then
    `0x02`…), so successive tokens and codes differ and are predictable.
  - `InMemoryDevices` implementing `DeviceRepository` with a `Mutex`, and
    `touch_count(&self) -> usize` and `stored_token_hashes(&self) ->
    Vec<String>` for assertions.

- [ ] **Step 1: Write the ports and the doubles**; no behavior of their
  own to test beyond compiling and the `layering` test (async-trait is
  not an I/O crate).

- [ ] **Step 2: Run `mise run ci`**

Expected: PASS, including `layering`.

- [ ] **Step 3: Commit**

```bash
git add server/Cargo.toml server/Cargo.lock server/crates/ports server/crates/application
git commit -m "feat(domain): add the device repository, clock and secret ports"
```

(Scope `domain` covers the core crates; no `ports` scope exists.)

---

### Task 3: Device use cases

**Files:**
- Create: `server/crates/application/src/devices/{mod.rs,token.rs,create_pairing_code.rs,pair_device.rs,authenticate_device.rs,list_devices.rs,revoke_device.rs}`
- Modify: `server/crates/application/src/lib.rs`,
  `server/crates/application/Cargo.toml` (`sha2 = "0.10"`, `base64 =
  "0.22"`, `chrono`, `thiserror`, `async-trait`; dev: `tokio` with
  `macros, rt`)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces (`application::devices`, re-exported):

```rust
pub struct Deps { pub repo: Arc<dyn DeviceRepository>, pub clock: Arc<dyn Clock>, pub secrets: Arc<dyn SecretGenerator> }

/// A freshly issued bearer token; `Debug` prints `DeviceToken(..)`.
pub struct DeviceToken(String);   // as_str(&self) -> &str
pub fn hash_token(token: &str) -> String;          // hex SHA-256
pub fn hash_pairing_code(code: &PairingCode) -> String; // hex SHA-256 of as_str()
pub fn looks_like_token(input: &str) -> bool;      // "um_" + 43 base64url chars

pub struct CreatePairingCode(Deps);
impl CreatePairingCode { pub fn new(deps: Deps) -> Self;
    pub async fn execute(&self, created_by: Option<DeviceId>) -> Result<CreatedPairingCode, RepositoryError>; }
pub struct CreatedPairingCode { pub code: PairingCode, pub expires_at: DateTime<Utc> }

pub struct PairDevice(Deps);
impl PairDevice { pub fn new(deps: Deps) -> Self;
    pub async fn execute(&self, code: &str, name: &str) -> Result<PairedDevice, PairError>; }
pub struct PairedDevice { pub device: Device, pub token: DeviceToken }
pub enum PairError { InvalidName(DeviceNameError), InvalidCode, Repository(RepositoryError) }

pub struct AuthenticateDevice(Deps);
impl AuthenticateDevice { pub fn new(deps: Deps) -> Self;
    pub async fn execute(&self, token: &str) -> Result<Option<DeviceId>, RepositoryError>; }

pub struct ListDevices(Deps);   // execute(&self) -> Result<Vec<Device>, RepositoryError>
pub struct RevokeDevice(Deps);  // execute(&self, id: DeviceId) -> Result<bool, RepositoryError>
```

Rules: `PairDevice` validates the name first (an invalid name is
`InvalidName` even with a bad code), then parses the code (a malformed
code is `InvalidCode` without a lookup), then draws 32 bytes for the
token. `AuthenticateDevice` returns `None` without a lookup when
`looks_like_token` is false, touches `last_seen_at` when it is `None` or
`now - last_seen_at >= LAST_SEEN_RESOLUTION`. `CreatePairingCode` draws
5 bytes, `expires_at = now + PAIRING_CODE_TTL`.

- [ ] **Step 1: Write the failing tests** (`#[tokio::test]`, with
  `InMemoryDevices`, `FixedClock` at `2026-01-01T00:00:00Z`,
  `SeqSecrets`):

  - `pair_then_authenticate`: create a code, pair with its `Display`
    form lower-cased, `authenticate(token)` → `Some(device.id)`.
  - `token_shape`: `token.as_str()` starts with `um_`, length 46,
    `looks_like_token` true; `looks_like_token("um_short")` false.
  - `token_not_stored_in_clear`: after pairing,
    `stored_token_hashes() == vec![hash_token(token.as_str())]` and none
    equals the token.
  - `code_valid_until_ttl`: advance 9 min 59 s → pairing works; with a
    new code, advance exactly 10 min → `PairError::InvalidCode`.
  - `code_single_use`: second pairing with the same code →
    `InvalidCode`.
  - `unknown_and_malformed_code`: `"0000-0000"` (never created) and
    `"nope"` → `InvalidCode`.
  - `invalid_name_wins`: name `"  "` with a malformed code →
    `InvalidName(DeviceNameError::Empty)`.
  - `revoked_device_does_not_authenticate`: revoke → `true`;
    authenticate → `None`; revoke again → `false`; revoke
    `DeviceId::new(999)` → `false`.
  - `last_seen_throttled`: authenticate twice at the same instant →
    `touch_count() == 1`; advance 59 s → still 1; advance to 60 s → 2.
  - `list_returns_active_only`: pair two devices, revoke the first →
    list has the second only.
  - `malformed_token_skips_lookup`: `authenticate("Bearer x")` → `None`
    and `touch_count() == 0`.

- [ ] **Step 2: Run them and see them fail**

Run: `cd server && cargo test -p application devices`
Expected: compile errors.

- [ ] **Step 3: Implement the use cases** with the signatures above.

- [ ] **Step 4: Run them and see them pass**

Run: `cd server && cargo test -p application devices`
Expected: PASS.

- [ ] **Step 5: `mise run ci`, then commit**

```bash
git commit -am "feat(domain): add the device pairing and authentication use cases"
```

(Add the new files first: `git add server/crates/application`.)

---

### Task 4: SQLite device repository

**Files:**
- Create: `server/crates/adapters/sqlite/migrations/0002_devices.sql`,
  `server/crates/adapters/sqlite/src/devices.rs`
- Modify: `server/crates/adapters/sqlite/src/lib.rs`,
  `server/crates/adapters/sqlite/src/store.rs` (`close(&self)` instead of
  `close(self)`), `server/crates/adapters/sqlite/Cargo.toml` (`domain`,
  `ports`, `async-trait` as normal deps)

**Interfaces:**
- Consumes: Task 2 `DeviceRepository`, `NewDevice`, `NewPairingCode`.
- Produces: `impl DeviceRepository for Store`; `Store::close(&self)`.

Schema:

```sql
CREATE TABLE devices (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (name <> ''),
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_seen_at TEXT,
    revoked_at TEXT
);
CREATE TABLE pairing_codes (
    code_hash TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_by INTEGER REFERENCES devices (id),
    used_at TEXT
);
```

Times through `crate::time::to_db` / `from_db` (they sort as text).
Reads (`find_active_by_token_hash`, `list_active`) use the read pool,
writes the write pool. `redeem_pairing_code` is one transaction: an
`UPDATE pairing_codes SET used_at = ? WHERE code_hash = ? AND used_at IS
NULL AND expires_at > ?` whose `rows_affected() == 1` gates the
`INSERT INTO devices … RETURNING`. A stored row that fails
`DeviceName::new` or `from_db` is a `RepositoryError`.

- [ ] **Step 1: Write the failing tests** in `devices.rs`
  (`#[tokio::test]`, store opened in a `tempdir`):

  - `migration_creates_tables`: both tables exist after `Store::open`.
  - `redeem_once`: insert a code expiring in 10 min, redeem → `Some`,
    redeem again → `None`.
  - `redeem_expired_or_unknown`: `now == expires_at` → `None`; unknown
    hash → `None`.
  - `concurrent_redeem_yields_one_device`: two `redeem_pairing_code`
    calls joined with `tokio::join!` on the same code → exactly one
    `Some`, and `list_active().len() == 1`.
  - `insert_purges_used_and_expired`: a used code and an expired code
    are gone after inserting a new one (count rows of `pairing_codes`).
  - `find_skips_revoked`: revoke → `find_active_by_token_hash` → `None`;
    `revoke` again → `false`.
  - `touch_and_list_order`: two devices created at t1 < t2 →
    `list_active` order t1, t2; `touch_last_seen` shows in the result.
  - `created_by_kept`: a code created by a device stores its id.

- [ ] **Step 2: Run them and see them fail**

Run: `cd server && cargo test -p sqlite devices`
Expected: compile errors.

- [ ] **Step 3: Implement the migration and the repository**; update
  `main.rs` call site if `close` is used (it is not yet).

- [ ] **Step 4: Run them and see them pass**, then `mise run ci`.

- [ ] **Step 5: Commit**

```bash
git add server/crates/adapters/sqlite server/Cargo.lock
git commit -m "feat(sqlite): store devices and pairing codes"
```

---

### Task 5: `http` crate, errors, health and the contract

**Files:**
- Create: `server/crates/http/Cargo.toml`, `server/crates/http/src/lib.rs`,
  `src/error.rs`, `src/extract.rs`, `src/health.rs`, `src/state.rs`,
  `src/bin/openapi.rs`, `tests/contract.rs`, `tests/errors.rs`
- Create: `api/openapi.json` (generated)
- Modify: `server/Cargo.toml` (member `crates/http`, workspace deps
  `axum = "0.8"`, `tower-http = { version = "0.6", features =
  ["trace", "set-header"] }`, `utoipa = { version = "5", features =
  ["chrono"] }`, `utoipa-axum = "0.2"`, `serde = { version = "1",
  features = ["derive"] }`, `serde_json = "1"`, `http = { path =
  "crates/http" }`), `mise.toml` (task `openapi`)

**Interfaces:**
- Consumes: Task 3 use cases.
- Produces:

```rust
#[derive(Clone)]
pub struct AppState {
    pub create_pairing_code: Arc<CreatePairingCode>,
    pub pair_device: Arc<PairDevice>,
    pub authenticate_device: Arc<AuthenticateDevice>,
    pub list_devices: Arc<ListDevices>,
    pub revoke_device: Arc<RevokeDevice>,
}
impl AppState { pub fn new(deps: application::Deps) -> AppState; }

pub fn router(state: AppState) -> axum::Router;          // the whole app, layers included
pub fn openapi() -> utoipa::openapi::OpenApi;           // same OpenApiRouter as router()

pub enum ErrorCode { InvalidPairingCode, Unauthorized, NotFound, MethodNotAllowed,
                     PayloadTooLarge, UnsupportedMediaType, InvalidRequest, Internal }
                     // serde + ToSchema, snake_case
pub struct ApiError { status: StatusCode, code: ErrorCode, message: String }
impl ApiError { pub fn new(code: ErrorCode, message: impl Into<String>) -> ApiError; } // status from code
impl IntoResponse for ApiError;  // adds WWW-Authenticate: Bearer on 401
impl From<RepositoryError> for ApiError;  // logs the cause with tracing::error!, returns Internal "internal error"

pub struct ApiJson<T>(pub T);   // FromRequest via axum::Json, rejection mapped:
                                // 413 → payload_too_large, 415 → unsupported_media_type,
                                // anything else (syntax and data errors) → invalid_request
pub struct ApiPath<T>(pub T);   // FromRequestParts via axum::extract::Path, any rejection → not_found
```

Router assembly (in `lib.rs`): one `fn api() ->
OpenApiRouter<AppState>` nested at `/api/v1`, `split_for_parts()` gives
both the router and the document. `router()` adds a `fallback` and
`method_not_allowed_fallback` returning `ApiError` (`not_found`,
`method_not_allowed`), `SetResponseHeaderLayer::overriding` for the two
headers, and `TraceLayer::new_for_http()` (default span: no headers).
The OpenAPI info: title `Unified Mailbox API`, version from
`CARGO_PKG_VERSION`. A `Modify` adds the `bearer` HTTP security scheme
and sets it as the global requirement; `/health` and `/pairing` declare
`security(())`.

`GET /api/v1/health` → `200 {"status": "ok", "version": "<CARGO_PKG_VERSION>"}`
(`HealthDto`, `status` a one-value enum `ok`).

`src/bin/openapi.rs` prints `openapi().to_pretty_json()` and a newline.
`mise.toml`:

```toml
[tasks.openapi]
description = "Regenerate api/openapi.json from the http crate"
dir = "{{config_root}}/server"
run = "cargo run -q -p http --bin openapi > ../api/openapi.json"
```

- [ ] **Step 1: Write the failing tests**

`tests/errors.rs` (router built with `AppState::new` over
`application::testing` doubles, driven by `tower::ServiceExt::oneshot`):

  - `health_without_token`: 200, JSON `status == "ok"`, `version ==
    env!("CARGO_PKG_VERSION")`.
  - `unknown_route_404`: `GET /api/v1/nope` → 404, body `error.code ==
    "not_found"`.
  - `wrong_method_405`: `DELETE /api/v1/health` → 405
    `method_not_allowed`.
  - `security_headers_on_success_and_error`: both responses above carry
    `cache-control: no-store` and `x-content-type-options: nosniff`.

`tests/contract.rs`:

  - `contract_is_current`: `serde_json::from_str` of
    `concat!(env!("CARGO_MANIFEST_DIR"), "/../../../api/openapi.json")`
    equals `serde_json::to_value(openapi())`; the failure message says
    `run: mise run openapi`.
  - `public_routes_declare_no_security`: in `openapi()`, `/api/v1/health`
    has `security: [{}]`-equivalent (empty requirement), the document's
    global security is `bearer`.

- [ ] **Step 2: Run them and see them fail**

Run: `cd server && cargo test -p http`
Expected: compile errors.

- [ ] **Step 3: Implement** the crate as above (no device routes yet).

- [ ] **Step 4: Generate the contract and run the tests**

Run: `mise run openapi && cd server && cargo test -p http`
Expected: `api/openapi.json` created; PASS.

- [ ] **Step 5: `mise run ci`, then commit**

```bash
git add server/Cargo.toml server/Cargo.lock server/crates/http mise.toml api/openapi.json
git commit -m "feat(http): serve /api/v1 with one error format and generate the contract"
```

---

### Task 6: Device routes and bearer authentication

**Files:**
- Create: `server/crates/http/src/auth.rs`, `src/devices.rs`,
  `tests/devices.rs`, `tests/auth.rs`
- Modify: `server/crates/http/src/lib.rs`, `api/openapi.json`
  (regenerated)

**Interfaces:**
- Consumes: Task 5 `AppState`, `ApiError`, `ApiJson`, `ApiPath`.
- Produces:
  - `pub struct CurrentDevice(pub DeviceId)` implementing
    `FromRequestParts<AppState>`: reads `Authorization`, requires the
    scheme `Bearer` (ASCII case-insensitive) followed by one or more
    spaces and the token; calls `AuthenticateDevice`; any failure →
    `ApiError` `unauthorized` (message `missing or invalid bearer
    token`), a repository error → `internal`.
  - Routes (handlers in `devices.rs`, all with `#[utoipa::path]`):

| Handler | Route | Extractors | Answer DTO |
|---|---|---|---|
| `pair` | `POST /pairing` | `ApiJson<PairingRequest { code: String, name: String }>` | `201 PairingResponse { device_id: i64, token: String }` |
| `list` | `GET /devices` | `CurrentDevice` | `200 Vec<DeviceDto { id: i64, name: String, created_at: DateTime<Utc>, last_seen_at: Option<DateTime<Utc>>, current: bool }>` |
| `create_code` | `POST /devices/pairing-codes` | `CurrentDevice` | `201 PairingCodeDto { code: String /* XXXX-XXXX */, expires_at: DateTime<Utc> }` |
| `revoke` | `DELETE /devices/{id}` | `CurrentDevice`, `ApiPath<i64>` | `204`, or 404 `not_found` |

  `PairError::InvalidCode` → 400 `invalid_pairing_code`;
  `InvalidName(_)` → 422 `invalid_request` (message names the field
  `name` and the rule). Every handler documents its error responses with
  `ErrorBody`.

- [ ] **Step 1: Write the failing tests**

`tests/auth.rs`:

  - `every_protected_route_requires_a_token`: for each path and method
    of `openapi()` other than `/api/v1/health` and `/api/v1/pairing`,
    a request without `Authorization` gets 401 with `www-authenticate:
    Bearer` and `error.code == "unauthorized"` (path parameters filled
    with `1`). This test fails if a future route forgets
    `CurrentDevice`.
  - `malformed_authorization_headers`: on `GET /api/v1/devices`,
    `"Basic abc"`, `"Bearer"`, `"Bearer "`, `"Token <valid>"`, `"<valid>"`
    alone, `"Bearer <valid minus last char>"` → 401 each.
  - `lowercase_scheme_and_extra_spaces_accepted`: `"bearer <valid>"` and
    `"Bearer   <valid>"` → 200.
  - `revoked_token_401`.

`tests/devices.rs`:

  - `pair_flow`: create a code through the use case (first device),
    `POST /pairing {"code": "<lower-case display form>", "name": " Phone "}`
    → 201 with a token starting `um_`; `GET /devices` with it → one
    device named `Phone`, `current: true`, `last_seen_at` set,
    `created_at` ending in `Z`.
  - `create_code_from_device`: `POST /devices/pairing-codes` → 201,
    `code` matches `^[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$`,
    `expires_at` = clock + 10 min; pairing a second device with it
    works; the list shows `current` true only for the caller.
  - `pair_invalid_code_400`: unknown code → 400
    `invalid_pairing_code`; the same code used twice → 400 the second
    time.
  - `pair_invalid_name_422`: name of 65 characters → 422
    `invalid_request`.
  - `pair_malformed_body`: `{"code": 1}` → 422; `not json` with
    `content-type: application/json` → 422; valid JSON without
    content type → 415; a body over 2 MiB → 413.
  - `revoke_other_and_self`: revoke the other device → 204; revoke it
    again → 404; revoke self → 204, then `GET /devices` → 401.
  - `revoke_bad_ids_404`: `/devices/abc`, `/devices/99999999999999999999`
    → 404 `not_found`.
  - `token_never_in_list`: the `GET /devices` body does not contain the
    token string.

- [ ] **Step 2: Run them and see them fail**

Run: `cd server && cargo test -p http`
Expected: the new tests fail (routes missing → 404).

- [ ] **Step 3: Implement `auth.rs` and `devices.rs`**, register the
  routes in `api()`.

- [ ] **Step 4: Regenerate and run**

Run: `mise run openapi && cd server && cargo test -p http`
Expected: PASS, `contract_is_current` included.

- [ ] **Step 5: `mise run ci`, then commit**

```bash
git add server/crates/http api/openapi.json
git commit -m "feat(api): pair, list and revoke devices with bearer tokens"
```

---

### Task 7: `app` serves, `LISTEN_ADDR` and `pair-code`

**Files:**
- Modify: `server/crates/app/src/config.rs` (field `listen_addr:
  SocketAddr`, parser `socket_addr`), `server/crates/app/src/main.rs`,
  `server/crates/app/Cargo.toml` (`http`, `chrono`, `getrandom`,
  `axum`), `server/Cargo.toml` (tokio features `net`, `signal`)
- Create: `server/crates/app/src/cli.rs`, `server/crates/app/src/system.rs`
- Modify: `server/crates/app/tests/startup.rs`
- Modify: `README.md`, `.env.example`

**Interfaces:**
- Consumes: Task 4 `impl DeviceRepository for Store`, `Store::close(&self)`;
  Task 5 `http::AppState::new`, `http::router`; Task 3 `Deps`,
  `CreatePairingCode`.
- Produces:
  - `system::SystemClock` (`chrono::Utc::now`), `system::OsSecrets`
    (`getrandom::fill`, panics on failure: the OS has no entropy).
  - `cli::Command { Serve, PairCode }`, `cli::parse(args: impl
    IntoIterator<Item = OsString>) -> Result<Command, String>` (the
    iterator excludes `argv[0]`); `cli::USAGE: &str`:

```text
usage: unified-mailbox [pair-code]

  (no argument)  serve the API on LISTEN_ADDR
  pair-code      print a one-time code to pair a new device, then exit
```

`main` flow: read config → logging → `prepare_dirs` → `Store::open` →
`Arc<Store>` → `Deps`; then either `serve` (bind
`tokio::net::TcpListener` on `listen_addr`; on error print
`LISTEN_ADDR: cannot listen on <addr>: <err>` to stderr and exit 1; log
`listening` with the address; `axum::serve(...)
.with_graceful_shutdown(signal)` where the signal is SIGINT or SIGTERM;
then `store.close().await`, exit 0) or `pair-code` (execute
`CreatePairingCode` with `None`, print to stdout:

```text
Pairing code: 04HM-ASW9
Valid until 2026-01-01T00:10:00Z (10 minutes, single use).
```

then close the store, exit 0). A parse error prints the message and
`USAGE` to stderr, exit 2.

- [ ] **Step 1: Write the failing tests**

`config.rs` tests: `assert_defaults` gains `listen_addr ==
"127.0.0.1:8000".parse().unwrap()`; `listen_addr_parsed`
(`"[::1]:9000"`); `listen_addr_invalid` (`"localhost"` and
`"127.0.0.1"` without a port each give one error naming `LISTEN_ADDR`).

`cli.rs` tests: `[]` → `Serve`; `["pair-code"]` → `PairCode`;
`["pair-code", "x"]`, `["serve"]`, `["--help"]` → `Err`.

`tests/startup.rs` (the binary; a free port found by binding
`127.0.0.1:0` and dropping the listener; raw HTTP/1.1 over
`std::net::TcpStream`, a small helper `request(addr, method, path,
headers, body) -> (u16, String)`; the server stopped with `kill -TERM
<pid>` through `std::process::Command`):

  - replace `first_start_creates_the_store` with
    `serves_until_sigterm`: spawn with `DATA_DIR=./d`,
    `LISTEN_ADDR=127.0.0.1:<port>`, `RUST_LOG=off`; poll
    `GET /api/v1/health` up to 10 s → 200; the database and key file
    exist; SIGTERM → exit status success within 10 s.
  - `pair_code_while_serving`: with the server above running, run
    `unified-mailbox pair-code` in the same directory → exit 0, stdout
    matches `Pairing code: XXXX-XXXX`; `POST /api/v1/pairing` with that
    code → 201; SIGTERM.
  - `pair_code_on_fresh_install`: in an empty directory, `pair-code`
    alone → exit 0 and creates `d/unified-mailbox.db` and
    `d/secret.key`.
  - `port_in_use`: hold a `TcpListener` on a port, start the server on
    it → exit 1, stderr contains `LISTEN_ADDR`.
  - `unknown_argument`: `unified-mailbox serve` → exit 2, stderr
    contains `usage:`.
  - keep `invalid_variables_listed` and
    `fatal_error_shown_when_logs_filter_it_out` unchanged.

- [ ] **Step 2: Run them and see them fail**

Run: `cd server && cargo test -p app`
Expected: failures (field, module and behavior missing).

- [ ] **Step 3: Implement** `config.rs`, `cli.rs`, `system.rs`,
  `main.rs`.

- [ ] **Step 4: Run them and see them pass**

Run: `cd server && cargo test -p app`
Expected: PASS.

- [ ] **Step 5: Documentation**

  - README `Usage and configuration`: replace "For now it reads its
    configuration, opens the store and exits" with: the server serves
    the API on `LISTEN_ADDR` until SIGTERM or Ctrl-C; a
    `LISTEN_ADDR` row in the table (default `127.0.0.1:8000`, a socket
    address; behind Tailscale use `tailscale serve` or the tailnet
    address, in Docker `0.0.0.0:8000` with the port published on the
    tailnet interface only).
  - README new section `### Pairing a device`: every route but
    `/api/v1/health` and `/api/v1/pairing` needs `Authorization: Bearer
    <token>`; first device: `cargo run -p app -- pair-code` (or
    `unified-mailbox pair-code` in the container), then `POST
    /api/v1/pairing` with the code and a name, a copy-pasteable `curl`
    example with an invented code; later devices: `POST
    /api/v1/devices/pairing-codes` from a paired one; revocation with
    `DELETE /api/v1/devices/{id}`; the token is shown once; losing every
    device: run `pair-code` again.
  - README `Development`: `mise run openapi` regenerates
    `api/openapi.json`; `mise run ci` fails while it is stale.
  - `.env.example`: a commented `# LISTEN_ADDR=127.0.0.1:8000` with a
    one-line explanation.

- [ ] **Step 6: `mise run ci`, then commit**

```bash
git add server README.md .env.example
git commit -m "feat(app): serve the API, add LISTEN_ADDR and the pair-code command"
```

Footer of this last commit: `Closes: #13`.
