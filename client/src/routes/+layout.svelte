<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import favicon from '#lib/assets/favicon.svg';
	import { session } from '../lib/auth/session.svelte';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const onPairPage = $derived(page.url.pathname === '/pair');

	// The routes are fixed app paths, so there is nothing to resolve against a base.
	$effect(() => {
		if (!session.signedIn && !onPairPage) {
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			void goto('/pair', { replaceState: true });
		} else if (session.signedIn && onPairPage) {
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			void goto('/', { replaceState: true });
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

{#if session.signedIn !== onPairPage}
	{@render children()}
{/if}

<style>
	/* SvelteKit styles its route announcer with a style attribute, which the CSP
	   blocks (no style-src): the same rules here keep it visually hidden. */
	:global(#svelte-announcer) {
		position: absolute;
		left: 0;
		top: 0;
		clip-path: inset(50%);
		overflow: hidden;
		white-space: nowrap;
		width: 1px;
		height: 1px;
	}
</style>
