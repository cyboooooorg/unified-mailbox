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
});
