import { describe, expect, it } from 'vitest';
import { localTokenStore } from './token-store';

function fakeStorage(): Storage {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k)
	} as unknown as Storage;
}

function throwingStorage(): Storage {
	const fail = () => {
		throw new Error('blocked');
	};
	return { getItem: fail, setItem: fail, removeItem: fail } as unknown as Storage;
}

describe('localTokenStore', () => {
	it('returns the token that was set', () => {
		const storage = fakeStorage();
		const store = localTokenStore(() => storage);
		store.set('tok-1');
		expect(store.get()).toBe('tok-1');
	});

	it('returns null after clear', () => {
		const storage = fakeStorage();
		const store = localTokenStore(() => storage);
		store.set('tok-1');
		store.clear();
		expect(store.get()).toBeNull();
	});

	it('keeps the token under its key', () => {
		const storage = fakeStorage();
		localTokenStore(() => storage).set('tok-1');
		expect(storage.getItem('unified-mailbox.token')).toBe('tok-1');
	});

	it('get returns null when storage throws', () => {
		expect(localTokenStore(() => throwingStorage()).get()).toBeNull();
	});

	it('set and clear do not throw when storage throws', () => {
		const store = localTokenStore(() => throwingStorage());
		expect(() => store.set('x')).not.toThrow();
		expect(() => store.clear()).not.toThrow();
	});

	it('does not throw when reaching the storage throws', () => {
		const store = localTokenStore(() => {
			throw new Error('denied');
		});
		expect(store.get()).toBeNull();
		expect(() => store.set('x')).not.toThrow();
	});
});
