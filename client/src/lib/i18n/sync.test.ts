import { describe, expect, it } from 'vitest';
import { browserLanguage, createLanguageSync, type LanguageSyncDeps } from './sync';
import type { Language } from './locale.svelte';

function fake(init: {
	server: Language | null | undefined;
	current?: Language;
	preferred?: string[];
	putOk?: boolean;
}) {
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
		},
		preferred: () => init.preferred ?? []
	};
	return { calls, state, sync: createLanguageSync(deps) };
}

describe('browserLanguage', () => {
	it('browser_language_matches_the_primary_subtag', () => {
		expect(browserLanguage(['fr-CA'])).toBe('fr');
		expect(browserLanguage(['de', 'fr'])).toBe('fr');
		expect(browserLanguage(['de'])).toBe('en');
		expect(browserLanguage([])).toBe('en');
		expect(browserLanguage(['EN-gb'])).toBe('en');
	});
});

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

	it('null_language_puts_the_browser_language_once', async () => {
		const f = fake({ server: null, current: 'en', preferred: ['fr-FR'] });
		await f.sync.sync();
		expect(f.calls).toEqual(['put:fr', 'apply:fr']);
		await f.sync.sync();
		expect(f.calls).toEqual(['put:fr', 'apply:fr']);
	});

	it('null_language_put_failure_applies_nothing', async () => {
		const f = fake({ server: null, preferred: ['fr'], putOk: false });
		await f.sync.sync();
		expect(f.calls).toEqual(['put:fr']);
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
