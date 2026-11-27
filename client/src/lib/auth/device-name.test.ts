import { describe, expect, it } from 'vitest';
import { suggestDeviceName, validateDeviceName } from './device-name';

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

	it('is never over 64 characters', () => {
		expect(suggestDeviceName(FIREFOX_LINUX.repeat(20)).length).toBeLessThanOrEqual(64);
	});
});

describe('validateDeviceName', () => {
	it('accepts a name once trimmed', () => {
		expect(validateDeviceName('  Phone  ')).toBeNull();
	});

	it('accepts exactly 64 characters', () => {
		expect(validateDeviceName('a'.repeat(64))).toBeNull();
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
