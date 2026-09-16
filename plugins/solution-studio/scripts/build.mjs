import { build } from 'esbuild';
import { chmod } from 'node:fs/promises';

await Promise.all([
  build({ entryPoints: ['src/mcp-server.ts'], outfile: 'dist/mcp-server.mjs', bundle: true, platform: 'node', format: 'esm', target: 'node18', packages: 'external' }),
  build({ entryPoints: ['src/studio-server.ts'], outfile: 'dist/studio-server.mjs', bundle: true, platform: 'node', format: 'esm', target: 'node18', packages: 'external' }),
  build({ entryPoints: ['src/schema.ts'], outfile: 'dist/schema.mjs', bundle: true, platform: 'node', format: 'esm', target: 'node18', packages: 'external' }),
  build({ entryPoints: ['src/store.ts'], outfile: 'dist/store.mjs', bundle: true, platform: 'node', format: 'esm', target: 'node18', packages: 'external' }),
  build({ entryPoints: ['src/runtime-context.ts'], outfile: 'dist/runtime-context.mjs', bundle: true, platform: 'node', format: 'esm', target: 'node18', packages: 'external' })
]);
await chmod('bin/chatos-solution-studio', 0o755);
