import { build } from 'esbuild';
import { chmod } from 'node:fs/promises';

const common = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node18',
  sourcemap: false,
  banner: {
    js: 'import { createRequire } from "node:module"; const require = createRequire(import.meta.url);'
  }
};

await Promise.all([
  build({ ...common, entryPoints: ['src/mcp-server.ts'], outfile: 'dist/mcp-server.mjs' }),
  build({ ...common, entryPoints: ['src/studio-server.ts'], outfile: 'dist/studio-server.mjs' }),
  build({ ...common, entryPoints: ['src/schema.ts'], outfile: 'dist/schema.mjs' }),
  build({ ...common, entryPoints: ['src/store.ts'], outfile: 'dist/store.mjs' }),
  build({ ...common, entryPoints: ['src/runtime-context.ts'], outfile: 'dist/runtime-context.mjs' })
]);
await chmod('bin/chatos-solution-studio', 0o755);
