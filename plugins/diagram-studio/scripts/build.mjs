import { build } from 'esbuild';

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
  }),
  build({
    ...common,
    entryPoints: ['src/layers.ts'],
    outfile: 'dist/layers.test.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/quality.ts'],
    outfile: 'dist/quality.test.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/layout.ts'],
    outfile: 'dist/layout.test.mjs'
  }),
  build({
    ...common,
    entryPoints: ['src/generation-guides.ts'],
    outfile: 'dist/generation-guides.test.mjs'
  })
]);
