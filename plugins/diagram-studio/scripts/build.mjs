import { build } from 'esbuild';

const common = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node18',
  sourcemap: false,
  packages: 'external'
};

await Promise.all([
  build({
    ...common,
    entryPoints: ['src/mcp-server.ts'],
    outfile: 'dist/mcp-server.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/studio-server.ts'],
    outfile: 'dist/studio-server.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/templates.ts'],
    outfile: 'dist/test-helpers.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/document-store.ts'],
    outfile: 'dist/document-store.test.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/plantuml.ts'],
    outfile: 'dist/plantuml.test.mjs'
  })
]);
