import type { components } from '../api/schema';

type EventDto = components['schemas']['EventDto'];
type Job = components['schemas']['JobDto'];

export interface StatusHandlerDeps {
	loadHealth(): Promise<void>;
	deviceSync: { load(): Promise<unknown>; deviceRevoked(): void };
	jobSync: { load(): Promise<unknown>; apply(job: Job): void };
	languageSync: { sync(): Promise<void> };
}

/** What the status page does when the event stream connects and when an event arrives. */
export function createStatusHandlers(deps: StatusHandlerDeps) {
	return {
		/** Every (re)connect, `resync` included: reload everything. */
		onConnect: (): void => {
			void deps.loadHealth();
			void deps.deviceSync.load();
			void deps.jobSync.load();
			void deps.languageSync.sync();
		},
		onEvent: (event: EventDto): void => {
			if (event.type === 'job') deps.jobSync.apply(event.job);
			else if (event.type === 'device_revoked') deps.deviceSync.deviceRevoked();
			else if (event.type === 'settings_changed') void deps.languageSync.sync();
			// `resync` already reloaded: the reader called `onConnect`.
		}
	};
}
