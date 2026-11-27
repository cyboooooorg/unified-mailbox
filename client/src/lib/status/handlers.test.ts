import { describe, expect, it, vi } from 'vitest';
import { createStatusHandlers } from './handlers';
import type { components } from '../api/schema';

const job: components['schemas']['JobDto'] = {
	id: 1,
	kind: 'k',
	status: 'running',
	done: 0,
	failed: 0,
	total: null,
	error: null,
	created_at: '2026-01-01T00:00:00Z',
	finished_at: null
};

function setup() {
	const deps = {
		loadHealth: vi.fn(() => Promise.resolve()),
		deviceSync: { load: vi.fn(() => Promise.resolve()), deviceRevoked: vi.fn() },
		jobSync: { load: vi.fn(() => Promise.resolve()), apply: vi.fn() },
		languageSync: { sync: vi.fn(() => Promise.resolve()) }
	};
	return { deps, handlers: createStatusHandlers(deps) };
}

describe('status handlers', () => {
	it('connect_reloads_everything_and_syncs_the_language', () => {
		const { deps, handlers } = setup();
		handlers.onConnect();
		expect(deps.loadHealth).toHaveBeenCalledOnce();
		expect(deps.deviceSync.load).toHaveBeenCalledOnce();
		expect(deps.jobSync.load).toHaveBeenCalledOnce();
		expect(deps.languageSync.sync).toHaveBeenCalledOnce();
	});

	it('settings_changed_syncs_the_language', () => {
		const { deps, handlers } = setup();
		handlers.onEvent({ type: 'settings_changed' });
		expect(deps.languageSync.sync).toHaveBeenCalledOnce();
		expect(deps.jobSync.apply).not.toHaveBeenCalled();
	});

	it('job_event_applies_the_job', () => {
		const { deps, handlers } = setup();
		handlers.onEvent({ type: 'job', job });
		expect(deps.jobSync.apply).toHaveBeenCalledExactlyOnceWith(job);
		expect(deps.languageSync.sync).not.toHaveBeenCalled();
	});

	it('device_revoked_reloads_devices', () => {
		const { deps, handlers } = setup();
		handlers.onEvent({ type: 'device_revoked', device_id: 7 });
		expect(deps.deviceSync.deviceRevoked).toHaveBeenCalledOnce();
	});

	it('resync_does_not_reload_again', () => {
		const { deps, handlers } = setup();
		handlers.onEvent({ type: 'resync' });
		expect(deps.loadHealth).not.toHaveBeenCalled();
		expect(deps.languageSync.sync).not.toHaveBeenCalled();
	});
});
