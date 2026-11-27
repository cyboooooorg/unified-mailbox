import type { Language } from './locale.svelte';

export interface LanguageSyncDeps {
	/** `undefined` when the request failed. */
	get(): Promise<Language | null | undefined>;
	/** `true` when the server stored it. */
	put(language: Language): Promise<boolean>;
	current(): Language;
	apply(language: Language): void;
	preferred(): readonly string[];
}

/** The first preferred language whose primary subtag is supported, else English. */
export function browserLanguage(preferred: readonly string[]): Language {
	for (const tag of preferred) {
		const primary = tag.toLowerCase().split('-')[0];
		if (primary === 'en' || primary === 'fr') return primary;
	}
	return 'en';
}

/**
 * The server holds the language; the locale (and its localStorage cache) follows it.
 * The first device to connect stores its browser language.
 */
export function createLanguageSync(deps: LanguageSyncDeps) {
	return {
		async sync(): Promise<void> {
			const server = await deps.get();
			if (server === undefined) return;
			if (server === null) {
				const language = browserLanguage(deps.preferred());
				if (await deps.put(language)) deps.apply(language);
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
