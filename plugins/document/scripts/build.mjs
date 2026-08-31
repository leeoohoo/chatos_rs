import { build } from 'esbuild';
import { chmod, copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const packageJson = JSON.parse(await readFile(path.join(projectRoot, 'package.json'), 'utf8'));

await mkdir(path.join(projectRoot, 'dist'), { recursive: true });
await mkdir(path.join(projectRoot, '.build'), { recursive: true });
const buildResult = await build({
  entryPoints: [path.join(projectRoot, 'src', 'server.ts')],
  outfile: path.join(projectRoot, 'dist', 'server.mjs'),
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node18',
  external: ['canvas', 'path2d'],
  metafile: true,
  sourcemap: false,
  legalComments: 'none',
  define: {
    __PACKAGE_VERSION__: JSON.stringify(packageJson.version)
  }
});
await writeFile(
  path.join(projectRoot, '.build', 'server-metafile.json'),
  `${JSON.stringify(buildResult.metafile, null, 2)}\n`
);
await copyFile(
  path.join(projectRoot, 'node_modules', 'pdfjs-dist', 'legacy', 'build', 'pdf.worker.mjs'),
  path.join(projectRoot, 'dist', 'pdf.worker.mjs')
);
await copyFile(
  path.join(projectRoot, 'node_modules', '@hyzyla', 'pdfium', 'dist', 'pdfium.wasm'),
  path.join(projectRoot, 'dist', 'pdfium.wasm')
);

await chmod(path.join(projectRoot, 'bin', 'chatos-document-mcp'), 0o755);
