/** Labels of the job kinds the server runs; none exists in production yet. */
export const JOB_KIND_LABELS: Record<string, () => string> = {};

/** The translated label of a job kind, or the raw kind when it has none. */
export function jobKindLabel(
	kind: string,
	labels: Record<string, () => string> = JOB_KIND_LABELS
): string {
	return Object.hasOwn(labels, kind) ? labels[kind]() : kind;
}
