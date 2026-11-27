import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { connectEvents, DEFAULT_RETRY_MS, WATCHDOG_MS, type StreamState } from './stream';
import type { TokenStore } from '../auth/token-store';
import type { components } from '../api/schema';

type EventDto = components['schemas']['EventDto'];

/** What the next call to the fake `fetch` does. */
type Script = number | 'network' | 'hang';

interface Connection {
	url: string;
	init: RequestInit;
	/** Sends a chunk of the body. */
	push(text: string): void;
	/** Sends raw bytes, to cut a character in two. */
	pushBytes(bytes: Uint8Array): void;
	/** Ends the body normally. */
	end(): void;
	/** Breaks the body mid-stream. */
	error(): void;
}

const encoder = new TextEncoder();

/**
 * A fake `fetch` that behaves like the real one with a signal: an abort rejects
 * a pending request and errors an open body. Each call takes the next script
 * entry (200 when the script is empty).
 */
function fakeFetch(...script: Script[]) {
	const connections: Connection[] = [];
	const fetch = vi.fn((input: RequestInfo | URL, init: RequestInit = {}) => {
		const step = script.shift() ?? 200;
		let controller!: ReadableStreamDefaultController<Uint8Array>;
		const body = new ReadableStream<Uint8Array>({
			start(c) {
				controller = c;
			}
		});
		const connection: Connection = {
			url: input instanceof Request ? input.url : input.toString(),
			init,
			push: (text) => controller.enqueue(encoder.encode(text)),
			pushBytes: (bytes) => controller.enqueue(bytes),
			end: () => controller.close(),
			error: () => controller.error(new TypeError('connection reset'))
		};
		connections.push(connection);
		return new Promise<Response>((resolve, reject) => {
			const signal = init.signal;
			signal?.addEventListener('abort', () => {
				const reason = new DOMException('The operation was aborted.', 'AbortError');
				reject(reason);
				try {
					controller.error(reason);
				} catch {
					// Already closed.
				}
			});
			if (step === 'hang') return;
			if (step === 'network') return reject(new TypeError('fetch failed'));
			if (step === 200) {
				return resolve(
					new Response(body, { status: 200, headers: { 'content-type': 'text/event-stream' } })
				);
			}
			resolve(new Response('{"error":{"code":"x","message":"x"}}', { status: step }));
		});
	});
	return { fetch, connections };
}

function memoryTokens(initial: string | null): TokenStore {
	let token = initial;
	return {
		get: () => token,
		set: (t) => void (token = t),
		clear: () => void (token = null)
	};
}

/** Lets every pending promise and stream read settle (setImmediate is not faked). */
async function flush() {
	for (let i = 0; i < 5; i++) await new Promise((r) => setImmediate(r));
}

/** Settles what is pending first (so a timer it arms counts), then moves the clock. */
async function advance(ms: number) {
	await flush();
	vi.advanceTimersByTime(ms);
	await flush();
}

function start(opts: { script?: Script[]; token?: string | null; tokens?: TokenStore } = {}) {
	const { fetch, connections } = fakeFetch(...(opts.script ?? []));
	const tokens = opts.tokens ?? memoryTokens(opts.token === undefined ? 'tok-1' : opts.token);
	const controller = new AbortController();
	const onConnect = vi.fn();
	const onEvent = vi.fn<(event: EventDto) => void>();
	const onUnauthorized = vi.fn();
	const states: StreamState[] = [];
	let finished = false;
	const done = connectEvents({
		tokens,
		onConnect,
		onEvent,
		onUnauthorized,
		onState: (s) => states.push(s),
		signal: controller.signal,
		fetch,
		setTimeout: globalThis.setTimeout,
		clearTimeout: globalThis.clearTimeout
	}).then(() => {
		finished = true;
	});
	return {
		fetch,
		connections,
		tokens,
		controller,
		onConnect,
		onEvent,
		onUnauthorized,
		states,
		done,
		isFinished: () => finished,
		last: () => connections[connections.length - 1]
	};
}

const JOB = {
	type: 'job',
	job: {
		id: 7,
		kind: 'relabel',
		status: 'running',
		done: 1,
		failed: 0,
		total: 3,
		error: null,
		created_at: '2026-01-01T00:00:00Z',
		finished_at: null
	}
};

beforeEach(() => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
});

afterEach(() => {
	vi.useRealTimers();
	vi.restoreAllMocks();
});

describe('connectEvents', () => {
	it('connects with the bearer token and accepts text/event-stream', async () => {
		const s = start();
		await flush();
		expect(s.fetch).toHaveBeenCalledOnce();
		const { url, init } = s.connections[0];
		expect(url).toBe('/api/v1/events');
		const headers = new Headers(init.headers);
		expect(headers.get('authorization')).toBe('Bearer tok-1');
		expect(headers.get('accept')).toBe('text/event-stream');
		expect(url).not.toContain('tok-1');
		expect(init.cache).toBe('no-store');
		s.controller.abort();
		await s.done;
	});

	it('calls onConnect on connect and on every reconnect', async () => {
		const s = start();
		await flush();
		expect(s.onConnect).toHaveBeenCalledTimes(1);
		s.last().end();
		await advance(DEFAULT_RETRY_MS);
		expect(s.fetch).toHaveBeenCalledTimes(2);
		expect(s.onConnect).toHaveBeenCalledTimes(2);
		s.controller.abort();
		await s.done;
	});

	it('delivers events split across chunks', async () => {
		const s = start();
		await flush();
		const json = JSON.stringify(JOB);
		s.last().push(`data: ${json.slice(0, 12)}`);
		await flush();
		expect(s.onEvent).not.toHaveBeenCalled();
		s.last().push(`${json.slice(12)}\n\n`);
		await flush();
		expect(s.onEvent).toHaveBeenCalledExactlyOnceWith(JOB);
		s.controller.abort();
		await s.done;
	});

	it('delivers an event whose multi-byte character is split across chunks', async () => {
		const s = start();
		await flush();
		const event = { ...JOB, job: { ...JOB.job, kind: 'étiquette' } };
		const bytes = encoder.encode(`data: ${JSON.stringify(event)}\n\n`);
		// Cut between the two bytes of the first "é".
		const cut = bytes.indexOf(0xc3) + 1;
		s.last().pushBytes(bytes.slice(0, cut));
		await flush();
		s.last().pushBytes(bytes.slice(cut));
		await flush();
		expect(s.onEvent).toHaveBeenCalledExactlyOnceWith(event);
		s.controller.abort();
		await s.done;
	});

	it('delivers two events in one chunk, each once', async () => {
		const s = start();
		await flush();
		const revoked = { type: 'device_revoked', device_id: 4 };
		s.last().push(`data: ${JSON.stringify(JOB)}\n\ndata: ${JSON.stringify(revoked)}\n\n`);
		await flush();
		expect(s.onEvent.mock.calls).toEqual([[JOB], [revoked]]);
		s.controller.abort();
		await s.done;
	});

	it('ignores invalid JSON and unknown types', async () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const s = start();
		await flush();
		s.last().push('data: {"type":"jo secret-payload\n\n');
		s.last().push('data: {"type":"future","secret":"secret-payload"}\n\n');
		s.last().push('data: "secret-payload"\n\n');
		s.last().push('data: null\n\n');
		s.last().push(`data: ${JSON.stringify(JOB)}\n\n`);
		await flush();
		expect(s.onEvent).toHaveBeenCalledExactlyOnceWith(JOB);
		expect(warn).toHaveBeenCalledTimes(4);
		expect(JSON.stringify(warn.mock.calls)).not.toContain('secret-payload');
		// Still connected: nothing was ignored by dropping the stream.
		expect(s.fetch).toHaveBeenCalledOnce();
		s.controller.abort();
		await s.done;
	});

	it('resync reloads at once', async () => {
		const s = start();
		await flush();
		expect(s.onConnect).toHaveBeenCalledTimes(1);
		s.last().push('data: {"type":"resync"}\n\n');
		await flush();
		expect(s.onConnect).toHaveBeenCalledTimes(2);
		expect(s.onEvent).toHaveBeenCalledExactlyOnceWith({ type: 'resync' });
		// The server then ends the stream: the reader reconnects after the delay.
		s.last().end();
		await advance(DEFAULT_RETRY_MS - 1);
		expect(s.fetch).toHaveBeenCalledOnce();
		await advance(1);
		expect(s.fetch).toHaveBeenCalledTimes(2);
		expect(s.onConnect).toHaveBeenCalledTimes(3);
		s.controller.abort();
		await s.done;
	});

	it('settings_changed reaches onEvent', async () => {
		const s = start();
		await flush();
		s.last().push('data: {"type":"settings_changed"}\n\n');
		await flush();
		expect(s.onEvent).toHaveBeenCalledExactlyOnceWith({ type: 'settings_changed' });
		expect(s.onConnect).toHaveBeenCalledTimes(1);
		s.controller.abort();
		await s.done;
	});

	it('retry field changes the reconnect delay', async () => {
		const s = start();
		await flush();
		s.last().push('retry: 500\n\n');
		await flush();
		s.last().end();
		await advance(499);
		expect(s.fetch).toHaveBeenCalledOnce();
		await advance(1);
		expect(s.fetch).toHaveBeenCalledTimes(2);
		// The new delay holds for the following reconnections too.
		s.last().end();
		await advance(500);
		expect(s.fetch).toHaveBeenCalledTimes(3);
		s.controller.abort();
		await s.done;
	});

	it.each([
		['end', (s: ReturnType<typeof start>) => s.last().end(), []],
		['network error', () => {}, ['network'] as Script[]],
		['broken body', (s: ReturnType<typeof start>) => s.last().error(), []],
		['5xx', () => {}, [503] as Script[]]
	])(
		'reconnects after end, network error and 5xx: %s',
		async (_name, breakIt, script: Script[]) => {
			const s = start({ script: [...script] });
			await flush();
			breakIt(s);
			await flush();
			await advance(DEFAULT_RETRY_MS - 1);
			expect(s.fetch).toHaveBeenCalledOnce();
			await advance(1);
			expect(s.fetch).toHaveBeenCalledTimes(2);
			expect(s.onUnauthorized).not.toHaveBeenCalled();
			s.controller.abort();
			await s.done;
		}
	);

	it('a 5xx does not count as connected', async () => {
		const s = start({ script: [503] });
		await flush();
		expect(s.onConnect).not.toHaveBeenCalled();
		await advance(DEFAULT_RETRY_MS);
		expect(s.onConnect).toHaveBeenCalledOnce();
		s.controller.abort();
		await s.done;
	});

	it('ping resets the watchdog', async () => {
		const s = start();
		await flush();
		await advance(WATCHDOG_MS - 5000);
		s.last().push(': ping\n\n');
		await flush();
		await advance(WATCHDOG_MS - 1);
		expect(s.connections[0].init.signal?.aborted).toBe(false);
		await advance(1);
		expect(s.connections[0].init.signal?.aborted).toBe(true);
		s.controller.abort();
		await s.done;
	});

	it('silence past 45 s aborts and reconnects', async () => {
		const s = start();
		await flush();
		await advance(WATCHDOG_MS - 1);
		expect(s.connections[0].init.signal?.aborted).toBe(false);
		await advance(1);
		expect(s.connections[0].init.signal?.aborted).toBe(true);
		await advance(DEFAULT_RETRY_MS);
		expect(s.fetch).toHaveBeenCalledTimes(2);
		expect(s.onConnect).toHaveBeenCalledTimes(2);
		s.controller.abort();
		await s.done;
	});

	it('the watchdog covers waiting for the response headers', async () => {
		const s = start({ script: ['hang'] });
		await flush();
		await advance(WATCHDOG_MS);
		expect(s.connections[0].init.signal?.aborted).toBe(true);
		await advance(DEFAULT_RETRY_MS);
		expect(s.fetch).toHaveBeenCalledTimes(2);
		expect(s.onConnect).toHaveBeenCalledOnce();
		s.controller.abort();
		await s.done;
	});

	it('401 stops for good', async () => {
		const s = start({ script: [401] });
		await s.done;
		expect(s.onUnauthorized).toHaveBeenCalledOnce();
		expect(s.onConnect).not.toHaveBeenCalled();
		expect(s.states.at(-1)).toBe('stopped');
		await advance(10 * WATCHDOG_MS);
		expect(s.fetch).toHaveBeenCalledOnce();
	});

	it('a revoked device gets the 401 on reconnection and stops', async () => {
		const s = start({ script: [200, 401] });
		await flush();
		s.last().end();
		await advance(DEFAULT_RETRY_MS);
		await s.done;
		expect(s.onUnauthorized).toHaveBeenCalledOnce();
		expect(s.fetch).toHaveBeenCalledTimes(2);
	});

	it('stops without a request when the token is gone', async () => {
		const s = start();
		await flush();
		s.tokens.clear();
		s.last().end();
		await advance(DEFAULT_RETRY_MS);
		await s.done;
		expect(s.fetch).toHaveBeenCalledOnce();
		expect(s.onUnauthorized).toHaveBeenCalledOnce();
		expect(s.states.at(-1)).toBe('stopped');
	});

	it('makes no request without a token at the start', async () => {
		const s = start({ token: null });
		await s.done;
		expect(s.fetch).not.toHaveBeenCalled();
		expect(s.onUnauthorized).toHaveBeenCalledOnce();
	});

	it('reads the token again on each attempt', async () => {
		const s = start();
		await flush();
		s.tokens.set('tok-2');
		s.last().end();
		await advance(DEFAULT_RETRY_MS);
		expect(s.fetch).toHaveBeenCalledTimes(2);
		expect(new Headers(s.connections[1].init.headers).get('authorization')).toBe('Bearer tok-2');
		s.controller.abort();
		await s.done;
	});

	it('abort cancels a pending reconnect', async () => {
		const s = start();
		await flush();
		s.last().end();
		await advance(DEFAULT_RETRY_MS / 2);
		s.controller.abort();
		await s.done;
		await advance(10 * DEFAULT_RETRY_MS);
		expect(s.fetch).toHaveBeenCalledOnce();
		expect(s.states.at(-1)).toBe('stopped');
	});

	it('abort ends an open request', async () => {
		const s = start();
		await flush();
		s.controller.abort();
		await s.done;
		expect(s.connections[0].init.signal?.aborted).toBe(true);
		await advance(10 * WATCHDOG_MS);
		expect(s.fetch).toHaveBeenCalledOnce();
		expect(s.onUnauthorized).not.toHaveBeenCalled();
	});

	it('abort ends a request waiting for its headers', async () => {
		const s = start({ script: ['hang'] });
		await flush();
		s.controller.abort();
		await s.done;
		expect(s.connections[0].init.signal?.aborted).toBe(true);
		await advance(10 * WATCHDOG_MS);
		expect(s.fetch).toHaveBeenCalledOnce();
	});

	it('an already aborted signal makes no request', async () => {
		const { fetch } = fakeFetch();
		const controller = new AbortController();
		controller.abort();
		await connectEvents({
			tokens: memoryTokens('tok-1'),
			onConnect: vi.fn(),
			onEvent: vi.fn(),
			onUnauthorized: vi.fn(),
			signal: controller.signal,
			fetch
		});
		expect(fetch).not.toHaveBeenCalled();
	});

	it('reports its state', async () => {
		const s = start();
		await flush();
		expect(s.states).toEqual(['connecting', 'connected']);
		s.last().end();
		await flush();
		expect(s.states).toEqual(['connecting', 'connected', 'reconnecting']);
		await advance(DEFAULT_RETRY_MS);
		expect(s.states).toEqual(['connecting', 'connected', 'reconnecting', 'connected']);
		s.controller.abort();
		await s.done;
		expect(s.states).toEqual(['connecting', 'connected', 'reconnecting', 'connected', 'stopped']);
	});

	it('reports stopped after a 401', async () => {
		const s = start({ script: [401] });
		await s.done;
		expect(s.states).toEqual(['connecting', 'stopped']);
	});

	it('a throwing callback does not drop the connection', async () => {
		const error = vi.spyOn(console, 'error').mockImplementation(() => {});
		const s = start();
		s.onEvent.mockImplementation(() => {
			throw new Error('page bug');
		});
		await flush();
		s.last().push(`data: ${JSON.stringify(JOB)}\n\ndata: ${JSON.stringify(JOB)}\n\n`);
		await flush();
		expect(s.onEvent).toHaveBeenCalledTimes(2);
		expect(error).toHaveBeenCalled();
		expect(s.fetch).toHaveBeenCalledOnce();
		expect(s.connections[0].init.signal?.aborted).toBe(false);
		s.controller.abort();
		await s.done;
	});

	it('resets the parser on each connection', async () => {
		const s = start();
		await flush();
		// A half event, then the connection breaks: its tail must not join the next stream.
		s.last().push('data: {"type":"res');
		await flush();
		s.last().error();
		await advance(DEFAULT_RETRY_MS);
		s.last().push(`ync"}\n\ndata: ${JSON.stringify(JOB)}\n\n`);
		await flush();
		expect(s.onEvent).toHaveBeenCalledExactlyOnceWith(JOB);
		s.controller.abort();
		await s.done;
	});
});
