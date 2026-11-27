import type { Language } from './locale.svelte';

type Instant = Date | string;

const toDate = (value: Instant): Date => (value instanceof Date ? value : new Date(value));

export function formatNumber(value: number, language: Language): string {
	return new Intl.NumberFormat(language).format(value);
}

export function formatDate(value: Instant, language: Language, timeZone?: string): string {
	return new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeZone }).format(toDate(value));
}

export function formatDateTime(value: Instant, language: Language, timeZone?: string): string {
	return new Intl.DateTimeFormat(language, {
		dateStyle: 'medium',
		timeStyle: 'short',
		timeZone
	}).format(toDate(value));
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
	['year', 365 * 86400],
	['month', 30 * 86400],
	['week', 7 * 86400],
	['day', 86400],
	['hour', 3600],
	['minute', 60],
	['second', 1]
];

/** The largest unit whose absolute difference is at least 1, truncated toward zero. */
export function formatRelative(value: Instant, now: Date, language: Language): string {
	const seconds = (toDate(value).getTime() - now.getTime()) / 1000;
	const format = new Intl.RelativeTimeFormat(language, { numeric: 'auto' });
	for (const [unit, size] of UNITS) {
		if (Math.abs(seconds) >= size) return format.format(Math.trunc(seconds / size), unit);
	}
	return format.format(0, 'second');
}
