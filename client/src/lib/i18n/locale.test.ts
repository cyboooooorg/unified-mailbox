import { afterEach, describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import { locale } from './locale.svelte';

afterEach(() => locale.set('en'));

describe('locale', () => {
	it('set_switches_the_messages_without_a_browser', () => {
		expect(locale.current).toBe('en');
		expect(m.error_not_found()).toBe('Not found.');
		locale.set('fr');
		expect(locale.current).toBe('fr');
		expect(m.error_not_found()).toBe('Introuvable.');
	});

	describe('with a browser', () => {
		const globals = globalThis as Record<string, unknown>;
		const saved = {
			window: globals.window,
			document: globals.document,
			localStorage: globals.localStorage
		};

		afterEach(() => {
			if (saved.window === undefined) delete globals.window;
			else globals.window = saved.window;
			globals.document = saved.document;
			globals.localStorage = saved.localStorage;
		});

		it('set_writes_html_lang_and_persists_the_choice', () => {
			const store = new Map<string, string>();
			const documentElement = { lang: 'en' };
			// Paraglide writes localStorage only when it sees a `window`.
			globals.window = {};
			globals.document = { documentElement };
			globals.localStorage = {
				getItem: (key: string) => store.get(key) ?? null,
				setItem: (key: string, value: string) => void store.set(key, value),
				removeItem: (key: string) => void store.delete(key)
			};
			locale.set('fr');
			expect(documentElement.lang).toBe('fr');
			expect(store.get('PARAGLIDE_LOCALE')).toBe('fr');
		});
	});
});
