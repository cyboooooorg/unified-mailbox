const KEY = 'unified-mailbox.token';

/** Where the device token lives. */
export interface TokenStore {
	get(): string | null;
	set(token: string): void;
	clear(): void;
}

/**
 * A token store over `localStorage`. The storage is reached lazily, inside the
 * `try`: merely accessing `localStorage` can throw (blocked storage).
 */
export function localTokenStore(storage: () => Storage = () => localStorage): TokenStore {
	return {
		get() {
			try {
				return storage().getItem(KEY);
			} catch {
				return null;
			}
		},
		set(token) {
			try {
				storage().setItem(KEY, token);
			} catch {
				// Storage unavailable: the session lasts until the page closes.
			}
		},
		clear() {
			try {
				storage().removeItem(KEY);
			} catch {
				// Nothing to clear.
			}
		}
	};
}
