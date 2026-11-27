import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

function load(language: string): Record<string, unknown> {
	const path = new URL(`../../../messages/${language}.json`, import.meta.url);
	return JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>;
}

const en = load('en');
const fr = load('fr');

function keys(messages: Record<string, unknown>): string[] {
	return Object.keys(messages)
		.filter((key) => key !== '$schema')
		.sort();
}

/** The `{name}` placeholders anywhere in a value, variants included. */
function parameters(value: unknown): string[] {
	const found = JSON.stringify(value).matchAll(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g);
	return [...new Set([...found].map((match) => match[1]))].sort();
}

describe('message files', () => {
	it('fr_has_the_keys_of_en', () => {
		const missing = keys(en).filter((key) => !(key in fr));
		expect(missing).toEqual([]);
	});

	it('en_has_the_keys_of_fr', () => {
		const missing = keys(fr).filter((key) => !(key in en));
		expect(missing).toEqual([]);
	});

	it('parameters_match', () => {
		for (const key of keys(en).filter((key) => key in fr)) {
			expect({ key, parameters: parameters(fr[key]) }).toEqual({
				key,
				parameters: parameters(en[key])
			});
		}
	});

	it('finds_the_parameters_of_a_variant_message', () => {
		const variants = [
			{
				declarations: ['input count'],
				match: { 'count=one': '{count} job', 'count=other': '{count} jobs' }
			}
		];
		expect(parameters(variants)).toEqual(['count']);
		expect(parameters('Hello')).toEqual([]);
	});
});
