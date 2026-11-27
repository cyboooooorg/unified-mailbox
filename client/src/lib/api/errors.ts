import type { components } from './schema';

export type ErrorCode = components['schemas']['ErrorCode'];

export const GENERIC_ERROR = 'Something went wrong. Try again.';

const MESSAGES: Record<ErrorCode, string> = {
	invalid_pairing_code: 'This code is invalid or expired.',
	unauthorized: 'This device is not signed in.',
	not_found: 'Not found.',
	method_not_allowed: 'This action is not allowed.',
	payload_too_large: 'The request is too large.',
	unsupported_media_type: 'The request format is not supported.',
	invalid_request: 'The request is invalid.',
	internal: GENERIC_ERROR
};

export function errorMessage(code: ErrorCode): string {
	return MESSAGES[code];
}

/** The message for a well-formed `{error: {code}}` body, else the generic one. */
export function describeError(body: unknown): string {
	if (typeof body === 'object' && body !== null && 'error' in body) {
		const error = body.error;
		if (typeof error === 'object' && error !== null && 'code' in error) {
			const code = error.code;
			if (typeof code === 'string' && Object.hasOwn(MESSAGES, code)) {
				return MESSAGES[code as ErrorCode];
			}
		}
	}
	return GENERIC_ERROR;
}
