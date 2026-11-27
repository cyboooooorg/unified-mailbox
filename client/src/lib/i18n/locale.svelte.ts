import type { components } from '../api/schema';
import { getLocale, overwriteGetLocale, setLocale } from '#lib/paraglide/runtime.js';

export type Language = components['schemas']['LanguageDto'];

/**
 * Each language written in itself, never translated. A language added to the
 * server's `LanguageDto` fails `npm run check` here until it has a name.
 */
export const LANGUAGE_NAMES: Record<Language, string> = { en: 'English', fr: 'Français' };

function createLocale() {
	// Paraglide's own resolution (localStorage, then the browser language, then
	// English), read once before it is overridden below.
	let current = $state<Language>(getLocale());

	// Every `m.*()` call resolves the locale through `getLocale`: pointing it at
	// the `$state` makes a template re-render when the language changes, with no
	// remount.
	overwriteGetLocale(() => current);

	return {
		get current(): Language {
			return current;
		},
		/** Persists the choice in Paraglide's localStorage entry and re-renders. */
		set(language: Language): void {
			void setLocale(language, { reload: false });
			current = language;
			if (typeof document !== 'undefined') {
				document.documentElement.lang = language;
			}
		}
	};
}

export const locale = createLocale();
