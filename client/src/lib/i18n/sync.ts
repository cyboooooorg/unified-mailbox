import type { Language } from './locale.svelte';

export interface LanguageSyncDeps {
	/** `undefined` when the request failed. */
	get(): Promise<Language | null | undefined>;
	/** `true` when the server stored it. */
	put(language: Language): Promise<boolean>;
	current(): Language;
	apply(language: Language): void;
}

/**
 * The server holds the language; the locale (and its localStorage cache) follows it.
 * The first device to connect stores the language it shows (Paraglide's own
 * resolution picked it: localStorage, then the browser language).
 */
export function createLanguageSync(deps: LanguageSyncDeps) {
	return {
		async sync(): Promise<void> {
			const server = await deps.get();
			if (server === undefined) return;
			if (server === null) {
				// Already on screen: nothing to apply.
				await deps.put(deps.current());
			} else if (server !== deps.current()) {
				deps.apply(server);
			}
		},
		async choose(language: Language): Promise<boolean> {
			const stored = await deps.put(language);
			if (stored) deps.apply(language);
			return stored;
		}
	};
}
