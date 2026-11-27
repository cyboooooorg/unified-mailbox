// Fails when src/lib/api/schema.d.ts differs from what openapi-typescript
// generates from ../api/openapi.json.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const committed = 'src/lib/api/schema.d.ts';
const dir = mkdtempSync(join(tmpdir(), 'contract-'));
const fresh = join(dir, 'schema.d.ts');
try {
	execFileSync('npx', ['openapi-typescript', '../api/openapi.json', '-o', fresh], {
		stdio: 'ignore'
	});
	if (readFileSync(fresh, 'utf8') !== readFileSync(committed, 'utf8')) {
		console.error('schema.d.ts is stale: run npm run generate');
		process.exitCode = 1;
	}
} finally {
	rmSync(dir, { recursive: true, force: true });
}
