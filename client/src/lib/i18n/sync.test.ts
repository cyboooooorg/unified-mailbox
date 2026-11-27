import { describe, expect, it } from 'vitest';
import { createLanguageSync, type LanguageSyncDeps } from './sync';
import type { Language } from './locale.svelte';

function fake(init: { server: Language | null | undefined; current?: Language; putOk?: boolean }) {
	const calls: string[] = [];
	const state = { server: init.server, current: init.current ?? 'en' };
	const deps: LanguageSyncDeps = {
		get: () => Promise.resolve(state.server),
		put: (language) => {
			calls.push(`put:${language}`);
			if (init.putOk ?? true) state.server = language;
			return Promise.resolve(init.putOk ?? true);
		},
		current: () => state.current,
		apply: (language) => {
			calls.push(`apply:${language}`);
			state.current = language;
		}
	};
	return { calls, state, sync: createLanguageSync(deps) };
}

describe('language sync', () => {
	it('server_language_is_applied', async () => {
		const f = fake({ server: 'fr', current: 'en' });
		await f.sync.sync();
		expect(f.calls).toEqual(['apply:fr']);
	});

	it('same_language_applies_nothing', async () => {
		const f = fake({ server: 'fr', current: 'fr' });
		await f.sync.sync();
		expect(f.calls).toEqual([]);
	});

	it('null_language_puts_the_language_on_screen_once', async () => {
		const f = fake({ server: null, current: 'fr' });
		await f.sync.sync();
		expect(f.calls).toEqual(['put:fr']);
		await f.sync.sync();
		expect(f.calls).toEqual(['put:fr']);
	});

	it('null_language_put_failure_applies_nothing', async () => {
		const f = fake({ server: null, current: 'fr', putOk: false });
		await f.sync.sync();
		expect(f.calls).toEqual(['put:fr']);
		expect(f.state.current).toBe('fr');
	});

	it('failed_get_keeps_the_cached_language', async () => {
		const f = fake({ server: undefined, current: 'fr' });
		await f.sync.sync();
		expect(f.calls).toEqual([]);
	});

	it('choose_applies_after_the_put', async () => {
		const f = fake({ server: 'en' });
		expect(await f.sync.choose('fr')).toBe(true);
		expect(f.calls).toEqual(['put:fr', 'apply:fr']);
	});

	it('choose_keeps_the_language_when_the_put_fails', async () => {
		const f = fake({ server: 'en', putOk: false });
		expect(await f.sync.choose('fr')).toBe(false);
		expect(f.calls).toEqual(['put:fr']);
	});
});
