import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { createRequire } from 'node:module';
import { generatedModuleMap, serializeModuleMap, writeRegistryFiles } from './lib/registry-sync.mjs';

const run = promisify(execFile);
const require = createRequire(import.meta.url);
const projectRoot = path.resolve(import.meta.dirname, '..');
const packageJson = JSON.parse(await readFile(path.join(projectRoot, 'package.json'), 'utf8'));
const version = packageJson.dependencies?.antd;
if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`Expected an exact Ant Design version, received ${version}.`);

const outputRoot = path.join(projectRoot, 'ui-src/library-runtime/vendor/antd');
const runtimeManifestFile = path.join(projectRoot, 'ui-src/library-runtime/antd-registry.generated.ts');
const catalogManifestFile = path.join(projectRoot, 'src/antd-registry.generated.ts');
const suppliedSourceRoot = process.env.ANTD_SOURCE_DIR ? path.resolve(process.env.ANTD_SOURCE_DIR) : undefined;
const temporaryRoot = suppliedSourceRoot ? undefined : await mkdtemp(path.join(tmpdir(), 'web-design-studio-antd-'));
const sourceRoot = suppliedSourceRoot ?? path.join(temporaryRoot, 'ant-design');

const excludedDemo = /(?:^|-)debug(?:-|$)|(?:^|-)semantic(?:-|$)|(?:^|-)component-token(?:-|$)/;
const componentPresentation = {
  affix: [520, 180], alert: [620, 260], anchor: [520, 300], app: [520, 220], 'auto-complete': [560, 240],
  avatar: [560, 220], badge: [560, 240], 'border-beam': [560, 280], breadcrumb: [600, 180], button: [620, 240],
  calendar: [620, 520], card: [620, 360], carousel: [640, 360], cascader: [600, 280], checkbox: [560, 240],
  collapse: [620, 340], 'color-picker': [580, 280], 'config-provider': [640, 360], 'date-picker': [640, 320],
  descriptions: [680, 360], divider: [620, 260], drawer: [560, 220], dropdown: [560, 240], empty: [520, 360],
  flex: [620, 300], 'float-button': [520, 260], form: [680, 520], grid: [680, 360], icon: [620, 300], image: [620, 400],
  input: [620, 320], 'input-number': [600, 300], layout: [680, 420], list: [680, 420], listy: [700, 460],
  masonry: [680, 440], mentions: [620, 300], menu: [680, 400], message: [520, 220], modal: [560, 220],
  notification: [520, 220], pagination: [640, 260], popconfirm: [560, 240], popover: [560, 240], progress: [620, 340],
  'qr-code': [560, 360], radio: [600, 280], rate: [560, 240], result: [620, 460], segmented: [620, 260],
  select: [620, 320], skeleton: [620, 340], slider: [620, 300], space: [620, 300], spin: [560, 260], splitter: [680, 400],
  statistic: [620, 300], steps: [680, 340], switch: [560, 220], table: [760, 480], tabs: [680, 380], tag: [620, 280],
  'time-picker': [620, 300], timeline: [620, 380], tooltip: [560, 240], tour: [600, 280], transfer: [720, 420],
  tree: [620, 440], 'tree-select': [620, 320], typography: [680, 420], upload: [680, 420], watermark: [640, 400]
};

function componentId(slug) {
  if (slug === 'qr-code') return 'QRCode';
  return slug.split('-').map((part) => `${part[0].toUpperCase()}${part.slice(1)}`).join('');
}

function importSpecifiers(source) {
  return [...source.matchAll(/import(?:[\s\S]*?from\s*)?["']([^"']+)["']/g)].map((match) => match[1]);
}

function canBundle(source) {
  return importSpecifiers(source).every((specifier) => {
    if (specifier.startsWith('.')) return false;
    try {
      require.resolve(specifier);
      return true;
    } catch {
      return false;
    }
  });
}

function exportedComponent(source) {
  if (/export\s+default\s+/.test(source)) return 'default';
  return source.match(/export\s+(?:function|const)\s+([A-Z][A-Za-z0-9_]*)/)?.[1];
}

function demoLabel(name) {
  if (name === 'basic') return '官方基础示例';
  return `官方 · ${name.split('-').map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`).join(' ')}`;
}

try {
  if (suppliedSourceRoot) {
    const { stdout } = await run('git', ['describe', '--tags', '--exact-match'], { cwd: sourceRoot });
    if (stdout.trim() !== version) throw new Error(`ANTD_SOURCE_DIR must point to Ant Design ${version}, received ${stdout.trim()}.`);
  } else {
    let cloneError;
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      try {
        await run('git', ['clone', '--depth', '1', '--filter=blob:none', '--branch', version, 'https://github.com/ant-design/ant-design.git', sourceRoot], {
          maxBuffer: 10 * 1024 * 1024
        });
        cloneError = undefined;
        break;
      } catch (error) {
        cloneError = error;
        await rm(sourceRoot, { recursive: true, force: true });
        if (attempt < 3) await new Promise((resolve) => setTimeout(resolve, attempt * 1000));
      }
    }
    if (cloneError) throw cloneError;
  }
  const componentsRoot = path.join(sourceRoot, 'components');
  const files = new Map();
  const entries = [];
  const componentDirectories = await readdir(componentsRoot, { withFileTypes: true });
  for (const directory of componentDirectories) {
    if (!directory.isDirectory() || directory.name === 'back-top') continue;
    const demoRoot = path.join(componentsRoot, directory.name, 'demo');
    if (!existsSync(demoRoot)) continue;
    const demos = [];
    for (const fileName of await readdir(demoRoot)) {
      if (!fileName.endsWith('.tsx')) continue;
      const name = fileName.slice(0, -4);
      if (excludedDemo.test(name) || !existsSync(path.join(demoRoot, `${name}.md`))) continue;
      const source = await readFile(path.join(demoRoot, fileName), 'utf8');
      const exportName = exportedComponent(source);
      if (!exportName || !canBundle(source)) continue;
      const relativePath = `${directory.name}/${fileName}`;
      files.set(relativePath, source);
      demos.push({
        id: `${directory.name}-${name}`,
        label: demoLabel(name),
        path: `./vendor/antd/${relativePath}`,
        export: exportName,
        type: 'official-demo'
      });
    }
    if (!demos.length) continue;
    demos.sort((a, b) => (a.id.endsWith('-basic') ? -1 : b.id.endsWith('-basic') ? 1 : a.id.localeCompare(b.id)));
    const [width, height] = componentPresentation[directory.name] ?? [620, 320];
    entries.push({
      slug: directory.name,
      title: componentId(directory.name),
      description: `Ant Design ${version} official demos`,
      rootPath: demos[0].path,
      rootExport: demos[0].export,
      previewPath: demos[0].path,
      previewExport: demos[0].export,
      demo: demos[0].id,
      demos,
      acceptsChildren: false,
      layout: 'intrinsic',
      previewHeight: height,
      previewSpan: width >= 680 ? 'wide' : 'normal',
      docsUrl: `https://ant.design/components/${directory.name}-cn`,
      editorWidth: width,
      editorHeight: height
    });
  }
  entries.sort((a, b) => a.slug.localeCompare(b.slug));
  await writeRegistryFiles({ outputRoot, files, alias: '@antd-registry' });
  const modulePaths = entries.flatMap((entry) => entry.demos.map((demo) => demo.path));
  const runtimeEntries = entries.map(({ editorWidth: _editorWidth, editorHeight: _editorHeight, ...entry }) => entry);
  const runtimeManifest = `// Generated from the official MIT-licensed Ant Design ${version} demo sources.\n` +
    `import type { ReactRegistryEntry } from './types';\n` +
    `export const ANTD_REGISTRY_ENTRIES = ${JSON.stringify(runtimeEntries, null, 2)} as const satisfies readonly ReactRegistryEntry[];\n` +
    `export const ANTD_REGISTRY_BY_SLUG = Object.fromEntries(ANTD_REGISTRY_ENTRIES.map((entry) => [entry.slug, entry])) as Record<string, ReactRegistryEntry>;\n` +
    `export const ANTD_REGISTRY_MODULES = ${serializeModuleMap(generatedModuleMap(modulePaths))};\n`;
  const variants = Object.fromEntries(entries.map((entry) => [componentId(entry.slug), entry.demos.map((demo) => ({
    id: demo.id,
    label: demo.label,
    props: { componentSlug: entry.slug, registryDemo: demo.id },
    width: entry.editorWidth,
    height: entry.editorHeight
  }))]));
  const catalogManifest = `// Generated from the official MIT-licensed Ant Design ${version} demo sources.\n` +
    `import type { UiComponentVariant } from './ui-library.js';\n` +
    `export const ANTD_OFFICIAL_COMPONENT_VARIANTS = ${JSON.stringify(variants, null, 2)} as const satisfies Record<string, readonly UiComponentVariant[]>;\n`;
  await mkdir(path.dirname(runtimeManifestFile), { recursive: true });
  await writeFile(runtimeManifestFile, runtimeManifest, 'utf8');
  await writeFile(catalogManifestFile, catalogManifest, 'utf8');
  await writeFile(path.join(outputRoot, 'LICENSE.txt'), await readFile(path.join(sourceRoot, 'LICENSE'), 'utf8'), 'utf8');
  console.log(`Synced ${entries.length} Ant Design components and ${entries.flatMap((entry) => entry.demos).length} official demos from ${version}.`);
} finally {
  if (temporaryRoot) await rm(temporaryRoot, { recursive: true, force: true });
}
