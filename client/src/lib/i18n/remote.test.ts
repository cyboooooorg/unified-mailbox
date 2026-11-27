import { describe, expect, it, vi } from 'vitest';
import { createApi } from '../api/client';
import { createRemoteLanguageSync } from './remote';
import type { Language } from './locale.svelte';

function setup(respond: () => Promise<Response>) {
	const requests: Request[] = [];
	const fetch = vi.fn((request: Request) => {
		requests.push(request);
		return respond();
	});
	const api = createApi({
		fetch: fetch as unknown as typeof globalThis.fetch,
		tokens: { get: () => 'tok', set: () => {}, clear: () => {} },
		onUnauthorized: () => {}
	});
	const state = { current: 'en' as Language };
	const locale = {
		get current() {
			return state.current;
		},
		set: (language: Language) => void (state.current = language)
	};
	return { fetch, requests, sync: createRemoteLanguageSync(api, locale), state };
}

const json = (body: unknown, status = 200) =>
	Promise.resolve(
		new Response(JSON.stringify(body), {
			status,
			headers: { 'content-type': 'application/json' }
		})
	);

describe('remote language sync', () => {
	it('get_applies_the_server_language', async () => {
		const s = setup(() => json({ language: 'fr' }));
		await s.sync.sync();
		expect(s.state.current).toBe('fr');
		expect(s.requests[0].method).toBe('GET');
	});

	it('get_error_response_keeps_the_language', async () => {
		const s = setup(() => json({ error: { code: 'internal', message: 'x' } }, 500));
		await s.sync.sync();
		expect(s.state.current).toBe('en');
		expect(s.fetch).toHaveBeenCalledOnce();
	});

	it('get_network_error_keeps_the_language', async () => {
		const s = setup(() => Promise.reject(new TypeError('network')));
		await s.sync.sync();
		expect(s.state.current).toBe('en');
	});

	it('put_200_applies_the_choice', async () => {
		const s = setup(() => json({ language: 'fr' }));
		expect(await s.sync.choose('fr')).toBe(true);
		expect(s.state.current).toBe('fr');
		const request = s.requests[0];
		expect(request.method).toBe('PUT');
		expect(await request.json()).toEqual({ language: 'fr' });
	});

	it('put_error_response_applies_nothing', async () => {
		const s = setup(() => json({ error: { code: 'internal', message: 'x' } }, 500));
		expect(await s.sync.choose('fr')).toBe(false);
		expect(s.state.current).toBe('en');
	});

	it('put_network_error_applies_nothing', async () => {
		const s = setup(() => Promise.reject(new TypeError('network')));
		expect(await s.sync.choose('fr')).toBe(false);
		expect(s.state.current).toBe('en');
	});
});
