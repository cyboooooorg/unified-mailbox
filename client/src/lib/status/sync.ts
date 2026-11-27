import type { components } from '../api/schema';

type Device = components['schemas']['DeviceDto'];
type Job = components['schemas']['JobDto'];

/** What a part of the page shows: the last good data, and whether the last load failed. */
export interface Loaded<T> {
	data: T | null;
	failed: boolean;
}

/** Loads a list; `null` (or a throw) means the load failed. */
export type Fetcher<T> = () => Promise<T[] | null>;

async function attempt<T>(fetcher: Fetcher<T>): Promise<T[] | null> {
	try {
		return (await fetcher()) ?? null;
	} catch {
		return null;
	}
}

/** `job` event applied to a list: a running job is replaced or added, any other is removed. */
function applyJob(jobs: Job[], job: Job): Job[] {
	if (job.status !== 'running') return jobs.filter((known) => known.id !== job.id);
	return jobs.some((known) => known.id === job.id)
		? jobs.map((known) => (known.id === job.id ? job : known))
		: [...jobs, job];
}

/**
 * The running jobs, kept right while a load and the event stream race (they
 * are separate connections): a response older than the latest request is
 * dropped, and the events received while a load was in flight are replayed
 * onto its result.
 */
export function createJobSync(fetchJobs: Fetcher<Job>, onChange: (state: Loaded<Job[]>) => void) {
	let state: Loaded<Job[]> = { data: null, failed: false };
	let latest = 0;
	// For each load in flight, the events that arrived since it started.
	const inFlight = new Map<number, Job[]>();

	const set = (next: Loaded<Job[]>) => {
		state = next;
		onChange(state);
	};

	return {
		state: () => state,
		/** A `job` event. */
		apply(job: Job) {
			for (const missed of inFlight.values()) missed.push(job);
			set({ ...state, data: applyJob(state.data ?? [], job) });
		},
		async load() {
			const id = ++latest;
			const missed: Job[] = [];
			inFlight.set(id, missed);
			const jobs = await attempt(fetchJobs);
			inFlight.delete(id);
			if (id !== latest) return;
			if (!jobs) return set({ ...state, failed: true });
			set({ data: missed.reduce(applyJob, jobs), failed: false });
		}
	};
}

/**
 * The active devices. A response older than the latest request is dropped;
 * a `device_revoked` event received while a load is in flight may be missing
 * from its result, so it triggers one more load once that one is done.
 */
export function createDeviceSync(
	fetchDevices: Fetcher<Device>,
	onChange: (state: Loaded<Device[]>) => void
) {
	let state: Loaded<Device[]> = { data: null, failed: false };
	let latest = 0;
	let inFlight = 0;
	let stale = false;

	const set = (next: Loaded<Device[]>) => {
		state = next;
		onChange(state);
	};

	async function load(): Promise<void> {
		const id = ++latest;
		inFlight++;
		const devices = await attempt(fetchDevices);
		inFlight--;
		if (id === latest) {
			set(devices ? { data: devices, failed: false } : { ...state, failed: true });
			if (stale) {
				stale = false;
				return load();
			}
		}
	}

	return {
		state: () => state,
		load,
		/** A `device_revoked` event. */
		deviceRevoked() {
			if (inFlight > 0) stale = true;
			else void load();
		}
	};
}
