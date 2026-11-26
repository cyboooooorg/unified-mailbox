# Client Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A SvelteKit web client in `client/`, typed from `api/openapi.json`, that pairs a device, keeps its token and stays live on the event stream, served by the server from an optional `CLIENT_DIR`.

**Architecture:** The server gains a static fallback behind `/api` (axum + `tower-http` `ServeDir`), configured by `CLIENT_DIR`. The client is a SvelteKit SPA (`adapter-static`, `ssr = false`) whose types come from `openapi-typescript`, whose calls go through `openapi-fetch`, and whose event stream reader is framework-free TypeScript tested with a fake `fetch`.

**Tech Stack:** Rust (axum, tower-http `fs`), Node LTS via mise, SvelteKit 2 + Svelte 5 (runes), TypeScript, `openapi-typescript`, `openapi-fetch`, `eventsource-parser`, Vitest, ESLint, Prettier.

**Spec:** `docs/superpowers/specs/2026-11-26-client-skeleton-design.md`

## Global Constraints

- No token in a URL, in a log, or sent anywhere but same-origin `/api`.
- No hand-written copy of a type from `api/openapi.json`; events use the generated `EventDto`.
- An unknown `/api` or `/api/...` path stays the JSON `not_found`, even with `CLIENT_DIR` set.
- Never `'unsafe-inline'` for scripts; `frame-ancestors 'none'` sent by the server on `index.html`.
- Reconnect delay 3000 ms until the server sends `retry`; watchdog 45 000 ms without a byte.
- Device name: trimmed, 1 to 64 characters, no control character.
- Cache: `/_app/immutable/*` gets `public, max-age=31536000, immutable`; every other static response `no-cache`; API responses keep `no-store`.
- Test data is invented; no test touches the network.
- Every commit follows Conventional Commits with the session's attribution lines; scopes `app`, `http`, `client`, `ci`.
- Each task ships its documentation (README, `.env.example`, `client/README.md`, CLAUDE.md) in the same commit.
- The implementer runs `mise run ci` before each commit (from Task 3 on it also runs the client checks).

## Review Focus

1. An event split across two network chunks, or two events in one chunk: each is delivered once, whole. (Task 5 test `delivers events split across chunks`.)
2. The token was cleared (sign-out in another tab) while the reader waited to reconnect: it must call `onUnauthorized` without sending a request with no token. (Task 5 test `stops without a request when the token is gone`.)
3. A deep link with a query string (`/jobs?x=1`) or a `HEAD` request still gets `index.html`. (Task 2 tests.)
4. `localStorage` that throws (disabled storage, private mode): the app treats it as no token instead of crashing. (Task 4 test `get returns null when storage throws`.)
5. A user agent that yields a name over 64 characters, or an empty one: the suggested name is cut to 64 and falls back to `Web browser`. (Task 6 tests.)

---

### Task 1: `CLIENT_DIR` configuration

**Files:**
- Modify: `server/crates/app/src/config.rs` (field, parser method, tests)
- Modify: `.env.example`, `README.md` (configuration table)

**Interfaces:**
- Produces: `Config.client_dir: Option<PathBuf>`; a `ConfigError { variable: "CLIENT_DIR", .. }` when invalid.

- [ ] **Step 1: Write the failing tests** in `config.rs`'s test module, with `tempfile::tempdir()`:
  - `client_dir_unset_or_empty_is_none`: no variable → `None`; `("CLIENT_DIR", "  ")` → `None`.
  - `client_dir_valid`: a temp dir holding `index.html` → `Some(path)` (value trimmed).
  - `client_dir_missing`, `client_dir_is_a_file`, `client_dir_without_index`: each → one error with `variable == "CLIENT_DIR"` whose message names the problem (`does not exist`, `is not a directory`, `has no index.html`) and quotes only the path.
  - `assert_defaults` also asserts `client_dir == None`.
- [ ] **Step 2: Run** `cargo test -p app config` from `server/`. Expected: compile failure (`client_dir` unknown).
- [ ] **Step 3: Implement** `Parser::client_dir(&mut self, name: &'static str) -> Option<PathBuf>` with `std::fs::metadata` (`is_dir`, then `join("index.html").is_file()`), errors through `self.fail`; add `client_dir` to `Config` and `from_vars`.
- [ ] **Step 4: Run** `cargo test -p app config`. Expected: PASS.
- [ ] **Step 5: Document**: `.env.example` gains a commented `# CLIENT_DIR=` block ("Directory of the built web client, served on every path outside /api. Unset: the API only."); the README table gains the row `CLIENT_DIR | empty | ...`, saying it must hold `index.html` (Task 6 links it to the web client section it adds).
- [ ] **Step 6: Commit** `feat(app): read CLIENT_DIR, the web client directory` (`Refs: #15`).

### Task 2: Serve the client next to the API

**Files:**
- Modify: `server/Cargo.toml` (`tower-http` features: add `fs`)
- Create: `server/crates/http/src/client.rs`
- Modify: `server/crates/http/src/lib.rs:81-111`, `server/crates/app/src/main.rs:120`
- Test: `server/crates/http/tests/client.rs`

**Interfaces:**
- Consumes: `Config.client_dir` (Task 1).
- Produces: `pub struct ClientDir(PathBuf)` with `ClientDir::new(path: PathBuf) -> ClientDir` (no I/O: `app` validated it); `pub fn router_with_client(state: AppState, client: Option<ClientDir>) -> Router`. `router(state)` stays and calls it with `None`, so existing tests and the contract binary do not change.

Routing and headers are the spec's "Server: serving the client" section. Implementation constraints:
- The API's `Cache-Control: no-store` layer becomes `SetResponseHeaderLayer::if_not_present`, and the static handler sets its own `Cache-Control`, so static responses are not overridden.
- The current JSON fallback becomes a handler that sends `/api` and `/api/...` to the JSON `not_found`, then (with a client dir) non-`GET`/`HEAD` to the JSON `method_not_allowed`, then `ServeDir`; on its 404, a last path segment without `.` gets `index.html`.
- `index.html` (served for `/`, `/index.html` or a fallback) gets `Content-Security-Policy: frame-ancestors 'none'; object-src 'none'; base-uri 'none'` (the script hashes come from SvelteKit's `<meta>`, Task 3) and `Referrer-Policy: no-referrer`; every static response gets `Referrer-Policy: no-referrer` and `nosniff`.

- [ ] **Step 1: Write the failing tests** in `tests/client.rs`, building `router_with_client` over a `tempdir` with invented files `index.html` (`<p>app</p>`), `_app/immutable/app.abc123.js`, `robots.txt`, reusing `tests/common` for the state and `send`:
  - `root_serves_index`: `GET /` → 200, body `<p>app</p>`, `cache-control: no-cache`, CSP header equal to the exact value above, `referrer-policy: no-referrer`.
  - `deep_link_serves_index`: `/jobs/42` and `/jobs?x=1` → 200 with the index body.
  - `head_deep_link`: `HEAD /settings` → 200, empty body.
  - `immutable_asset`: `/_app/immutable/app.abc123.js` → 200, `content-type` JavaScript, `cache-control: public, max-age=31536000, immutable`.
  - `other_asset_no_cache`: `/robots.txt` → 200, `no-cache`, no CSP header.
  - `missing_asset_is_404`: `/_app/immutable/gone.js` and `/favicon.ico` → 404, body not the index.
  - `api_paths_stay_json`: `/api`, `/api/unknown`, `/api/v2/x` → 404 JSON with `error.code == "not_found"`; `GET /api/v1/health` still 200 with `no-store`.
  - `post_root_is_405`: `POST /` → 405 JSON `method_not_allowed`.
  - `no_escape`: `/../secret.txt` and `/%2e%2e/secret.txt` (with `secret.txt` in the tempdir's parent) → never 200 with its content.
  - `without_client_dir`: `router(state)` → `GET /` is the JSON 404.
- [ ] **Step 2: Run** `cargo test -p http-api --test client`. Expected: compile failure.
- [ ] **Step 3: Implement** `client.rs` and `router_with_client` per the constraints above; in `main.rs`, `http_api::router_with_client(state, config.client_dir.clone().map(ClientDir::new))`, and log `client_dir` at startup when set.
- [ ] **Step 4: Run** `cargo test -p http-api` (the whole crate: `contract_is_current` must still pass, the document is unchanged). Expected: PASS.
- [ ] **Step 5: Run** `mise run ci`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(http): serve the web client outside /api` (`Refs: #15`).

### Task 3: Client scaffold, contract types and CI

**Files:**
- Create: `client/` (SvelteKit minimal template, TypeScript, ESLint, Prettier, Vitest), `client/svelte.config.js`, `client/vite.config.ts`, `client/src/routes/+layout.ts`, `client/src/routes/+page.svelte` (placeholder), `client/src/lib/api/schema.d.ts` (generated), `client/scripts/check-contract.mjs`, `client/scripts/check-csp.mjs`
- Modify: `mise.toml`, `.github/workflows/server.yml`, `.gitignore`, `CLAUDE.md` (Checks), `README.md` (Development)

**Interfaces:**
- Produces: `import type { paths, components } from '$lib/api/schema'`; npm scripts `dev`, `build`, `generate`, `check`, `check:contract`, `check:csp`, `lint`, `format`, `test` (`vitest run`: one run, no watch, so `npm test` and `npm test -- <path>` work everywhere).

- [ ] **Step 1: Scaffold** with `npx sv create client` run non-interactively (minimal template, TypeScript; add ESLint, Prettier and Vitest unit tests with `npx sv add`; check `--help` for the flags), then pin Node LTS in `mise.toml` (`node = "<current LTS>"`), commit `package-lock.json`.
- [ ] **Step 2: Configure** `adapter-static` with `fallback: 'index.html'`; `+layout.ts` exports `ssr = false` and `prerender = false`; `kit.csp = { mode: 'hash', directives: { 'default-src': ['self'], 'script-src': ['self'], 'connect-src': ['self'], 'img-src': ['self', 'data:'], 'object-src': ['none'], 'base-uri': ['none'], 'form-action': ['self'] } }`; Vite dev proxy `/api` → `process.env.API_PROXY_TARGET ?? 'http://127.0.0.1:8000'`.
- [ ] **Step 3: Contract scripts**: `generate` runs `openapi-typescript ../api/openapi.json -o src/lib/api/schema.d.ts`; `check-contract.mjs` generates into a temp file and exits 1 with "schema.d.ts is stale: run npm run generate" when it differs from the committed one. `schema.d.ts` is listed in `.prettierignore` and the ESLint ignores: it stays exactly as generated, so `lint` and `check:contract` cannot disagree.
- [ ] **Step 4: Verify the contract check** fails when stale: edit one character in `schema.d.ts`, run `npm run check:contract` → exit 1; restore, run → exit 0.
- [ ] **Step 5: CSP check**: `check-csp.mjs` reads `build/index.html` and exits 1 unless it holds a `Content-Security-Policy` `<meta>` whose `script-src` has at least one `'sha256-` source and no `'unsafe-inline'`. Run `npm run build && npm run check:csp`. Expected: exit 0. If SvelteKit emits no hash for the fallback page, stop and report: the spec's alternative (build-written hashes sent by the server) needs a design note.
- [ ] **Step 6: Wire the checks**: in `mise.toml`, split into `ci:server` (today's commands) and `ci:client` (`dir = client`: `npm run check:contract`, `npm run check`, `npm run lint`, `npm test`, `npm run build`, `npm run check:csp`), with `ci` depending on both; the workflow keeps `jdx/mise-action` (it now installs the pinned Node too; no `actions/setup-node`), adds an `actions/cache` step for `~/.npm` keyed on `client/package-lock.json`, and runs `npm ci` in `client/` before `mise run ci`; `.gitignore` adds `/client/node_modules/`, `/client/build/`, `/client/.svelte-kit/`.
- [ ] **Step 7: Document**: CLAUDE.md "Checks" says `mise run ci` now runs the client checks (after `npm ci` in `client/`); README Development gains the client part (Node through mise, `npm ci`, `npm run dev` with the server running, `npm run generate` after `mise run openapi`).
- [ ] **Step 8: Run** `mise run ci` from the repository root. Expected: PASS.
- [ ] **Step 9: Commit** `feat(client): scaffold the SvelteKit client and its checks` (`Refs: #15`).

### Task 4: Token store, session, API client and error messages

**Files:**
- Create: `client/src/lib/auth/token-store.ts`, `client/src/lib/auth/session.svelte.ts`, `client/src/lib/api/client.ts`, `client/src/lib/api/app.ts`, `client/src/lib/api/errors.ts`
- Test: `client/src/lib/auth/token-store.test.ts`, `client/src/lib/api/client.test.ts`, `client/src/lib/api/errors.test.ts`

**Interfaces:**
- Consumes: `paths`, `components` (Task 3).
- Produces:
  - `interface TokenStore { get(): string | null; set(token: string): void; clear(): void }`; `localTokenStore(storage?: () => Storage): TokenStore` (key `unified-mailbox.token`; the default reads `localStorage` lazily, inside the `try`, since merely accessing it can throw).
  - In `session.svelte.ts`: `createSession(tokens: TokenStore, navigate: (path: string) => void): Session` with `Session = { readonly signedIn: boolean; readonly tokens: TokenStore; signIn(token: string): void; signOut(): void }` (`signedIn` is `$state`; `signOut` clears the token and navigates to `/pair`), and the app-wide `session = createSession(localTokenStore(), goto)`.
  - In `client.ts` (no `$app` import, so it tests without SvelteKit): `API_BASE = ''` (same origin; #58 changes it); `createApi(opts: { fetch?: typeof fetch; tokens: TokenStore; onUnauthorized: () => void }) => Client<paths>`.
  - In `app.ts`: `api = createApi({ tokens: session.tokens, onUnauthorized: () => session.signOut() })`.
  - `type ErrorCode = components['schemas']['ErrorCode']`; `errorMessage(code: ErrorCode): string` (a `Record<ErrorCode, string>`, so a new code fails `npm run check`); `describeError(body: unknown): string` returns the message for a well-formed `{error: {code}}` and `GENERIC_ERROR = 'Something went wrong. Try again.'` otherwise. `invalid_pairing_code` → `This code is invalid or expired.`

- [ ] **Step 1: Write the failing tests**:
  - token store with a fake `Storage`: `set` then `get` returns the token; `clear` → `null`; `get returns null when storage throws` (a fake whose `getItem` throws); `set` on a throwing storage does not throw.
  - `createApi` with a fake `fetch`: `adds the bearer header` when a token exists; `no header without a token`; `a 401 clears the token and calls onUnauthorized` on `GET /api/v1/devices`; `a 401 from /api/v1/pairing does not sign out`; the request URL is `/api/v1/...` (no token in it).
  - `describeError`: the pairing code message; an unknown shape (`null`, `{}`, `{error: "x"}`) → `GENERIC_ERROR`.
- [ ] **Step 2: Run** `npm test` in `client/`. Expected: FAIL (modules missing).
- [ ] **Step 3: Implement** the five modules; the auth middleware is an `openapi-fetch` `Middleware` with `onRequest` (header) and `onResponse` (401 handling).
- [ ] **Step 4: Run** `npm test && npm run check`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(client): add the token store and the typed API client` (`Refs: #15`).

### Task 5: Event stream reader (hard: concurrency and timers, run on opus)

**Files:**
- Create: `client/src/lib/events/stream.ts`
- Test: `client/src/lib/events/stream.test.ts`

**Interfaces:**
- Consumes: `components['schemas']['EventDto']`, `TokenStore`, `API_BASE` (Task 4).
- Produces:

```ts
export type StreamState = 'connecting' | 'connected' | 'reconnecting' | 'stopped';
export interface EventStreamOptions {
  tokens: TokenStore;
  onConnect: () => void;             // reload what is shown
  onEvent: (event: EventDto) => void;
  onUnauthorized: () => void;        // session.signOut
  onState?: (state: StreamState) => void;
  signal: AbortSignal;
  fetch?: typeof fetch;
  setTimeout?: typeof setTimeout;    // injected for fake timers
  clearTimeout?: typeof clearTimeout;
}
export const DEFAULT_RETRY_MS = 3000;
export const WATCHDOG_MS = 45_000;
export function connectEvents(opts: EventStreamOptions): Promise<void>; // resolves when stopped
```

Behavior is the spec's "Client: event stream reader" section. Parse with `eventsource-parser` (`createParser` with `onEvent` and `onRetry`; comments reset the watchdog like any byte). Known types are checked against the three discriminator values before `onEvent`.

- [ ] **Step 1: Write the failing tests** with Vitest fake timers and a fake `fetch` that returns a `Response` over a controllable `ReadableStream` (a small helper in the test file: `push(text)`, `end()`, `error()`):
  - `connects with the bearer token and accepts text/event-stream`; URL `/api/v1/events`.
  - `calls onConnect on connect and on every reconnect`.
  - `delivers events split across chunks` (`data: {"type":"jo` then `b",...}\n\n`) and two events in one chunk, each once.
  - `ignores invalid JSON and unknown types`.
  - `resync reloads at once`, then the stream ends and it reconnects after the delay.
  - `retry field changes the reconnect delay` (`retry: 500` → reconnect at 500 ms, not 3000).
  - `reconnects after end, network error and 5xx` after 3000 ms.
  - `ping resets the watchdog`; `silence past 45 s aborts and reconnects`.
  - `401 stops for good`: `onUnauthorized` once, no further `fetch`, state `stopped`.
  - `stops without a request when the token is gone` at reconnect time.
  - `reads the token again on each attempt` (token changed between attempts).
  - `abort cancels a pending reconnect` and ends an open request; the promise resolves.
  - `reports its state`: `onState` sees `connecting`, `connected`, then `reconnecting` after an end, `connected` again, and `stopped` after abort or 401.
- [ ] **Step 2: Run** `npm test -- src/lib/events`. Expected: FAIL.
- [ ] **Step 3: Implement** `connectEvents`.
- [ ] **Step 4: Run** `npm test && npm run check && npm run lint`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(client): read the event stream with fetch and reconnect` (`Refs: #15`).

### Task 6: Pairing and status screens

**Files:**
- Create: `client/src/lib/auth/device-name.ts`, `client/src/routes/+layout.svelte`, `client/src/routes/pair/+page.svelte`, `client/README.md`
- Modify: `client/src/routes/+page.svelte` (status page), `README.md` (Usage: web client)
- Test: `client/src/lib/auth/device-name.test.ts`

**Interfaces:**
- Consumes: `api` (`$lib/api/app`), `session`, `describeError` (Task 4); `connectEvents`, `StreamState` (Task 5).
- Produces: `suggestDeviceName(userAgent: string): string`; `validateDeviceName(name: string): string | null` (null when valid, else the message).

- [ ] **Step 1: Write the failing tests**:
  - `suggestDeviceName`: a Firefox on Linux UA → `Firefox on Linux`; Safari on iPhone → `Safari on iPhone`; empty or unknown UA → `Web browser`; the result is never over 64 characters.
  - `validateDeviceName`: `'  Phone  '` → valid; `''` and `'   '` → `Enter a name.`; 65 characters → `At most 64 characters.`; a name with `\u0007` → `No control characters.`.
- [ ] **Step 2: Run** `npm test -- src/lib/auth`. Expected: FAIL.
- [ ] **Step 3: Implement** `device-name.ts`.
- [ ] **Step 4: Run** the same. Expected: PASS.
- [ ] **Step 5: Build the screens**:
  - `+layout.svelte`: without a token, `goto('/pair')`; with one on `/pair`, `goto('/')`.
  - `/pair`: code and name fields, `POST /api/v1/pairing` with the trimmed code; on success `session.signIn(token)` and `goto('/')`; errors through `describeError`.
  - `/`: loads `GET /health`, `/devices`, `/jobs` in `onConnect` (each part shows `Server unreachable` on its own failure); marks `current` devices ("This device"); `job` events upsert a `running` job and drop one whose status is `succeeded`, `failed` or `interrupted`; `device_revoked` reloads devices; a stream state indicator (`Connected` / `Reconnecting…`); `connectEvents` started in `$effect` with an `AbortController` aborted on cleanup. Plain CSS only (#77).
- [ ] **Step 6: Verify end to end** with a throwaway `DATA_DIR` in the scratchpad: `npm run build` in `client/`; from `server/`, `CLIENT_DIR=../client/build cargo run -p app`; open `http://127.0.0.1:8000` (chrome-devtools tools if the session has them, otherwise hand these steps to the user); pair with a code from `cargo run -p app -- pair-code` → status page, health, the device marked "This device", `Connected`. Pair a second device with `curl` (README "Pairing a device") and revoke the browser's device with `curl -X DELETE /api/v1/devices/{id}` using that second token → the page returns to `/pair`. Pair again, stop and restart the server → `Reconnecting…`, then `Connected` with the data reloaded. Report what was seen.
- [ ] **Step 7: Document**: README Usage gains "Web client" (build with `npm ci && npm run build` in `client/`, set `CLIENT_DIR`, open the server address, pair with a code), and the `CLIENT_DIR` row links to it; `client/README.md` covers purpose, scripts, layout, how the reader follows the protocol of #14, and that style waits for #77.
- [ ] **Step 8: Run** `mise run ci`. Expected: PASS.
- [ ] **Step 9: Commit** `feat(client): add the pairing and status screens` (`Closes: #15`).
