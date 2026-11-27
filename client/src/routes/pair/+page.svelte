<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '../../lib/api/app';
	import { describeError, genericError } from '../../lib/api/errors';
	import { session } from '../../lib/auth/session.svelte';
	import { m } from '#lib/paraglide/messages.js';
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
	<h1>{m.pair_title()}</h1>
	<p>{m.pair_intro_before()} <code>unified-mailbox pair-code</code>{m.pair_intro_after()}</p>
	<form onsubmit={pair}>
		<label>
			{m.pair_code_label()}
			<input bind:value={code} autocomplete="off" autocapitalize="characters" required />
		</label>
		<label>
			{m.pair_name_label()}
			<input bind:value={name} maxlength="64" required />
		</label>
		{#if error}
			<p role="alert">{error}</p>
		{/if}
		<button type="submit" disabled={busy}>{m.pair_submit()}</button>
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
