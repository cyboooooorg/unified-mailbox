import { afterEach, describe, expect, it } from 'vitest';
import { healthLabel } from './health';
import { locale } from './locale.svelte';

afterEach(() => locale.set('en'));

describe('healthLabel', () => {
	it('ok_is_translated', () => {
		expect(healthLabel('ok')).toBe('Operational');
		locale.set('fr');
		expect(healthLabel('ok')).toBe('En service');
	});
});
