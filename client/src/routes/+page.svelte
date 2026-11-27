<script lang="ts">
	import { api } from '../lib/api/app';
	import { session } from '../lib/auth/session.svelte';
	import { connectEvents, type StreamState } from '../lib/events/stream';
	import { createStatusHandlers } from '../lib/status/handlers';
	import { createDeviceSync, createJobSync, type Loaded } from '../lib/status/sync';
	import { formatNumber } from '../lib/i18n/format';
	import { healthLabel } from '../lib/i18n/health';
	import { jobKindLabel } from '../lib/i18n/job-kind';
	import { languageSync } from '../lib/i18n/app';
	import LanguagePicker from '../lib/i18n/LanguagePicker.svelte';
	import { locale } from '../lib/i18n/locale.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import type { components } from '../lib/api/schema';

	type Health = components['schemas']['HealthDto'];
	type Device = components['schemas']['DeviceDto'];
	type Job = components['schemas']['JobDto'];

	const STREAM_LABELS: Record<StreamState, () => string> = {
		connecting: m.status_stream_connecting,
		connected: m.status_stream_connected,
		reconnecting: m.status_stream_reconnecting,
		stopped: m.status_stream_stopped
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

	const { onConnect, onEvent } = createStatusHandlers({
		loadHealth,
		deviceSync,
		jobSync,
		languageSync
	});

	$effect(() => {
		const controller = new AbortController();
		void connectEvents({
			tokens: session.tokens,
			onConnect,
			onEvent,
			onUnauthorized: () => session.signOut(),
			onState: (state) => (stream = state),
			signal: controller.signal
		});
		return () => controller.abort();
	});
</script>

<main>
	<h1>{m.app_title()}</h1>
	<p class="stream" data-state={stream}>
		{STREAM_LABELS[stream]()}
	</p>
	<LanguagePicker />

	<section>
		<h2>{m.status_server_heading()}</h2>
		{#if healthFailed}
			<p>{m.status_unreachable()}</p>
		{:else if health}
			<p>{m.status_server_line({ status: healthLabel(health.status), version: health.version })}</p>
		{:else}
			<p>{m.status_loading()}</p>
		{/if}
	</section>

	<section>
		<h2>{m.status_devices_heading()}</h2>
		{#if devices.failed}
			<p>{m.status_unreachable()}</p>
		{:else if devices.data}
			<ul>
				{#each devices.data as device (device.id)}
					<li>
						{device.name}
						{#if device.current}<strong>{m.status_device_current()}</strong>{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p>{m.status_loading()}</p>
		{/if}
	</section>

	<section>
		<h2>{m.status_jobs_heading()}</h2>
		{#if jobs.failed}
			<p>{m.status_unreachable()}</p>
		{:else if jobs.data}
			{#if jobs.data.length === 0}
				<p>{m.status_jobs_none()}</p>
			{:else}
				<ul>
					{#each jobs.data as job (job.id)}
						<li>
							{#if job.total === null}
								{m.status_job_progress_open({
									kind: jobKindLabel(job.kind),
									done: formatNumber(job.done, locale.current)
								})}
							{:else}
								{m.status_job_progress({
									kind: jobKindLabel(job.kind),
									done: formatNumber(job.done, locale.current),
									total: formatNumber(job.total, locale.current)
								})}
							{/if}
							{#if job.failed > 0}
								{m.status_job_failed({
									count: job.failed,
									countText: formatNumber(job.failed, locale.current)
								})}
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		{:else}
			<p>{m.status_loading()}</p>
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
