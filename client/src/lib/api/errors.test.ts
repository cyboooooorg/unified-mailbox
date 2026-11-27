import { describe, expect, it } from 'vitest';
import { GENERIC_ERROR, describeError, errorMessage } from './errors';

describe('describeError', () => {
	it('explains an invalid pairing code', () => {
		expect(describeError({ error: { code: 'invalid_pairing_code', message: 'x' } })).toBe(
			'This code is invalid or expired.'
		);
	});

	it.each([null, undefined, {}, { error: 'x' }, { error: {} }, { error: { code: 'nope' } }, 'x'])(
		'falls back to the generic message for %j',
		(body) => {
			expect(describeError(body)).toBe(GENERIC_ERROR);
		}
	);

	it('has a message for every code', () => {
		expect(errorMessage('unauthorized')).not.toBe('');
	});
});
