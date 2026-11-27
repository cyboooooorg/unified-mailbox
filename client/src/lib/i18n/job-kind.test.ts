import { describe, expect, it } from 'vitest';
import { jobKindLabel } from './job-kind';

describe('jobKindLabel', () => {
	it('known_kind_is_translated', () => {
		expect(jobKindLabel('relabel', { relabel: () => 'Relabelling' })).toBe('Relabelling');
	});

	it('unknown_kind_is_shown_raw', () => {
		expect(jobKindLabel('labels_sync')).toBe('labels_sync');
	});

	it.each(['constructor', 'toString'])('inherited_property_%s_is_shown_raw', (kind) => {
		expect(jobKindLabel(kind)).toBe(kind);
		expect(jobKindLabel(kind, { other: () => 'x' })).toBe(kind);
	});
});
