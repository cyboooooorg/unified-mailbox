<script lang="ts">
	import { genericError } from '../api/errors';
	import { languageSync } from './app';
	import { locale, type Language } from './locale.svelte';
	import { m } from '#lib/paraglide/messages.js';

	let error = $state<string | null>(null);
	let select = $state<HTMLSelectElement>();

	async function onchange(event: Event) {
		const language = (event.currentTarget as HTMLSelectElement).value as Language;
		if (await languageSync.choose(language)) {
			error = null;
		} else {
			error = genericError();
			if (select) select.value = locale.current;
		}
	}
</script>

<label>
	{m.language_label()}
	<select bind:this={select} value={locale.current} {onchange}>
		<option value="en">English</option>
		<option value="fr">Français</option>
	</select>
</label>
{#if error}
	<p role="alert">{error}</p>
{/if}
