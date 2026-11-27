<script lang="ts">
	import { api } from '../lib/api/app';
	import { session } from '../lib/auth/session.svelte';
	import { connectEvents, type StreamState } from '../lib/events/stream';
	import { createDeviceSync, createJobSync, type Loaded } from '../lib/status/sync';
	import type { components } from '../lib/api/schema';

	type Health = components['schemas']['HealthDto'];
	type Device = components['schemas']['DeviceDto'];
	type Job = components['schemas']['JobDto'];
	type EventDto = components['schemas']['EventDto'];

	const UNREACHABLE = 'Server unreachable';
	const STREAM_LABELS: Record<StreamState, string> = {
		connecting: 'Connecting…',
		connected: 'Connected',
		reconnecting: 'Reconnecting…',
		stopped: 'Disconnected'
	};

	// Each part loads on its own: a failure shows in that part only.
	let health = $state<Health | null>(null);
	let healthFailed = $state(false);
	let devices = $state<Loaded<Device[]>>({ data: null, failed: false });
	let jobs = $state<Loaded<Job[]>>({ data: null, failed: false });
	let stream = $state<StreamState>('connecting');

	// Loads and events race (separate connections): the sync modules reconcile them.
	const deviceSync = createDeviceSync(
		async () => (await api.GET('/api/v1/devices')).data ?? null,
		(state) => (devices = state)
	);
	const jobSync = createJobSync(
		async () => (await api.GET('/api/v1/jobs')).data ?? null,
		(state) => (jobs = state)
	);

	async function loadHealth() {
		try {
			const { data } = await api.GET('/api/v1/health');
			health = data ?? null;
			healthFailed = !data;
		} catch {
			healthFailed = true;
		}
	}

	function reload() {
		void loadHealth();
		void deviceSync.load();
		void jobSync.load();
	}

	function onEvent(event: EventDto) {
		if (event.type === 'job') jobSync.apply(event.job);
		else if (event.type === 'device_revoked') deviceSync.deviceRevoked();
		// `resync` already reloaded: the reader called `onConnect`.
	}

	$effect(() => {
		const controller = new AbortController();
		void connectEvents({
			tokens: session.tokens,
			onConnect: reload,
			onEvent,
			onUnauthorized: () => session.signOut(),
			onState: (state) => (stream = state),
			signal: controller.signal
		});
		return () => controller.abort();
	});
</script>

<main>
	<h1>Unified Mailbox</h1>
	<p class="stream" data-state={stream}>
		{STREAM_LABELS[stream]}
	</p>

	<section>
		<h2>Server</h2>
		{#if healthFailed}
			<p>{UNREACHABLE}</p>
		{:else if health}
			<p>Status: {health.status}, version {health.version}</p>
		{:else}
			<p>Loading…</p>
		{/if}
	</section>

	<section>
		<h2>Devices</h2>
		{#if devices.failed}
			<p>{UNREACHABLE}</p>
		{:else if devices.data}
			<ul>
				{#each devices.data as device (device.id)}
					<li>
						{device.name}
						{#if device.current}<strong>This device</strong>{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p>Loading…</p>
		{/if}
	</section>

	<section>
		<h2>Running jobs</h2>
		{#if jobs.failed}
			<p>{UNREACHABLE}</p>
		{:else if jobs.data}
			{#if jobs.data.length === 0}
				<p>No running job.</p>
			{:else}
				<ul>
					{#each jobs.data as job (job.id)}
						<li>
							{job.kind}: {job.done}{job.total === null ? '' : ` of ${job.total}`}
							{#if job.failed > 0}({job.failed} failed){/if}
						</li>
					{/each}
				</ul>
			{/if}
		{:else}
			<p>Loading…</p>
		{/if}
	</section>
</main>

<style>
	main {
		max-width: 40rem;
		margin: 2rem auto;
		padding: 0 1rem;
		font-family: system-ui, sans-serif;
	}
	.stream[data-state='connected'] {
		color: #1b6e2e;
	}
	strong {
		margin-left: 0.5rem;
	}
</style>
