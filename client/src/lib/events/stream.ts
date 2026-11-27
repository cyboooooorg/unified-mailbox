import { createParser } from 'eventsource-parser';
import { API_BASE } from '../api/client';
import type { TokenStore } from '../auth/token-store';
import type { components } from '../api/schema';

type EventDto = components['schemas']['EventDto'];

export type StreamState = 'connecting' | 'connected' | 'reconnecting' | 'stopped';

export interface EventStreamOptions {
	tokens: TokenStore;
	/** Reload what is shown: called on every connect, reconnect and `resync`. */
	onConnect: () => void;
	onEvent: (event: EventDto) => void;
	/** The server refused the token, or there is none: sign out. */
	onUnauthorized: () => void;
	onState?: (state: StreamState) => void;
	/** Aborting it stops the reader: it ends the request and any pending reconnect. */
	signal: AbortSignal;
	fetch?: typeof fetch;
	setTimeout?: typeof setTimeout;
	clearTimeout?: typeof clearTimeout;
}

/** The reconnect delay until the server sends a `retry` field. */
export const DEFAULT_RETRY_MS = 3000;
/** Without a byte for this long (three missed pings), the connection is dead. */
export const WATCHDOG_MS = 45_000;

const EVENTS_PATH = '/api/v1/events';
const KNOWN_TYPES: ReadonlySet<string> = new Set<EventDto['type']>([
	'job',
	'device_revoked',
	'resync'
]);

/** How one connection ended. */
type Outcome = 'retry' | 'unauthorized' | 'aborted';

/**
 * Reads the server's event stream with `fetch` and the bearer token, and
 * reconnects until `signal` is aborted or the server refuses the token.
 * Resolves once stopped; never rejects.
 */
export async function connectEvents(opts: EventStreamOptions): Promise<void> {
	const fetchFn = opts.fetch ?? globalThis.fetch.bind(globalThis);
	const setTimer = opts.setTimeout ?? globalThis.setTimeout;
	const clearTimer = opts.clearTimeout ?? globalThis.clearTimeout;
	const { signal } = opts;

	let retryMs = DEFAULT_RETRY_MS;
	let state: StreamState | undefined;
	const setState = (next: StreamState) => {
		if (next === state) return;
		state = next;
		guard(() => opts.onState?.(next));
	};

	const parser = createParser({
		onRetry: (ms) => {
			retryMs = ms;
		},
		onEvent: ({ data }) => {
			const event = parseEvent(data);
			if (!event) return;
			if (event.type === 'resync') guard(opts.onConnect);
			guard(() => opts.onEvent(event));
		}
	});

	setState('connecting');
	while (!signal.aborted) {
		const token = opts.tokens.get();
		if (!token) {
			setState('stopped');
			guard(opts.onUnauthorized);
			return;
		}
		// A new stream: drop what is left of an event cut by the last one.
		parser.reset();
		const outcome = await connectOnce(token);
		if (outcome === 'unauthorized') {
			setState('stopped');
			guard(opts.onUnauthorized);
			return;
		}
		if (outcome === 'aborted') break;
		setState('reconnecting');
		await wait(retryMs);
	}
	setState('stopped');

	async function connectOnce(token: string): Promise<Outcome> {
		const request = new AbortController();
		const stop = () => request.abort();
		signal.addEventListener('abort', stop);
		// The watchdog runs from the request on: it covers the wait for the
		// headers as well as a silent body.
		let watchdog: ReturnType<typeof setTimeout> | undefined;
		const feedWatchdog = () => {
			if (watchdog !== undefined) clearTimer(watchdog);
			watchdog = setTimer(stop, WATCHDOG_MS);
		};
		feedWatchdog();
		try {
			const response = await fetchFn(`${API_BASE}${EVENTS_PATH}`, {
				headers: { Accept: 'text/event-stream', Authorization: `Bearer ${token}` },
				cache: 'no-store',
				signal: request.signal
			});
			if (response.status === 401) {
				await response.body?.cancel().catch(() => {});
				return 'unauthorized';
			}
			if (!response.ok || !response.body) {
				await response.body?.cancel().catch(() => {});
				return 'retry';
			}
			setState('connected');
			guard(opts.onConnect);
			const reader = response.body.getReader();
			const decoder = new TextDecoder();
			for (;;) {
				const { done, value } = await reader.read();
				if (done) break;
				feedWatchdog();
				parser.feed(decoder.decode(value, { stream: true }));
			}
			return signal.aborted ? 'aborted' : 'retry';
		} catch {
			// Network error, broken body, watchdog or abort.
			return signal.aborted ? 'aborted' : 'retry';
		} finally {
			if (watchdog !== undefined) clearTimer(watchdog);
			signal.removeEventListener('abort', stop);
		}
	}

	/** Waits `ms`, or less when `signal` is aborted meanwhile. */
	function wait(ms: number): Promise<void> {
		return new Promise((resolve) => {
			if (signal.aborted) return resolve();
			const done = () => {
				clearTimer(timer);
				signal.removeEventListener('abort', done);
				resolve();
			};
			const timer = setTimer(done, ms);
			signal.addEventListener('abort', done);
		});
	}
}

/** Parses one `data` field; logs and drops what this client does not know. */
function parseEvent(data: string): EventDto | undefined {
	let value: unknown;
	try {
		value = JSON.parse(data);
	} catch {
		// Never log the payload: it may carry mail metadata.
		console.warn('events: ignored an event that is not valid JSON');
		return undefined;
	}
	const type =
		typeof value === 'object' && value !== null ? (value as { type?: unknown }).type : undefined;
	if (typeof type !== 'string' || !KNOWN_TYPES.has(type)) {
		console.warn('events: ignored an event of an unknown type');
		return undefined;
	}
	return value as EventDto;
}

/** Runs a caller's callback so that its exception cannot drop the stream. */
function guard(callback: () => void) {
	try {
		callback();
	} catch (error) {
		console.error('events: a callback threw', error);
	}
}
