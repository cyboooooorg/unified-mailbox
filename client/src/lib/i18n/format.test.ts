import { describe, expect, it } from 'vitest';
import { formatDate, formatDateTime, formatNumber, formatRelative } from './format';

const now = new Date('2026-03-05T14:07:00Z');
const ago = (seconds: number) => new Date(now.getTime() - seconds * 1000);

describe('format', () => {
	it('numbers_use_the_locale_separators', () => {
		expect(formatNumber(1234, 'en')).toBe('1,234');
		expect(formatNumber(1234, 'fr')).toBe('1\u{202F}234');
	});

	it('dates_follow_the_locale', () => {
		const date = '2026-03-05T14:07:00Z';
		expect(formatDate(date, 'en', 'UTC')).toBe('Mar 5, 2026');
		expect(formatDate(date, 'fr', 'UTC')).toBe('5 mars 2026');
	});

	it('date_times_follow_the_locale', () => {
		const date = new Date('2026-03-05T14:07:00Z');
		expect(formatDateTime(date, 'en', 'UTC')).toContain('2:07');
		expect(formatDateTime(date, 'fr', 'UTC')).toContain('14:07');
	});

	it('relative_picks_the_largest_unit', () => {
		expect(formatRelative(ago(30), now, 'en')).toBe('30 seconds ago');
		expect(formatRelative(ago(90), now, 'en')).toBe('1 minute ago');
		expect(formatRelative(ago(8 * 86400), now, 'en')).toBe('last week');
		expect(formatRelative(ago(86400), now, 'en')).toBe('yesterday');
		expect(formatRelative(ago(86400), now, 'fr')).toBe('hier');
		expect(formatRelative(new Date(now.getTime() + 2 * 3600 * 1000), now, 'en')).toBe('in 2 hours');
	});

	it('relative_covers_months_and_years', () => {
		expect(formatRelative(ago(45 * 86400), now, 'en')).toBe('last month');
		expect(formatRelative(ago(400 * 86400), now, 'en')).toBe('last year');
	});

	it('accepts_rfc3339_strings', () => {
		expect(formatRelative('2026-03-05T14:06:00Z', now, 'en')).toBe('1 minute ago');
	});
});
