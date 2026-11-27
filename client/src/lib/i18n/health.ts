import type { components } from '../api/schema';
import { m } from '#lib/paraglide/messages.js';

export type HealthStatus = components['schemas']['HealthStatus'];

const HEALTH_LABELS: Record<HealthStatus, () => string> = {
	ok: m.status_health_ok
};

/** The translated label of a server status code. */
export function healthLabel(status: HealthStatus): string {
	return HEALTH_LABELS[status]();
}
