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
  build({ ...common, entryPoints: ['src/mcp-server.ts'], outfile: 'dist/mcp-server.mjs' }),
  build({ ...common, entryPoints: ['src/studio-server.ts'], outfile: 'dist/studio-server.mjs' }),
  build({ ...common, entryPoints: ['src/schema.ts'], outfile: 'dist/schema.test.mjs' }),
  build({ ...common, entryPoints: ['src/editor-model.ts'], outfile: 'dist/editor-model.test.mjs' }),
  build({ ...common, entryPoints: ['src/html-exporter.ts'], outfile: 'dist/html-exporter.test.mjs' }),
  build({ ...common, entryPoints: ['src/react-exporter.ts'], outfile: 'dist/react-exporter.test.mjs' }),
  build({ ...common, entryPoints: ['src/vue-exporter.ts'], outfile: 'dist/vue-exporter.test.mjs' }),
  build({ ...common, entryPoints: ['src/component-library.ts'], outfile: 'dist/component-library.test.mjs' }),
  build({ ...common, entryPoints: ['src/antd-library.ts'], outfile: 'dist/antd-library.test.mjs' }),
  build({ ...common, entryPoints: ['src/antd-slots.ts'], outfile: 'dist/antd-slots.test.mjs' }),
  build({ ...common, entryPoints: ['src/ui-library.ts'], outfile: 'dist/ui-library.test.mjs' }),
  build({ ...common, entryPoints: ['src/chakra-library.ts'], outfile: 'dist/chakra-library.test.mjs' }),
  build({ ...common, entryPoints: ['src/shadcn-library.ts'], outfile: 'dist/shadcn-library.test.mjs' }),
  build({ ...common, entryPoints: ['src/ui-libraries.ts'], outfile: 'dist/ui-libraries.test.mjs' }),
  build({ ...common, entryPoints: ['src/library-slots.ts'], outfile: 'dist/library-slots.test.mjs' }),
  build({ ...common, entryPoints: ['src/viewport-presets.ts'], outfile: 'dist/viewport-presets.test.mjs' }),
  build({ ...common, entryPoints: ['src/templates.ts'], outfile: 'dist/templates.test.mjs' }),
  build({ ...common, entryPoints: ['src/document-store.ts'], outfile: 'dist/document-store.test.mjs' })
]);
