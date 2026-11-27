import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from './schema';
import type { TokenStore } from '../auth/token-store';

/** Same origin: the server serves the client and the API. */
export const API_BASE = '';

const PAIRING_PATH = '/api/v1/pairing';

export function createApi(opts: {
	fetch?: typeof fetch;
	tokens: TokenStore;
	onUnauthorized: () => void;
}) {
	const auth: Middleware = {
		onRequest({ request }) {
			const token = opts.tokens.get();
			if (token) request.headers.set('Authorization', `Bearer ${token}`);
			return request;
		},
		onResponse({ request, response }) {
			// A wrong pairing code is a 401 too, but the device is not signed in yet.
			if (
				response.status === 401 &&
				new URL(request.url, 'http://localhost').pathname !== PAIRING_PATH
			) {
				opts.tokens.clear();
				opts.onUnauthorized();
			}
			return response;
		}
	};
	const client = createClient<paths>({
		// A Request needs an absolute URL: resolve the empty base against this origin.
		baseUrl: API_BASE || globalThis.location?.origin || 'http://localhost',
		fetch: opts.fetch
	});
	client.use(auth);
	return client;
}
