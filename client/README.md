# sv

Everything you need to build a Svelte project, powered by [`sv`](https://github.com/sveltejs/cli).

## Creating a project

If you're seeing this, you've probably already done this step. Congrats!

```sh
# create a new project
npx sv create my-app
```

To recreate this project with the same configuration:

```sh
# recreate this project
npx sv@1.7.1 create --template minimal --types ts --add prettier eslint vitest="usages:unit" --install npm client
```

## Adding features

Add features to your project with `sv add`:

```sh
npx sv add
```

For example, to add Tailwind CSS:

```sh
npx sv add tailwindcss
```

## Developing

Once you've created a project and installed dependencies with `npm install` (or `pnpm install` or `yarn`), start a development server:

```sh
npm run dev

# or start the server and open the app in a new browser tab
npm run dev -- --open
```

## Building

To create a production version of your app:

```sh
npm run build
```

You can preview the production build with `npm run preview`.

> To deploy your app, you may need to install an [adapter](https://svelte.dev/docs/kit/adapters) for your target environment.

## API client and session

- `src/lib/api/client.ts` builds the typed `openapi-fetch` client from the
  generated `paths`. It sends the device token as a bearer header to the same
  origin only, and on a 401 (except from `/api/v1/pairing`) clears the token
  and signs out.
- `src/lib/api/app.ts` is the app-wide `api`; `src/lib/auth/session.svelte.ts`
  holds the session state. The token lives in `localStorage` under
  `unified-mailbox.token`.
- `src/lib/api/errors.ts` maps each server error code to a short message;
  adding a code to the contract fails `npm run check` until it has one.

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
  changed since. Every known event (`job`, `device_revoked`, `resync`) then
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
