import { goto } from '$app/navigation';
import { localTokenStore, type TokenStore } from './token-store';

export type Session = {
	readonly signedIn: boolean;
	readonly tokens: TokenStore;
	signIn(token: string): void;
	signOut(): void;
};

export function createSession(tokens: TokenStore, navigate: (path: string) => void): Session {
	let signedIn = $state(tokens.get() !== null);
	return {
		get signedIn() {
			return signedIn;
		},
		tokens,
		signIn(token) {
			tokens.set(token);
			signedIn = true;
		},
		signOut() {
			tokens.clear();
			signedIn = false;
			navigate('/pair');
		}
	};
}

// The path is a fixed app route, so there is nothing to resolve against a base.
// eslint-disable-next-line svelte/no-navigation-without-resolve
export const session = createSession(localTokenStore(), (path) => void goto(path));
