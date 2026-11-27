<script lang="ts">
	import { api } from '../lib/api/app';
	import { session } from '../lib/auth/session.svelte';
	import { connectEvents, type StreamState } from '../lib/events/stream';
	import type { components } from '../lib/api/schema';

	type Health = components['schemas']['HealthDto'];
	type Device = components['schemas']['DeviceDto'];
	type Job = components['schemas']['JobDto'];
	type EventDto = components['schemas']['EventDto'];

	const UNREACHABLE = 'Server unreachable';

	// Each part loads on its own: a failure shows in that part only.
	let health = $state<Health | null>(null);
	let healthFailed = $state(false);
	let devices = $state<Device[] | null>(null);
	let devicesFailed = $state(false);
	let jobs = $state<Job[] | null>(null);
	let jobsFailed = $state(false);
	let stream = $state<StreamState>('connecting');

	async function loadHealth() {
		try {
			const { data } = await api.GET('/api/v1/health');
			health = data ?? null;
			healthFailed = !data;
		} catch {
			healthFailed = true;
		}
	}

	async function loadDevices() {
		try {
			const { data } = await api.GET('/api/v1/devices');
			if (data) devices = data;
			devicesFailed = !data;
		} catch {
			devicesFailed = true;
		}
	}

	async function loadJobs() {
		try {
			const { data } = await api.GET('/api/v1/jobs');
			if (data) jobs = data;
			jobsFailed = !data;
		} catch {
			jobsFailed = true;
		}
	}

	function reload() {
		void loadHealth();
		void loadDevices();
		void loadJobs();
	}

	function onEvent(event: EventDto) {
		if (event.type === 'job') {
			const job = event.job;
			const others = (jobs ?? []).filter((known) => known.id !== job.id);
			// The event is a hint: the next reload is the truth.
			if (job.status === 'running') {
				const at = (jobs ?? []).findIndex((known) => known.id === job.id);
				jobs = at < 0 ? [...others, job] : (jobs ?? []).map((k) => (k.id === job.id ? job : k));
			} else {
				jobs = others;
			}
		} else if (event.type === 'device_revoked') {
			void loadDevices();
		}
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
		{stream === 'connected' ? 'Connected' : 'Reconnecting…'}
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
		{#if devicesFailed}
			<p>{UNREACHABLE}</p>
		{:else if devices}
			<ul>
				{#each devices as device (device.id)}
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
		{#if jobsFailed}
			<p>{UNREACHABLE}</p>
		{:else if jobs}
			{#if jobs.length === 0}
				<p>No running job.</p>
			{:else}
				<ul>
					{#each jobs as job (job.id)}
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
