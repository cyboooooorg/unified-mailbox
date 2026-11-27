import { createApi } from './client';
import { session } from '../auth/session.svelte';

export const api = createApi({
	tokens: session.tokens,
	onUnauthorized: () => session.signOut()
});
