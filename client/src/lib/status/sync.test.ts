import { describe, expect, it } from 'vitest';
import { createDeviceSync, createJobSync, type Loaded } from './sync';
import type { components } from '../api/schema';

type Device = components['schemas']['DeviceDto'];
type Job = components['schemas']['JobDto'];

function job(id: number, status: Job['status'] = 'running'): Job {
	return {
		id,
		kind: 'relabel',
		status,
		done: 0,
		failed: 0,
		total: null,
		error: null,
		created_at: '2026-01-01T00:00:00Z',
		finished_at: null
	};
}

function device(id: number): Device {
	return {
		id,
		name: `Device ${id}`,
		current: false,
		created_at: '2026-01-01T00:00:00Z',
		last_seen_at: null
	};
}

/** A fetcher whose calls are answered by the test, in any order. */
function controlled<T>() {
	const calls: { resolve: (value: T[] | null) => void; reject: () => void }[] = [];
	const fetcher = () =>
		new Promise<T[] | null>((resolve, reject) => {
			calls.push({ resolve, reject: () => reject(new Error('down')) });
		});
	return { fetcher, calls };
}

const ids = (state: Loaded<{ id: number }[]>) => state.data?.map((x) => x.id);

describe('createJobSync', () => {
	it('shows the loaded jobs', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		const loading = sync.load();
		calls[0].resolve([job(1)]);
		await loading;
		expect(ids(sync.state())).toEqual([1]);
	});

	it('replays an event received before an older GET response', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		const loading = sync.load();
		// J finished: its event arrives before the GET (answered with J running).
		sync.apply(job(1, 'succeeded'));
		calls[0].resolve([job(1), job(2)]);
		await loading;
		expect(ids(sync.state())).toEqual([2]);
	});

	it('replays a running event that the GET response does not hold', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		const loading = sync.load();
		sync.apply(job(3));
		calls[0].resolve([]);
		await loading;
		expect(ids(sync.state())).toEqual([3]);
	});

	it('does not replay an event received before the load started', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		sync.apply(job(5));
		const loading = sync.load();
		calls[0].resolve([]);
		await loading;
		// The GET is the truth for what came before it.
		expect(ids(sync.state())).toEqual([]);
	});

	it('drops a response older than the latest request', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		const first = sync.load();
		const second = sync.load();
		calls[1].resolve([job(2)]);
		await second;
		calls[0].resolve([job(1)]);
		await first;
		expect(ids(sync.state())).toEqual([2]);
	});

	it('replays events onto the latest of two overlapping loads', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		const first = sync.load();
		const second = sync.load();
		sync.apply(job(2, 'failed'));
		calls[1].resolve([job(2)]);
		await second;
		calls[0].resolve([job(2)]);
		await first;
		expect(ids(sync.state())).toEqual([]);
	});

	it('keeps the data and flags a failed load, then recovers', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		let loading = sync.load();
		calls[0].resolve([job(1)]);
		await loading;
		loading = sync.load();
		calls[1].reject();
		await loading;
		expect(sync.state()).toEqual({ data: [job(1)], failed: true });
		loading = sync.load();
		calls[2].resolve([job(1)]);
		await loading;
		expect(sync.state().failed).toBe(false);
	});

	it('flags a null response as failed', async () => {
		const { fetcher, calls } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		const loading = sync.load();
		calls[0].resolve(null);
		await loading;
		expect(sync.state()).toEqual({ data: null, failed: true });
	});

	it('replaces a running job and removes a finished one on events', () => {
		const { fetcher } = controlled<Job>();
		const sync = createJobSync(fetcher, () => {});
		sync.apply(job(1));
		sync.apply({ ...job(1), done: 4 });
		expect(sync.state().data).toEqual([{ ...job(1), done: 4 }]);
		sync.apply(job(1, 'succeeded'));
		expect(sync.state().data).toEqual([]);
	});

	it('tells onChange about each state', async () => {
		const { fetcher, calls } = controlled<Job>();
		const seen: (number[] | undefined)[] = [];
		const sync = createJobSync(fetcher, (state) => seen.push(ids(state)));
		sync.apply(job(1));
		const loading = sync.load();
		calls[0].resolve([]);
		await loading;
		expect(seen).toEqual([[1], []]);
	});
});

describe('createDeviceSync', () => {
	it('shows the loaded devices', async () => {
		const { fetcher, calls } = controlled<Device>();
		const sync = createDeviceSync(fetcher, () => {});
		const loading = sync.load();
		calls[0].resolve([device(1)]);
		await loading;
		expect(ids(sync.state())).toEqual([1]);
	});

	it('loads again after a device_revoked received during a load', async () => {
		const { fetcher, calls } = controlled<Device>();
		const sync = createDeviceSync(fetcher, () => {});
		const loading = sync.load();
		sync.deviceRevoked();
		calls[0].resolve([device(1), device(2)]);
		await Promise.resolve();
		await Promise.resolve();
		expect(calls).toHaveLength(2);
		calls[1].resolve([device(1)]);
		await loading;
		expect(ids(sync.state())).toEqual([1]);
	});

	it('loads at once on device_revoked when idle', async () => {
		const { fetcher, calls } = controlled<Device>();
		const sync = createDeviceSync(fetcher, () => {});
		sync.deviceRevoked();
		expect(calls).toHaveLength(1);
		calls[0].resolve([device(1)]);
		await Promise.resolve();
		await Promise.resolve();
		expect(ids(sync.state())).toEqual([1]);
		expect(calls).toHaveLength(1);
	});

	it('drops a response older than the latest request', async () => {
		const { fetcher, calls } = controlled<Device>();
		const sync = createDeviceSync(fetcher, () => {});
		const first = sync.load();
		const second = sync.load();
		calls[1].resolve([device(2)]);
		await second;
		calls[0].resolve([device(1), device(2)]);
		await first;
		expect(ids(sync.state())).toEqual([2]);
	});

	it('flags a failed load and keeps the data', async () => {
		const { fetcher, calls } = controlled<Device>();
		const sync = createDeviceSync(fetcher, () => {});
		let loading = sync.load();
		calls[0].resolve([device(1)]);
		await loading;
		loading = sync.load();
		calls[1].reject();
		await loading;
		expect(sync.state()).toEqual({ data: [device(1)], failed: true });
	});
});
