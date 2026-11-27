import { describe, expect, it } from 'vitest';
import { limit, suggestDeviceName, validateDeviceName } from './device-name';

const FIREFOX_LINUX = 'Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0';
const SAFARI_IPHONE =
	'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1';
const CHROME_WINDOWS =
	'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36';

describe('suggestDeviceName', () => {
	it('names Firefox on Linux', () => {
		expect(suggestDeviceName(FIREFOX_LINUX)).toBe('Firefox on Linux');
	});

	it('names Safari on iPhone', () => {
		expect(suggestDeviceName(SAFARI_IPHONE)).toBe('Safari on iPhone');
	});

	it('names Chrome on Windows, not Safari', () => {
		expect(suggestDeviceName(CHROME_WINDOWS)).toBe('Chrome on Windows');
	});

	it('falls back for an empty or unknown user agent', () => {
		expect(suggestDeviceName('')).toBe('Web browser');
		expect(suggestDeviceName('SomeBot/1.0')).toBe('Web browser');
	});

	it.each([
		[
			'Edge, which also says Chrome and Safari',
			CHROME_WINDOWS + ' Edg/126.0.2592.81',
			'Edge on Windows'
		],
		[
			'Opera, which also says Chrome and Safari',
			CHROME_WINDOWS + ' OPR/112.0.0.0',
			'Opera on Windows'
		],
		[
			'Chrome on Android, which also says Linux',
			'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36',
			'Chrome on Android'
		],
		[
			'Safari on iPad, which also says Mac OS X',
			'Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1',
			'Safari on iPad'
		]
	])('names %s', (_name, userAgent, expected) => {
		expect(suggestDeviceName(userAgent)).toBe(expected);
	});
});

describe('limit', () => {
	it('keeps a name of 64 code points', () => {
		expect(limit('a'.repeat(64))).toBe('a'.repeat(64));
	});

	it('cuts a longer name to 64 code points, not UTF-16 units', () => {
		const emoji = '\u{1F600}';
		expect(limit(emoji.repeat(70))).toBe(emoji.repeat(64));
		expect(limit('a'.repeat(70))).toBe('a'.repeat(64));
	});

	it('gives a valid name once cut', () => {
		expect(validateDeviceName(limit('b'.repeat(100)))).toBeNull();
	});
});

describe('validateDeviceName', () => {
	it('accepts a name once trimmed', () => {
		expect(validateDeviceName('  Phone  ')).toBeNull();
	});

	it('accepts exactly 64 characters', () => {
		expect(validateDeviceName('a'.repeat(64))).toBeNull();
	});

	it('counts code points, not UTF-16 units', () => {
		const emoji = '\u{1F600}';
		expect(validateDeviceName(emoji.repeat(64))).toBeNull();
		expect(validateDeviceName(emoji.repeat(65))).toBe('At most 64 characters.');
	});

	it('trims before counting', () => {
		expect(validateDeviceName('  ' + 'a'.repeat(64) + '  ')).toBeNull();
	});

	it('rejects an empty or blank name', () => {
		expect(validateDeviceName('')).toBe('Enter a name.');
		expect(validateDeviceName('   ')).toBe('Enter a name.');
	});

	it('rejects more than 64 characters', () => {
		expect(validateDeviceName('a'.repeat(65))).toBe('At most 64 characters.');
	});

	it('rejects a control character', () => {
		expect(validateDeviceName('Pho\u0007ne')).toBe('No control characters.');
	});
});
