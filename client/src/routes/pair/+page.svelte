<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '../../lib/api/app';
	import { describeError, genericError } from '../../lib/api/errors';
	import { session } from '../../lib/auth/session.svelte';
	import { suggestDeviceName, validateDeviceName } from '../../lib/auth/device-name';

	let code = $state('');
	let name = $state(suggestDeviceName(globalThis.navigator?.userAgent ?? ''));
	let error = $state<string | null>(null);
	let busy = $state(false);

	async function pair(event: SubmitEvent) {
		event.preventDefault();
		error = validateDeviceName(name);
		if (error) return;
		busy = true;
		try {
			const { data, error: body } = await api.POST('/api/v1/pairing', {
				body: { code: code.trim(), name: name.trim() }
			});
			if (data) {
				session.signIn(data.token);
				// eslint-disable-next-line svelte/no-navigation-without-resolve
				await goto('/', { replaceState: true });
			} else {
				error = describeError(body);
			}
		} catch {
			error = genericError();
		} finally {
			busy = false;
		}
	}
</script>

<main>
	<h1>Pair this device</h1>
	<p>Print a code on the server with <code>unified-mailbox pair-code</code>, then enter it here.</p>
	<form onsubmit={pair}>
		<label>
			Pairing code
			<input bind:value={code} autocomplete="off" autocapitalize="characters" required />
		</label>
		<label>
			Device name
			<input bind:value={name} maxlength="64" required />
		</label>
		{#if error}
			<p role="alert">{error}</p>
		{/if}
		<button type="submit" disabled={busy}>Pair</button>
	</form>
</main>

<style>
	main {
		max-width: 28rem;
		margin: 2rem auto;
		padding: 0 1rem;
		font-family: system-ui, sans-serif;
	}
	form {
		display: grid;
		gap: 1rem;
	}
	label {
		display: grid;
		gap: 0.25rem;
	}
	[role='alert'] {
		color: #b00020;
		margin: 0;
	}
</style>
