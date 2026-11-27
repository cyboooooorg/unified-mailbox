import { m } from '#lib/paraglide/messages.js';
import type { components } from './schema';

export type ErrorCode = components['schemas']['ErrorCode'];

/** A `Record` over every code: a new code without a message fails `npm run check`. */
const MESSAGES: Record<ErrorCode, () => string> = {
	invalid_pairing_code: () => m.error_invalid_pairing_code(),
	unauthorized: () => m.error_unauthorized(),
	not_found: () => m.error_not_found(),
	method_not_allowed: () => m.error_method_not_allowed(),
	payload_too_large: () => m.error_payload_too_large(),
	unsupported_media_type: () => m.error_unsupported_media_type(),
	invalid_request: () => m.error_invalid_request(),
	internal: () => m.error_internal()
};

/** The message for any failure that has no known code, in the current language. */
export function genericError(): string {
	return m.error_generic();
}

export function errorMessage(code: ErrorCode): string {
	return MESSAGES[code]();
}

/** The message for a well-formed `{error: {code}}` body, else the generic one. */
export function describeError(body: unknown): string {
	if (typeof body === 'object' && body !== null && 'error' in body) {
		const error = body.error;
		if (typeof error === 'object' && error !== null && 'code' in error) {
			const code = error.code;
			if (typeof code === 'string' && Object.hasOwn(MESSAGES, code)) {
				return errorMessage(code as ErrorCode);
			}
		}
	}
	return genericError();
}
