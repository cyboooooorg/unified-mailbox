const MAX_LENGTH = 64;
const FALLBACK = 'Web browser';

// Order matters: Edge and Opera also say Chrome, and Chrome also says Safari.
const BROWSERS: readonly [RegExp, string][] = [
	[/Edg(?:e|A|iOS)?\//, 'Edge'],
	[/OPR\/|Opera/, 'Opera'],
	[/Firefox\/|FxiOS\//, 'Firefox'],
	[/Chrome\/|CriOS\//, 'Chrome'],
	[/Safari\//, 'Safari']
];

// Order matters: an iPhone says "like Mac OS X", Android says "Linux".
const SYSTEMS: readonly [RegExp, string][] = [
	[/iPhone/, 'iPhone'],
	[/iPad/, 'iPad'],
	[/Android/, 'Android'],
	[/Windows/, 'Windows'],
	[/Mac OS X|Macintosh/, 'macOS'],
	[/CrOS/, 'ChromeOS'],
	[/Linux|X11/, 'Linux']
];

/**
 * A name for this browser, such as "Firefox on Linux", to prefill the pairing
 * form. Built from the fixed names above, so it is always a valid name.
 */
export function suggestDeviceName(userAgent: string): string {
	const browser = BROWSERS.find(([pattern]) => pattern.test(userAgent))?.[1];
	if (!browser) return FALLBACK;
	const system = SYSTEMS.find(([pattern]) => pattern.test(userAgent))?.[1];
	return system ? `${browser} on ${system}` : browser;
}

/** The server's rule (`PairingRequest`): trimmed, 1 to 64 characters, no control character. */
export function validateDeviceName(name: string): string | null {
	const trimmed = name.trim();
	if (trimmed === '') return 'Enter a name.';
	if ([...trimmed].length > MAX_LENGTH) return 'At most 64 characters.';
	if (/\p{Cc}/u.test(trimmed)) return 'No control characters.';
	return null;
}
