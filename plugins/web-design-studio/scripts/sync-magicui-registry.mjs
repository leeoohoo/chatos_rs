import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fetchJson, fetchResponse, generatedModuleMap, officialCatalogVariants, registryCss, serializeModuleMap, stableRegistryVariantId, writeRegistryFiles } from './lib/registry-sync.mjs';

const projectRoot = path.resolve(import.meta.dirname, '..');
const outputRoot = path.join(projectRoot, 'ui-src/library-runtime/vendor/magicui');
const manifestFile = path.join(projectRoot, 'ui-src/library-runtime/magicui-registry.generated.ts');
const catalogManifestFile = path.join(projectRoot, 'src/magicui-registry.generated.ts');
const cssFile = path.join(projectRoot, 'ui-src/library-runtime/magicui-registry.generated.css');
const registryUrl = 'https://raw.githubusercontent.com/magicuidesign/magicui/main/registry.json';
const itemRoot = 'https://magicui.design/r';
const shadcnRoot = 'https://ui.shadcn.com/r/styles/new-york';
const githubFilesRoot = 'https://raw.githubusercontent.com/magicuidesign/magicui/main/apps/www';

const registry = await fetchJson(registryUrl);
const officialComponents = registry.items.filter((item) => item.type === 'registry:ui');
const officialExamples = registry.items.filter((item) => item.type === 'registry:example');
const officialNames = new Set(registry.items.map((item) => item.name));
const officialItems = new Map(registry.items.map((item) => [item.name, item]));
const fetched = new Map();
const files = new Map();
const missingFiles = new Set();
const dependencies = new Set();

function dependencyName(value) {
  return String(value).replace(/\.json$/, '').split('/').at(-1);
}

function itemUrl(reference) {
  if (/^https?:/.test(reference)) return `${reference.replace(/\.json$/, '')}.json`;
  const name = dependencyName(reference);
  return officialNames.has(name) ? `${itemRoot}/${name}.json` : `${shadcnRoot}/${name}.json`;
}

function storedPath(url, file) {
  if (url.startsWith(shadcnRoot)) return file.target || (file.path.startsWith('ui/') ? `components/${file.path}` : file.path);
  return file.path;
}

async function collect(reference) {
  const name = dependencyName(reference);
  const official = officialItems.get(name);
  const url = official ? `magicui:${name}` : itemUrl(reference);
  if (fetched.has(url)) return fetched.get(url);
  const pending = (async () => {
    let item = official;
    if (official) {
      const response = await fetchResponse(`${itemRoot}/${name}.json`);
      if (response.ok) item = await response.json();
    } else {
      item = await fetchJson(url);
    }
    for (const dependency of item.dependencies ?? []) dependencies.add(dependency);
    await Promise.all((item.files ?? []).map(async (file) => {
      let content = typeof file.content === 'string' ? file.content : undefined;
      if (!content && official) {
        const response = await fetchResponse(`${githubFilesRoot}/${file.path}`);
        if (response.ok) content = await response.text();
      }
      if (typeof content === 'string') {
        files.set(storedPath(url, file), content);
        if (url.startsWith(shadcnRoot)) {
          files.set(file.path, content);
          if (file.path.startsWith('ui/')) files.set(`registry/new-york/${file.path}`, content);
        }
      }
      else missingFiles.add(file.path);
    }));
    await Promise.all((item.registryDependencies ?? []).map(collect));
    return item;
  })();
  fetched.set(url, pending);
  return pending;
}

function demosFor(slug) {
  return officialExamples.filter((example) => (example.registryDependencies ?? []).some((dependency) => dependencyName(dependency) === slug));
}

function preferredDemo(slug, demos) {
  const exact = demos.find((demo) => demo.name === `${slug}-demo`);
  return exact ?? [...demos].sort((a, b) => a.name.length - b.name.length)[0];
}

function demoLabel(demo) {
  return demo.title || demo.name.split('-').map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`).join(' ');
}

function rootFile(item) {
  return item.files?.find((file) => /\.[jt]sx?$/.test(file.path))?.path;
}

function namedExport(slug, source = '') {
  if (slug === 'client-tweet-card') return 'ClientTweetCard';
  if (slug === 'glyph-matrix') return 'GlyphMatrix';
  return source.match(/export\s+(?:function|const)\s+([A-Z][A-Za-z0-9_]*)/)?.[1];
}

const entries = (await Promise.all(officialComponents.map(async (component) => {
  const item = await collect(component.name);
  const officialDemos = demosFor(component.name);
  const preferred = preferredDemo(component.name, officialDemos);
  const orderedDemos = preferred ? [preferred, ...officialDemos.filter((demo) => demo.name !== preferred.name)] : [];
  const resolvedDemos = (await Promise.all(orderedDemos.map(async (demo) => {
    const demoItem = await collect(demo.name);
    const demoPath = rootFile(demoItem);
    if (!demoPath || !files.has(demoPath)) return undefined;
    return {
      id: stableRegistryVariantId('magic', component.name, demo.name),
      label: demoLabel(demo),
      path: `./vendor/magicui/${demoPath}`,
      export: 'default'
    };
  }))).filter(Boolean);
  const sourcePath = rootFile(item);
  if (!sourcePath || !files.has(sourcePath)) return undefined;
  const sourceExport = namedExport(component.name, files.get(sourcePath));
  const usablePreviewPath = resolvedDemos[0]?.path.replace('./vendor/magicui/', '') ?? sourcePath;
  return {
    slug: component.name,
    title: component.title ?? component.name,
    description: component.description ?? '',
    rootPath: `./vendor/magicui/${sourcePath}`,
    rootExport: sourceExport,
    previewPath: `./vendor/magicui/${usablePreviewPath}`,
    previewExport: usablePreviewPath === sourcePath ? sourceExport : 'default',
    demos: resolvedDemos,
    acceptsChildren: usablePreviewPath === sourcePath ? component.name === 'animated-subscribe-button' : undefined,
    docsUrl: `https://magicui.design/docs/components/${component.name}`
  };
}))).filter(Boolean);

const resolvedItems = await Promise.all(fetched.values());
const registryUtils = await collect('utils');
const registryUtilsPath = rootFile(registryUtils);
if (!registryUtilsPath || !files.has(registryUtilsPath)) throw new Error('Magic UI official registry utils are unavailable.');
files.set('lib/utils.ts', files.get(registryUtilsPath));

const shadcnImports = new Set();
for (const source of files.values()) {
  for (const match of source.matchAll(/["']@\/components\/ui\/([^"']+)["']/g)) shadcnImports.add(match[1]);
}
await Promise.all([...shadcnImports].map(collect));
await Promise.all(fetched.values());

await writeRegistryFiles({ outputRoot, files, alias: '@magic' });
const modulePaths = entries.flatMap((entry) => [entry.rootPath, entry.previewPath, ...entry.demos.map((demo) => demo.path)]);
const modules = generatedModuleMap(modulePaths);
const generated = `// Generated by scripts/sync-magicui-registry.mjs from the official MIT-licensed Magic UI public registry.\n` +
  `import type { ReactRegistryEntry } from './types';\n` +
  `export const MAGICUI_REGISTRY_ENTRIES = ${JSON.stringify(entries, null, 2)} as const satisfies readonly ReactRegistryEntry[];\n` +
  `export const MAGICUI_REGISTRY_BY_SLUG = Object.fromEntries(MAGICUI_REGISTRY_ENTRIES.map((entry) => [entry.slug, entry])) as Record<string, ReactRegistryEntry>;\n` +
  `export const MAGICUI_REGISTRY_MODULES = ${serializeModuleMap(modules)};\n`;
await mkdir(path.dirname(manifestFile), { recursive: true });
await writeFile(manifestFile, generated, 'utf8');
const catalog = entries.map(({ slug, title, description }) => ({ slug, title, description }));
const catalogVariants = officialCatalogVariants(entries, 'magic');
const catalogGenerated = `// Generated by scripts/sync-magicui-registry.mjs from the official Magic UI registry.\n` +
  `import type { UiComponentVariant } from './ui-library.js';\n` +
  `export const MAGICUI_OFFICIAL_CATALOG = ${JSON.stringify(catalog, null, 2)} as const;\n` +
  `export const MAGICUI_OFFICIAL_COMPONENT_VARIANTS = ${JSON.stringify(catalogVariants, null, 2)} as const satisfies Record<string, readonly UiComponentVariant[]>;\n`;
await writeFile(catalogManifestFile, catalogGenerated, 'utf8');
await writeFile(cssFile, registryCss(resolvedItems), 'utf8');
console.log(`Synced ${entries.length} Magic UI components, ${entries.flatMap((entry) => entry.demos).length} official demos, and ${files.size} source files.`);
if (missingFiles.size) console.log(`Skipped unpublished registry files: ${[...missingFiles].sort().join(', ')}`);
console.log(`Runtime dependencies: ${[...dependencies].sort().join(', ')}`);
