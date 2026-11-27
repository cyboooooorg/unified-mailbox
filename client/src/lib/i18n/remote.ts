import type { createApi } from '../api/client';
import { createLanguageSync } from './sync';
import type { Language } from './locale.svelte';

type Api = Pick<ReturnType<typeof createApi>, 'GET' | 'PUT'>;

/** A language sync, over the settings endpoint, on the given `api` and `locale`. */
export function createRemoteLanguageSync(
	api: Api,
	locale: { current: Language; set(language: Language): void }
) {
	return createLanguageSync({
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
		apply: (language) => locale.set(language)
	});
}
