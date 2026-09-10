import { build } from 'esbuild';
import { chmod, rm } from 'node:fs/promises';
// Only generated output belonging to this plugin; never the host's plugin data directory.
await rm(new URL('../dist', import.meta.url), { recursive: true, force: true });
await build({
  entryPoints: { mcp: 'src/mcp.ts', studio: 'src/studio.ts', store: 'src/store.ts', http: 'src/http.ts' },
  outdir: 'dist', outExtension: { '.js': '.mjs' }, bundle: true, splitting: true, platform: 'node', format: 'esm', target: 'node22',
  banner: { js: 'import { createRequire } from "node:module"; const require = createRequire(import.meta.url);' }
});
await chmod('bin/chatos-project-management', 0o755);
