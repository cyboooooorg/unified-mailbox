# Unified Mailbox web client

The web client of Unified Mailbox: a SvelteKit single-page app in TypeScript
(no server rendering). It only talks to the server's HTTP API, through types
generated from `api/openapi.json`, and the Rust server serves its build (see
"Web client" in the repository README).

It has two screens: `/pair` (pairing code and device name, the name suggested
from the browser) and `/` (server health, paired devices with this one marked,
running jobs, and the state of the event stream). An unpaired browser is sent
to `/pair`, a paired one away from it. Styling is plain CSS in component
`<style>` blocks, kept minimal until #77: the Content-Security-Policy has no
`style-src`, so there is no `style=` attribute and no injected style.

## Prerequisites

Node, installed by `mise install` at the repository root (version pinned in
`mise.toml`).

## Scripts

Run from `client/`:

| Script                   | What it does                                                                              |
| ------------------------ | ----------------------------------------------------------------------------------------- |
| `npm ci`                 | Installs the pinned dependencies.                                                         |
| `npm run dev`            | Dev server; `/api` is proxied to `http://127.0.0.1:8000` (`API_PROXY_TARGET` changes it). |
| `npm run build`          | Builds the static app into `build/`, to serve with `CLIENT_DIR`.                          |
| `npm run generate`       | Regenerates `src/lib/api/schema.d.ts` after `mise run openapi`.                           |
| `npm run messages`       | Compiles `messages/*.json` into `src/lib/paraglide/` (also run by `prepare` and `check`). |
| `npm run check`          | `svelte-check` (types).                                                                   |
| `npm run check:contract` | Fails when `schema.d.ts` is stale.                                                        |
| `npm run check:csp`      | Fails unless the built page has a hashed CSP without `'unsafe-inline'`.                   |
| `npm run lint`           | Prettier and ESLint.                                                                      |
| `npm test`               | Vitest.                                                                                   |

## Layout

- `src/routes/`: the screens (`+layout.svelte` holds the sign-in redirects,
  `pair/` the pairing page, `+page.svelte` the status page).
- `src/lib/api/`: the typed client, the error messages, the generated types.
- `src/lib/auth/`: the token store, the session, the device name rules.
- `src/lib/events/`: the event stream reader.
- `src/lib/status/`: keeps the job and device lists right while a load and the
  event stream race (drops older responses, replays the events a load missed).

## API client and session

- `src/lib/api/client.ts` builds the typed `openapi-fetch` client from the
  generated `paths`. It sends the device token as a bearer header to the same
  origin only, and on a 401 (except from `/api/v1/pairing`) clears the token
  and signs out.
- `src/lib/api/app.ts` is the app-wide `api`; `src/lib/auth/session.svelte.ts`
  holds the session state. The token lives in `localStorage` under
  `unified-mailbox.token`.
- `src/lib/api/errors.ts` maps each server error code to a translated short
  message; adding a code to the contract fails `npm run check` until it has one.

## Event stream reader

`src/lib/events/stream.ts` exports `connectEvents`, which follows the server's
event stream (`GET /api/v1/events`, see "Jobs and events" in the repository
README). It does not depend on Svelte: `fetch` and the timer functions are
injected, so the tests drive it with a fake `fetch` and fake timers.

- It sends the device token as a bearer header (never in the URL), read from
  the `TokenStore` again on every attempt, and parses the stream with
  `eventsource-parser`, reset for each connection.
- On every connect and reconnect, and on every `resync` event, it calls
  `onConnect`: the page reloads what it shows, since events only tell what
  changed since. Every known event (`job`, `device_revoked`, `settings_changed`, `resync`) then
  goes to `onEvent`; invalid JSON and unknown types are logged without their
  payload and ignored, so a newer server does not break an older client.
- After the end of the stream, a network error or any status other than 2xx
  and 401, it waits the reconnect delay (3000 ms until the server sends a
  `retry` field) and reconnects.
- A 401, or no token left to send, stops it for good and calls
  `onUnauthorized` (sign-out).
- A watchdog aborts the request and reconnects after 45 s without a byte
  (three missed pings), from the request on: it covers the wait for the
  headers too.
- Aborting the `signal` ends the request and any pending reconnect; the
  returned promise resolves once the reader has stopped. `onState` reports
  `connecting`, `connected`, `reconnecting` and `stopped`.

```ts
const controller = new AbortController();
void connectEvents({
	tokens: session.tokens,
	onConnect: reload,
	onEvent: (event) => console.log(event.type),
	onUnauthorized: () => session.signOut(),
	signal: controller.signal
});
```

The status page starts it in an `$effect` and aborts it when the page is left.
Its `onConnect` reloads health, devices and jobs; `job` events add, update or
drop a running job and `device_revoked` reloads the devices. A `resync` is
already handled by the reader (it calls `onConnect`), so the page does not
reload again on it.

## Translations

Messages are compiled by [Paraglide](https://paraglidejs.com) (`@inlang/paraglide-js`
3.x) into typed functions, imported as `import { m } from
'#lib/paraglide/messages.js'` and called in templates: `{m.error_not_found()}`.

- `messages/en.json` (the base language and the fallback) and
  `messages/fr.json` hold the strings; `project.inlang/settings.json` lists
  the locales. `src/lib/paraglide/` is generated (by `npm run messages`, which
  `prepare`, `check` and the Vite plugin run) and git-ignored. The
  message-format plugin is a pinned devDependency loaded from `node_modules`
  (`modules` in `settings.json`), so compiling needs no network.
- Key names: `<screen>_<thing>` (`pair_title`), `error_<code>` for the server
  error codes (`error_generic` for any other failure) and `job_kind_<kind>`
  for job kinds. A `{name}` parameter is passed as `m.key({ name })`; plurals
  and other variants use the message-format variant syntax. A test checks that
  both files have the same keys and the same parameters.
- `src/lib/i18n/locale.svelte.ts` holds the language: `locale.current` is a
  `$state` read from Paraglide's own resolution (`localStorage`, then the
  browser language, then English) and `locale.set(language)` stores the choice,
  sets `<html lang>` and re-renders every message in place. Paraglide's
  `getLocale` is overridden to read that state, so a language change does not
  reload or remount the page (the status page keeps its event stream).
- Formatting: never build a number, date or relative time by hand. Use
  `src/lib/i18n/format.ts` (`formatNumber`, `formatDate`, `formatDateTime`,
  `formatRelative`, all `Intl`-based) and pass `locale.current`, so the screen
  re-renders when the language changes. A count shown in a sentence is
  formatted first and passed next to the number: the plural message selects on
  `count` (`local countPlural = count: plural`) and displays `countText`. Job
  kinds go through `jobKindLabel` (`src/lib/i18n/job-kind.ts`): add a
  `job_kind_<kind>` message and register it in `JOB_KIND_LABELS`; an unknown
  kind is shown as is.
- Language sync (`src/lib/i18n/sync.ts`, wired in `app.ts`): the server holds
  the language (`/api/v1/settings/language`) and is the truth; `localStorage`
  is only a cache that makes the first paint right, and it is kept on
  sign-out. The first device to connect, when the server has no language yet,
  stores the language it shows (Paraglide's resolution: `localStorage`, then the
  browser language, then English). The status page syncs on every stream connect
  (reconnects and `resync` included) and on `settings_changed`; a failed
  request keeps the cached language. `LanguagePicker.svelte` stores the
  choice on the server first and applies it only when that succeeded.
- Language names are written in their own language and are not translated
  (`LANGUAGE_NAMES` in `locale.svelte.ts`; the picker lists Paraglide's
  `locales`); `Unified Mailbox` is not translated either. The server status
  code is translated through `healthLabel` (`src/lib/i18n/health.ts`).

To add a message: add the key to `messages/en.json` and `messages/fr.json`
(the same parameters in both), then call `m.<key>()`.

To add a language, in the same change:

1. add `messages/<code>.json` with every key of `messages/en.json`;
2. add the code to `locales` in `project.inlang/settings.json`;
3. add its name, in itself, to `LANGUAGE_NAMES` in
   `src/lib/i18n/locale.svelte.ts` (`npm run check` fails until it has one,
   once the contract lists the code);
4. add the variant to `Language` and `LanguageDto` in the server, then
   regenerate the contract (`mise run openapi`) and the client types
   (`npm run generate`).
