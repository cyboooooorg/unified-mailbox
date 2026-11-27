import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it } from 'vitest';
import { getLocale, overwriteGetLocale } from '#lib/paraglide/runtime.js';
import { describeError, errorMessage, genericError } from './errors';

function load(language: string): Record<string, string> {
	const path = new URL(`../../../messages/${language}.json`, import.meta.url);
	return JSON.parse(readFileSync(path, 'utf8')) as Record<string, string>;
}

const en = load('en');
const fr = load('fr');

const original = getLocale;
afterEach(() => overwriteGetLocale(original));

describe('describeError', () => {
	it('explains an invalid pairing code', () => {
		overwriteGetLocale(() => 'en');
		expect(describeError({ error: { code: 'invalid_pairing_code', message: 'x' } })).toBe(
			'This code is invalid or expired.'
		);
	});

	it.each([
		null,
		undefined,
		{},
		{ error: 'x' },
		{ error: {} },
		{ error: { code: 'nope' } },
		{ error: { code: 'toString' } },
		{ error: { code: 'constructor' } },
		'x'
	])('falls back to the generic message for %j', (body) => {
		for (const [language, messages] of [
			['en', en],
			['fr', fr]
		] as const) {
			overwriteGetLocale(() => language);
			expect(describeError(body)).toBe(messages.error_generic);
			expect(genericError()).toBe(messages.error_generic);
		}
	});

	it('has a message for every code', () => {
		expect(errorMessage('unauthorized')).not.toBe('');
	});

	it('messages_follow_the_locale', () => {
		overwriteGetLocale(() => 'fr');
		expect(errorMessage('invalid_pairing_code')).toBe(fr.error_invalid_pairing_code);
		expect(describeError({ error: { code: 'invalid_pairing_code' } })).toBe(
			fr.error_invalid_pairing_code
		);
		overwriteGetLocale(() => 'en');
		expect(errorMessage('invalid_pairing_code')).toBe(en.error_invalid_pairing_code);
	});
});
