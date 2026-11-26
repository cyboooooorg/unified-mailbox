# Client skeleton design

Issue: #15. Related: #13 (HTTP skeleton), #14 (jobs and SSE), #57
(deployment), #58 (iOS and Android apps), #62 (internationalization), #77
(visual style and UI kit), #5 (phase 3: client).

## Intent

`client/` becomes a working web app that knows the server only through
`api/openapi.json`. It proves the whole chain end to end: a device pairs
and keeps its bearer token, calls typed by the contract, and an event
stream that keeps what is shown up to date. Every screen of phase 3
builds on it. The server serves the built app next to the API, so the
web client needs one origin, one port and no CORS.

## Goals

- A SvelteKit + TypeScript app in `client/`, built as static files.
- Types generated from `api/openapi.json`, and calls typed by them. No
  hand-written copy of a server type, events included.
- The device token flow: pairing with a one-time code, the token kept on
  the device, sign-out when the server answers 401.
- An event stream reader that follows the protocol of #14.
- Two screens: pairing, and a status page (server health, devices, running
  jobs) kept live by the stream.
- The server serves the built client from an optional directory.
- `npm run check`, `npm run lint` and `npm test` in `mise run ci` and the
  CI, with a check that the generated types match the contract.

## Non-goals

- A UI kit and a visual style: decided with the owner in #77, before the
  first screen of phase 3. The skeleton uses plain, minimal CSS.
- An i18n library: #62. The skeleton keeps its strings in English in one
  module.
- Capacitor, native token storage and CORS for the mobile apps: #58.
- Revoking a device or creating a pairing code from the client: they
  come with the settings screens. The status page lists devices only.
- Offline use, a service worker, push notifications.

## Anti-goals

- A token in a URL, in a log, or sent to anything but `/api`.
- A hand-written type duplicating one of `api/openapi.json`.
- The app shell able to hide an API route: an unknown `/api/...` path
  must stay a JSON 404, never `index.html`.
- A stream that stays silently dead, or that keeps retrying with a
  revoked token.
- A client that shows stale data after a reconnect or a `resync`.

## Mailbot reference

Mailbot's client is a single page served by FastAPI from the same
process, with `EventSource` on an unauthenticated `/api/events`. Nothing
of its UI is carried over; the event protocol was redesigned in #14.

## Approaches

### Serving the client (open since #13), chosen by the user

- **The server serves a directory, `CLIENT_DIR` (chosen).** The API stays
  on `/api`, the app is on every other path. One origin: no CORS for the
  web, one `tailscale serve`, and the container of #57 copies the build.
  Unset, the server serves the API only, as today (development, where
  Vite serves the app).
- **Embed the build in the binary (`rust-embed`).** One file to deploy,
  but the Rust build and CI would depend on npm.
- **A separate static server (Caddy, or a path route of `tailscale
  serve`).** Two things to deploy and configure, for no gain behind
  Tailscale.

### Framework

- **SvelteKit with `adapter-static` in SPA mode (chosen).** A router
  built in, a purely static build with an `index.html` fallback, which is
  what Capacitor needs (#58). No Node in production.
- **Svelte + Vite with a third-party router.** Lighter, but the router is
  one more choice to make and maintain.

### API client

- **`openapi-typescript` + `openapi-fetch` (chosen).** Generates types
  only; calls are typed by path and method. Small, OpenAPI 3.1, no
  runtime code generated.
- **`@hey-api/openapi-ts`.** Generates one function per operation; more
  generated code to review, and its plugins change often.

### Event stream reader

- **`fetch` + `eventsource-parser`, with our own reconnect loop
  (chosen).** The parser is maintained and does only parsing; the loop is
  small and is where the protocol of #14 lives, so it is ours and tested.
- **`@microsoft/fetch-event-source`.** Does both, but is unmaintained
  since 2021.

## Design

### Server: serving the client

`CLIENT_DIR` is a new optional variable, read once in `app` and passed to
`http_api::router` as a value (such as `Option<ClientDir>`).

- Unset or empty: no static serving; every path outside the API is the
  JSON 404 of today.
- Set: at startup it must be a directory holding `index.html`, otherwise
  it is a `ConfigError` reported with the others, and the server exits 1.

Routing, in this order:

1. A path equal to `/api` or starting with `/api/` is the API's. An
   unknown one gets the JSON `not_found`, an unsupported method the JSON
   `method_not_allowed`, as today.
2. `GET` or `HEAD` of a file that exists in `CLIENT_DIR`: that file
   (`tower-http` `ServeDir`, which refuses paths leaving the directory).
3. `GET` or `HEAD` of a missing path whose last segment has no extension
   (`/`, `/jobs`, `/settings/devices`): `index.html`, so a deep link or a
   reload works. The app's router shows the page.
4. Any other missing path (`/_app/immutable/x.js`, `/favicon.ico`): a
   plain 404, so a stale asset fails clearly instead of loading HTML as
   JavaScript. Another method: 405.

Headers:

- API responses keep `Cache-Control: no-store` and `nosniff`.
- `/_app/immutable/*` (hashed names): `Cache-Control: public,
  max-age=31536000, immutable`. Every other static response, `index.html`
  included: `Cache-Control: no-cache`, so a server update is picked up on
  the next load.
- Every static response: `X-Content-Type-Options: nosniff`,
  `Referrer-Policy: no-referrer`, and on `index.html` a strict
  `Content-Security-Policy`: `default-src 'self'; script-src 'self'
  <hashes>; connect-src 'self'; img-src 'self' data:; object-src 'none';
  base-uri 'none'; form-action 'self'; frame-ancestors 'none'`. The token
  lives in `localStorage`, so the CSP is the guard against injected
  script; `'unsafe-inline'` is never allowed for scripts. SvelteKit's
  inline bootstrap script is allowed by hash: either SvelteKit emits the
  CSP itself (`kit.csp` in `hash` mode, a `<meta>` tag) and the server
  adds only `frame-ancestors` (which a `<meta>` cannot carry), or the
  build writes the hashes for the server. The builder picks the one that
  works with the SPA fallback page and documents it. Start with
  `kit.csp` in `hash` mode, which keeps the server a plain file server;
  move to build-written hashes only if the fallback page comes out
  without the hash of its bootstrap script.
- Static files need no token: they hold no data. Tailscale stays the
  first barrier. The request log keeps logging the path only.

The OpenAPI document is unchanged: static serving is outside it.

### Client: layout

```text
client/
  package.json            # scripts below; versions pinned by the lockfile
  svelte.config.js        # adapter-static, fallback index.html
  vite.config.ts          # dev proxy /api -> $API_PROXY_TARGET,
                          # default http://127.0.0.1:8000
  src/
    lib/
      api/
        schema.d.ts       # generated from ../api/openapi.json, committed
        client.ts         # openapi-fetch client + auth middleware
        errors.ts         # ErrorCode -> message, from the generated enum
      auth/
        token-store.ts    # TokenStore interface + localStorage impl
        session.svelte.ts # signed-in state, sign-out
      events/
        stream.ts         # the event stream reader
    routes/
      +layout.ts          # ssr = false, prerender = false
      +layout.svelte      # redirects to /pair without a token
      +page.svelte        # status page
      pair/+page.svelte   # pairing form
```

Svelte 5 with runes. The client only ever calls same-origin `/api/v1`
(the base URL is a single constant, so #58 can set the server's address).

### Client: API and token

- `npm run generate` writes `src/lib/api/schema.d.ts` from
  `../api/openapi.json`. The file is committed. `npm run check:contract`
  regenerates it into a temporary file and fails when it differs, and
  runs in CI: a contract change without a regenerated client fails.
  That `api/openapi.json` matches the server is already checked by the
  server tests (`mise run ci` fails while it is stale); together they
  cover server to client.
- The `openapi-fetch` client has one middleware: it adds `Authorization:
  Bearer <token>` when there is a token, and on a 401 from any route but
  `/pairing` it clears the token and signs out (back to `/pair`). The
  stream reader's `onUnauthorized` calls the same sign-out.
- `TokenStore` (`get`, `set`, `clear`) is backed by `localStorage` under
  one key. #58 replaces the implementation, not the callers.
- Errors are read from the body `{"error": {"code", "message"}}` and
  shown through `errors.ts`, keyed by the generated `ErrorCode` type, so
  a new code fails the type check until it gets a message. A response
  without that body (network error, proxy) shows a generic message.

### Client: pairing

A form with the code and a device name, suggested from the browser
(such as "Firefox on Linux", from the user agent) and editable. The name
is checked as the server does (`PairingRequest`): trimmed, 1 to 64
characters, no control character. The code is sent as typed, trimmed: the server
already accepts lower case and the missing hyphen. On success the token
is stored and the app goes to `/`. `invalid_pairing_code` shows "This
code is invalid or expired"; other errors through `errors.ts`.

### Client: event stream reader

`stream.ts` exports one function, for example
`connectEvents({ fetch, token, onConnect, onEvent, onUnauthorized,
signal })`, independent of Svelte so it can be tested with a fake
`fetch`. `fetch` and the timer functions are injected.

- It requests `GET /api/v1/events` with `Accept: text/event-stream` and
  the bearer token.
- **Connected** (2xx): calls `onConnect`, which reloads what the page
  shows. This happens on the first connect and on every reconnect.
- **Fields**: a `retry` field sets the reconnect delay (3000 ms until the
  server sends one). Comments (`: ping`) only reset the watchdog. Each
  `data` field is parsed as JSON and passed to `onEvent` typed as the
  generated `EventDto`. Invalid JSON or an unknown `type` is ignored
  (logged to the console without the payload), so a newer server does
  not break an older client.
- **`resync`**: calls `onConnect` (reload) at once; the server then ends
  the stream and the reader reconnects as below.
- **End of stream, network error, or 5xx**: wait the retry delay, then
  reconnect, re-reading the token each time.
- **401**: no retry; calls `onUnauthorized` (sign-out). A revoked
  device's stream ends at once and its reconnection gets the 401.
- **Watchdog** (our call): no byte for 45 s (three missed pings) aborts
  the request and reconnects, so a dead connection behind a sleeping
  laptop or a dropped tailnet does not stay silent.
- **Stop**: aborting `signal` (sign-out, leaving the page) ends the
  request and cancels any pending reconnect.

### Client: status page

Shows the server health, the active devices and the running jobs, each
loaded from its route. `onConnect` reloads all three; health has no
event and is not polled. A failed load shows "Server unreachable" for
that part, the others stay. The current device (`DeviceDto.current`)
is marked. Events:

- `job`: replaces or adds the job in the list, removes it when it is no
  longer running (the event is a hint; the next reload is the truth);
- `device_revoked`: reloads the device list;
- `resync`: handled by the reader (reload).

A small indicator shows the stream state: connected, reconnecting.

### Tooling and CI

- Node, current LTS, is pinned in `mise.toml`. Dependencies are pinned
  by `package-lock.json`; CI uses `npm ci`.
- Scripts: `dev`, `build`, `generate`, `check` (`svelte-check`),
  `check:contract`, `lint` (ESLint + Prettier check), `test` (Vitest).
- `mise run ci` runs the server checks and the client checks
  (`npm ci` if needed is the CI's job, not the task's), so it stays the
  one command to run before a pull request. The existing workflow
  (`.github/workflows/server.yml`) is extended, not duplicated: Node
  through mise, `npm ci` in `client/`, npm cache. Renaming it to `ci` is
  the builder's call.
- `.gitignore` covers `client/node_modules`, `client/build` and
  `client/.svelte-kit`.

### Tests

Server (`http` crate, against a temporary `CLIENT_DIR` with invented
files, and `app` for the variable):

- `/` and a deep link (`/jobs/42`) return `index.html` with `no-cache`
  and the CSP; an existing asset returns its bytes and type; an
  immutable asset gets the long cache; a missing path with an extension
  is a 404, not `index.html`; `/api/unknown` and `/api` stay JSON 404s
  with a client directory set; `POST /` is a 405; `../` cannot escape
  the directory; without `CLIENT_DIR`, `/` is the JSON 404.
- `CLIENT_DIR` pointing to a missing directory, a file, or a directory
  without `index.html` is a `ConfigError`; empty means unset.

Client (Vitest, no network):

- The stream reader against a fake `fetch` returning a `ReadableStream`:
  `onConnect` on connect and on every reconnect; `retry` changes the
  delay; a ping resets the watchdog and silence past it reconnects; a
  `resync` reloads and the following end reconnects; a 401 stops for
  good and calls `onUnauthorized`; a 5xx and a network error retry;
  invalid JSON and an unknown type are ignored; aborting cancels a
  pending reconnect; the token is read again on each attempt.
- The auth middleware: adds the header; a 401 clears the token, except
  on `/pairing`; no header without a token.
- `TokenStore` against a fake storage.

### Documentation

- README: `CLIENT_DIR` in the configuration table and `.env.example`
  (the container of #57 will set it); a
  "Web client" part under Usage (build it, point `CLIENT_DIR` at
  `client/build`, open the server's address, pair); Development: Node
  through mise, `npm run dev` with the server running, `npm run
  generate` after `mise run openapi`, the client checks in `mise run ci`.
- `client/README.md`: purpose, scripts, layout, how the stream reader
  follows the protocol.
- CLAUDE.md: the client checks are now part of `mise run ci`.

## Left to the builder

- Exact versions of Node, SvelteKit, Svelte, Vite and the libraries.
- How the CSP hashes reach the response (see Headers), as long as no
  `'unsafe-inline'` script is allowed and `frame-ancestors 'none'` is
  sent.
- The module split inside `http` for static serving, and whether the
  extension rule uses the last segment or a list of asset prefixes, as
  long as the cases under Tests hold.
- Whether `mise run ci` is split into `ci:server` and `ci:client` tasks
  that `ci` depends on.
- The look of the two screens, kept minimal until #77.
