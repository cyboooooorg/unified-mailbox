import { api } from '../api/app';
import { createLanguageSync } from './sync';
import { locale } from './locale.svelte';

/** The app-wide language sync, built on the app-wide `api` and `locale`. */
export const languageSync = createLanguageSync({
	async get() {
		try {
			const { data } = await api.GET('/api/v1/settings/language');
			return data ? data.language : undefined;
		} catch {
			return undefined;
		}
	},
	async put(language) {
		try {
			const { data } = await api.PUT('/api/v1/settings/language', { body: { language } });
			return data !== undefined;
		} catch {
			return false;
		}
	},
	current: () => locale.current,
	apply: (language) => locale.set(language),
	preferred: () => navigator.languages ?? []
});
