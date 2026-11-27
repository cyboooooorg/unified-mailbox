// Fails unless build/index.html carries a Content-Security-Policy <meta>
// whose script-src holds a sha256 hash and no 'unsafe-inline'.
import { readFileSync } from 'node:fs';

const html = readFileSync('build/index.html', 'utf8');
const meta = [...html.matchAll(/<meta\b[^>]*>/gi)]
	.map((m) => m[0])
	.find((tag) => /http-equiv=["']content-security-policy["']/i.test(tag));
const content = meta?.match(/content="([^"]*)"/i)?.[1] ?? '';
const scriptSrc =
	content
		.split(';')
		.map((d) => d.trim())
		.find((d) => d.startsWith('script-src ')) ?? '';

if (!scriptSrc.includes("'sha256-") || scriptSrc.includes("'unsafe-inline'")) {
	console.error(
		`build/index.html has no hashed script-src without 'unsafe-inline': "${scriptSrc}"`
	);
	process.exit(1);
}
