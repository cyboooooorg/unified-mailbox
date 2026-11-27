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
