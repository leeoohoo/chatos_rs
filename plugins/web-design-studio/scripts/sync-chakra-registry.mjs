import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fetchJson, fetchText, generatedModuleMap, serializeModuleMap, writeRegistryFiles } from './lib/registry-sync.mjs';

const require = createRequire(import.meta.url);
const projectRoot = path.resolve(import.meta.dirname, '..');
const packageJson = JSON.parse(await readFile(path.join(projectRoot, 'package.json'), 'utf8'));
const version = packageJson.dependencies?.['@chakra-ui/react'];
if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`Expected an exact Chakra UI version, received ${version}.`);

const outputRoot = path.join(projectRoot, 'ui-src/library-runtime/vendor/chakra/compositions');
const runtimeManifestFile = path.join(projectRoot, 'ui-src/library-runtime/chakra-registry.generated.ts');
const catalogManifestFile = path.join(projectRoot, 'src/chakra-registry.generated.ts');
const sourceCatalog = await readFile(path.join(projectRoot, 'src/chakra-library.ts'), 'utf8');
const componentBlock = sourceCatalog.slice(sourceCatalog.indexOf('export const CHAKRA_COMPONENTS'), sourceCatalog.indexOf('export const CHAKRA_LIBRARY'));
const componentMatches = [...componentBlock.matchAll(/item\('([^']+)',\s*'([^']+)',\s*'([^']+)',\s*'[^']*',\s*'[^']+',\s*(\d+),\s*(\d+)/g)];
const definitions = componentMatches.map((match) => ({ id: match[1], title: match[2], category: match[3], width: Number(match[4]), height: Number(match[5]) }));

function kebab(id) {
  if (id === 'QRCode') return 'qr-code';
  return id.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase();
}

const definitionsById = new Map(definitions.map((definition) => [definition.id, definition]));
const definitionsBySlug = definitions.map((definition) => ({ ...definition, slug: kebab(definition.id) })).sort((a, b) => b.slug.length - a.slug.length);
const internalExample = /(?:^|-)(?:debug|test|explorer-demo)(?:-|$)/;

function definitionForExample(name) {
  if (name.startsWith('date-picker-calendar-')) return definitionsById.get('Calendar');
  if (name === 'radio' || (name.startsWith('radio-') && !name.startsWith('radio-card-'))) return definitionsById.get('RadioGroup');
  if (name === 'toaster' || name.startsWith('toaster-')) return definitionsById.get('Toast');
  if (name === 'overlay' || name.startsWith('overlay-')) return definitionsById.get('OverlayManager');
  return definitionsBySlug.find((definition) => name === definition.slug || name.startsWith(`${definition.slug}-`));
}

function exportedComponent(source) {
  return source.match(/export\s+(?:const|function)\s+([A-Z][A-Za-z0-9_]*)/)?.[1]
    ?? (/export\s+default\s+/.test(source) ? 'default' : undefined);
}

function candidatePaths(from, specifier) {
  const base = specifier.startsWith('compositions/')
    ? specifier.slice('compositions/'.length)
    : path.posix.normalize(path.posix.join(path.posix.dirname(from), specifier));
  return [base, `${base}.ts`, `${base}.tsx`, `${base}.js`, `${base}.jsx`, `${base}/index.ts`, `${base}/index.tsx`];
}

function importsFor(source) {
  return [...source.matchAll(/import(?:[\s\S]*?from\s*)?["']([^"']+)["']/g)].map((match) => match[1]);
}

function canBundle(relativePath, source, files) {
  return importsFor(source).every((specifier) => {
    if (specifier.startsWith('.') || specifier.startsWith('compositions/')) return candidatePaths(relativePath, specifier).some((candidate) => files.has(candidate));
    try {
      require.resolve(specifier);
      return true;
    } catch {
      return false;
    }
  });
}

function label(name, slug) {
  const suffix = name === slug ? 'basic' : name.slice(slug.length + 1);
  if (suffix === 'basic') return '官方基础示例';
  return `官方 · ${suffix.split('-').map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`).join(' ')}`;
}

async function concurrentMap(values, concurrency, operation) {
  const result = new Array(values.length);
  let cursor = 0;
  await Promise.all(Array.from({ length: Math.min(concurrency, values.length) }, async () => {
    while (cursor < values.length) {
      const index = cursor++;
      result[index] = await operation(values[index], index);
    }
  }));
  return result;
}

const encodedTag = encodeURIComponent(`@chakra-ui/react@${version}`);
const refs = await fetchJson(`https://api.github.com/repos/chakra-ui/chakra-ui/git/matching-refs/tags/${encodedTag}`);
const ref = refs.find((candidate) => candidate.ref === `refs/tags/@chakra-ui/react@${version}`);
if (!ref) throw new Error(`Unable to resolve Chakra UI ${version}.`);
const tag = ref.object.type === 'tag' ? await fetchJson(ref.object.url) : ref.object;
const commit = tag.object?.sha ?? tag.sha;
const tree = await fetchJson(`https://api.github.com/repos/chakra-ui/chakra-ui/git/trees/${commit}?recursive=1`);
if (tree.truncated) throw new Error('The Chakra UI source tree response was truncated.');

const exampleSources = tree.tree
  .filter((entry) => entry.type === 'blob' && entry.path.startsWith('apps/compositions/src/examples/') && entry.path.endsWith('.tsx'))
  .map((entry) => ({ ...entry, name: path.posix.basename(entry.path, '.tsx') }))
  .map((entry) => ({ ...entry, definition: definitionForExample(entry.name) }))
  .filter((entry) => entry.definition && !internalExample.test(entry.name));
const uiSources = tree.tree.filter((entry) => entry.type === 'blob' && entry.path.startsWith('apps/compositions/src/ui/') && /\.[jt]sx?$/.test(entry.path));
const libSources = tree.tree.filter((entry) => entry.type === 'blob' && entry.path.startsWith('apps/compositions/src/lib/') && /\.[jt]sx?$/.test(entry.path));
const requestedSources = [...exampleSources, ...uiSources, ...libSources];
const cdnRoot = `https://cdn.jsdelivr.net/gh/chakra-ui/chakra-ui@${commit}/apps/compositions/src`;
const fetchedSources = await concurrentMap(requestedSources, 32, async (entry) => {
  const relativePath = entry.path.slice('apps/compositions/src/'.length);
  const source = (await fetchText(`${cdnRoot}/${relativePath}`))
    .replaceAll('"@/components/ui/', '"compositions/ui/')
    .replaceAll("'@/components/ui/", "'compositions/ui/");
  return { entry, relativePath, source };
});
const files = new Map(fetchedSources.map(({ relativePath, source }) => [relativePath, source]));

const demosByComponent = new Map(definitions.map((definition) => [definition.id, []]));
for (const { entry, relativePath, source } of fetchedSources.slice(0, exampleSources.length)) {
  const exportName = exportedComponent(source);
  if (!exportName || !canBundle(relativePath, source, files)) continue;
  const slug = kebab(entry.definition.id);
  demosByComponent.get(entry.definition.id).push({
    id: `chakra-${entry.name}`,
    label: label(entry.name, slug),
    path: `./vendor/chakra/compositions/${relativePath}`,
    export: exportName,
    type: 'official-demo'
  });
}

const entries = [];
for (const definition of definitions) {
  const demos = demosByComponent.get(definition.id);
  if (!demos?.length) continue;
  demos.sort((a, b) => (a.label === '官方基础示例' ? -1 : b.label === '官方基础示例' ? 1 : a.id.localeCompare(b.id)));
  const width = Math.max(definition.width, ['Table', 'RichTextEditor', 'CodeBlock', 'DatePicker', 'Calendar', 'TreeView', 'Carousel'].includes(definition.id) ? 720 : 560);
  const height = Math.max(definition.height, ['DatePicker', 'Calendar'].includes(definition.id) ? 520 : ['Table', 'RichTextEditor', 'CodeBlock', 'TreeView', 'Carousel'].includes(definition.id) ? 420 : 260);
  entries.push({
    slug: kebab(definition.id),
    title: definition.id,
    description: `Chakra UI ${version} official examples`,
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
    docsUrl: `https://chakra-ui.com/docs/components/${kebab(definition.id)}`,
    editorWidth: width,
    editorHeight: height
  });
}

const usedFiles = new Map();
for (const entry of entries) for (const demo of entry.demos) {
  const relativePath = demo.path.replace('./vendor/chakra/compositions/', '');
  usedFiles.set(relativePath, files.get(relativePath));
}
for (const [relativePath, source] of files) {
  if (relativePath.startsWith('ui/') || relativePath.startsWith('lib/')) usedFiles.set(relativePath, source);
}
await writeRegistryFiles({ outputRoot, files: usedFiles, alias: '@chakra-registry' });

const modulePaths = entries.flatMap((entry) => entry.demos.map((demo) => demo.path));
const runtimeEntries = entries.map(({ editorWidth: _editorWidth, editorHeight: _editorHeight, ...entry }) => entry);
const runtimeManifest = `// Generated from the official MIT-licensed Chakra UI ${version} compositions.\n` +
  `import type { ReactRegistryEntry } from './types';\n` +
  `export const CHAKRA_REGISTRY_ENTRIES = ${JSON.stringify(runtimeEntries, null, 2)} as const satisfies readonly ReactRegistryEntry[];\n` +
  `export const CHAKRA_REGISTRY_BY_SLUG = Object.fromEntries(CHAKRA_REGISTRY_ENTRIES.map((entry) => [entry.slug, entry])) as Record<string, ReactRegistryEntry>;\n` +
  `export const CHAKRA_REGISTRY_MODULES = ${serializeModuleMap(generatedModuleMap(modulePaths))};\n`;
const variants = Object.fromEntries(entries.map((entry) => [entry.title, entry.demos.map((demo) => ({
  id: demo.id,
  label: demo.label,
  props: { componentSlug: entry.slug, registryDemo: demo.id },
  width: entry.editorWidth,
  height: entry.editorHeight
}))]));
const catalogManifest = `// Generated from the official MIT-licensed Chakra UI ${version} compositions.\n` +
  `import type { UiComponentVariant } from './ui-library.js';\n` +
  `export const CHAKRA_OFFICIAL_COMPONENT_IDS = ${JSON.stringify(entries.map((entry) => entry.title), null, 2)} as const;\n` +
  `export const CHAKRA_OFFICIAL_COMPONENT_VARIANTS = ${JSON.stringify(variants, null, 2)} as const satisfies Record<string, readonly UiComponentVariant[]>;\n`;
await mkdir(path.dirname(runtimeManifestFile), { recursive: true });
await writeFile(runtimeManifestFile, runtimeManifest, 'utf8');
await writeFile(catalogManifestFile, catalogManifest, 'utf8');
console.log(`Synced ${entries.length} Chakra UI components and ${entries.flatMap((entry) => entry.demos).length} official examples from ${version}.`);
const missing = definitions.filter((definition) => !entries.some((entry) => entry.title === definition.id)).map((definition) => definition.id);
if (missing.length) console.log(`Components without a runnable official example: ${missing.join(', ')}`);
