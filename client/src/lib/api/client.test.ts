import { describe, expect, it, vi } from 'vitest';
import { createApi } from './client';
import type { TokenStore } from '../auth/token-store';

function memoryTokens(initial: string | null): TokenStore {
	let token = initial;
	return {
		get: () => token,
		set: (t) => void (token = t),
		clear: () => void (token = null)
	};
}

function setup(token: string | null, status = 200) {
	const requests: Request[] = [];
	const fetch = vi.fn((input: Request) => {
		requests.push(input);
		return Promise.resolve(
			new Response(status === 401 ? '{"error":{"code":"unauthorized","message":"x"}}' : '[]', {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
	});
	const tokens = memoryTokens(token);
	const onUnauthorized = vi.fn();
	const api = createApi({
		fetch: fetch as unknown as typeof globalThis.fetch,
		tokens,
		onUnauthorized
	});
	return { api, requests, tokens, onUnauthorized };
}

describe('createApi', () => {
	it('adds the bearer header when a token exists', async () => {
		const { api, requests } = setup('tok-1');
		await api.GET('/api/v1/devices');
		expect(requests[0].headers.get('authorization')).toBe('Bearer tok-1');
	});

	it('sends no header without a token', async () => {
		const { api, requests } = setup(null);
		await api.GET('/api/v1/devices');
		expect(requests[0].headers.has('authorization')).toBe(false);
	});

	it('requests a same-origin /api/v1 path with no token in it', async () => {
		const { api, requests } = setup('tok-1');
		await api.GET('/api/v1/devices');
		const url = new URL(requests[0].url, 'http://localhost');
		expect(url.pathname).toBe('/api/v1/devices');
		expect(requests[0].url).not.toContain('tok-1');
	});

	it('a 401 clears the token and calls onUnauthorized', async () => {
		const { api, tokens, onUnauthorized } = setup('tok-1', 401);
		await api.GET('/api/v1/devices');
		expect(tokens.get()).toBeNull();
		expect(onUnauthorized).toHaveBeenCalledOnce();
	});

	it('a 401 from /api/v1/pairing does not sign out', async () => {
		const { api, tokens, onUnauthorized } = setup('tok-1', 401);
		await api.POST('/api/v1/pairing', { body: { code: 'abc', device_name: 'd' } as never });
		expect(tokens.get()).toBe('tok-1');
		expect(onUnauthorized).not.toHaveBeenCalled();
	});
});
