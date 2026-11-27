import { api } from '../api/app';
import { createRemoteLanguageSync } from './remote';
import { locale } from './locale.svelte';

/** The app-wide language sync, built on the app-wide `api` and `locale`. */
export const languageSync = createRemoteLanguageSync(api, locale);
