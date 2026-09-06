import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fetchJson, generatedModuleMap, registryCss, serializeModuleMap, writeRegistryFiles } from './lib/registry-sync.mjs';

const projectRoot = path.resolve(import.meta.dirname, '..');
const outputRoot = path.join(projectRoot, 'ui-src/library-runtime/vendor/shadcn');
const runtimeManifestFile = path.join(projectRoot, 'ui-src/library-runtime/shadcn-registry.generated.ts');
const catalogManifestFile = path.join(projectRoot, 'src/shadcn-registry.generated.ts');
const cssFile = path.join(projectRoot, 'ui-src/library-runtime/shadcn-registry.generated.css');
const compositionsFile = path.join(projectRoot, 'ui-src/library-runtime/shadcn-compositions.json');
const registryRoot = 'https://ui.shadcn.com/r/styles/new-york-v4';
const registry = await fetchJson(`${registryRoot}/registry.json`);
const compositionCatalog = JSON.parse(await readFile(compositionsFile, 'utf8'));
const catalogItems = new Map(registry.items.map((item) => [item.name, item]));
const uiItems = registry.items.filter((item) => item.type === 'registry:ui');
const uiSlugs = uiItems.map((item) => item.name).sort((a, b) => b.length - a.length);
const files = new Map();
const fetched = new Map();
const dependencies = new Set();

function dependencyName(value) {
  return String(value).replace(/\.json$/, '').split('/').at(-1);
}

function itemUrl(reference) {
  if (/^https?:/.test(reference)) return `${reference.replace(/\.json$/, '')}.json`;
  return `${registryRoot}/${dependencyName(reference)}.json`;
}

async function collect(reference) {
  const name = dependencyName(reference);
  if (fetched.has(name)) return fetched.get(name);
  const pending = (async () => {
    const item = await fetchJson(itemUrl(reference));
    for (const dependency of item.dependencies ?? []) {
      if (dependency !== 'cn') dependencies.add(dependency);
    }
    for (const file of item.files ?? []) {
      if (typeof file.content === 'string') files.set(file.path, file.content);
    }
    await Promise.all((item.registryDependencies ?? []).map(collect));
    return item;
  })();
  fetched.set(name, pending);
  return pending;
}

function componentId(slug) {
  return slug.split('-').map((part) => part.toUpperCase() === 'OTP'
    ? 'OTP'
    : `${part[0].toUpperCase()}${part.slice(1)}`).join('');
}

function primaryComponentFor(item) {
  return uiSlugs.find((slug) => item.name === `${slug}-demo` || item.name.startsWith(`${slug}-`));
}

function sourceFile(item, role) {
  const files = item.files ?? [];
  if (role === 'preview') {
    return files.find((file) => file.type === 'registry:page')
      ?? files.find((file) => file.type === 'registry:block' || file.type === 'registry:example')
      ?? files.find((file) => /\.[jt]sx?$/.test(file.path));
  }
  return files.find((file) => file.type === 'registry:ui' && /\.[jt]sx?$/.test(file.path))
    ?? files.find((file) => /\.[jt]sx?$/.test(file.path));
}

function exportedComponent(source, preferred) {
  if (source.match(new RegExp(`export\\s+(?:function|const)\\s+${preferred}\\b`))) return preferred;
  if (source.match(new RegExp(`export\\s*\\{[^}]*\\b${preferred}\\b`, 's'))) return preferred;
  if (/export\s+default\s+/.test(source)) return 'default';
  return source.match(/export\s+(?:function|const)\s+([A-Z][A-Za-z0-9_]*)/)?.[1]
    ?? source.match(/export\s*\{\s*([A-Z][A-Za-z0-9_]*)/)?.[1];
}

function hasMissingRelativeImports(sourcePath, source) {
  for (const match of source.matchAll(/(?:from\s+|import\s*\()(["'])(\.[^"']+)\1/g)) {
    const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(sourcePath), match[2]));
    const candidates = [resolved, `${resolved}.ts`, `${resolved}.tsx`, `${resolved}.js`, `${resolved}.jsx`, `${resolved}/index.ts`, `${resolved}/index.tsx`];
    if (!candidates.some((candidate) => files.has(candidate))) return true;
  }
  for (const match of source.matchAll(/(?:from\s+|import\s*\()(["'])@\/(.+?)\1/g)) {
    const resolved = match[2];
    const candidates = [resolved, `${resolved}.ts`, `${resolved}.tsx`, `${resolved}.js`, `${resolved}.jsx`, `${resolved}/index.ts`, `${resolved}/index.tsx`];
    if (!candidates.some((candidate) => files.has(candidate))) return true;
  }
  return false;
}

function variantLabel(slug, name) {
  if (name === `${slug}-demo`) return '官方默认示例';
  const suffix = name.slice(slug.length + 1);
  return suffix.split('-').map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`).join(' ');
}

const previewCatalog = registry.items
  .filter((item) => item.type === 'registry:example' || item.type === 'registry:block')
  .map((item) => ({ item, component: primaryComponentFor(item) }))
  .filter((entry) => entry.component);

await Promise.all([
  ...uiItems.map((item) => collect(item.name)),
  ...previewCatalog.map((entry) => collect(entry.item.name))
]);
await Promise.all(fetched.values());

const entries = await Promise.all(uiItems.map(async (catalogItem) => {
  const item = await collect(catalogItem.name);
  const root = sourceFile(item, 'root');
  if (!root || !files.has(root.path)) throw new Error(`shadcn/ui ${catalogItem.name} does not publish a runtime component.`);
  const rootExport = exportedComponent(files.get(root.path), componentId(catalogItem.name));
  if (!rootExport) throw new Error(`Unable to identify the shadcn/ui ${catalogItem.name} export.`);

  const previews = (await Promise.all(previewCatalog.filter((entry) => entry.component === catalogItem.name).map(async (candidate) => {
    const previewItem = await collect(candidate.item.name);
    const preview = sourceFile(previewItem, 'preview');
    if (!preview || !files.has(preview.path)) return undefined;
    if (hasMissingRelativeImports(preview.path, files.get(preview.path))) return undefined;
    const previewExport = exportedComponent(files.get(preview.path), componentId(candidate.item.name));
    if (!previewExport) return undefined;
    return {
      id: candidate.item.name,
      label: variantLabel(catalogItem.name, candidate.item.name),
      path: `./vendor/shadcn/${preview.path}`,
      export: previewExport,
      type: candidate.item.type,
    };
  }))).filter(Boolean);
  previews.sort((a, b) => (a.id === `${catalogItem.name}-demo` ? -1 : b.id === `${catalogItem.name}-demo` ? 1 : a.id.localeCompare(b.id)));
  const initialPreview = previews[0];
  return {
    slug: catalogItem.name,
    title: catalogItem.title ?? catalogItem.name,
    description: catalogItem.description ?? '',
    rootPath: `./vendor/shadcn/${root.path}`,
    rootExport,
    previewPath: initialPreview?.path ?? `./vendor/shadcn/${root.path}`,
    previewExport: initialPreview?.export ?? rootExport,
    demo: initialPreview?.id,
    demos: previews,
    acceptsChildren: previews.length ? false : true,
    docsUrl: `https://ui.shadcn.com/docs/components/${catalogItem.name}`,
  };
}));

const presentationBySlug = {
  chart: { layout: 'fill', previewHeight: 370, previewSpan: 'wide' },
  sidebar: { layout: 'fill', previewHeight: 540, previewSpan: 'wide' },
  resizable: { layout: 'fill', previewHeight: 300, previewSpan: 'wide' },
  'scroll-area': { layout: 'fill', previewHeight: 280 },
  carousel: { layout: 'fill', previewHeight: 330, previewSpan: 'wide' },
  form: { previewHeight: 460, previewSpan: 'wide' },
  command: { previewHeight: 350, previewSpan: 'wide' },
  table: { previewHeight: 310, previewSpan: 'wide' }
};

const entryBySlug = new Map(entries.map((entry) => [entry.slug, entry]));

function resolvedCompositionNode(node) {
  if (Array.isArray(node)) return node.map(resolvedCompositionNode);
  if (!node || typeof node !== 'object') return node;
  const resolved = Object.fromEntries(Object.entries(node).map(([key, value]) => [key, resolvedCompositionNode(value)]));
  if (typeof node.module === 'string') {
    const separator = node.module.lastIndexOf('.');
    const slug = node.module.slice(0, separator);
    const exportName = node.module.slice(separator + 1);
    const referencedEntry = entryBySlug.get(slug);
    if (!referencedEntry || !exportName) throw new Error(`Unknown shadcn composition export: ${node.module}`);
    resolved.module = referencedEntry.rootPath;
    resolved.export = exportName;
  }
  return resolved;
}

for (const entry of entries) {
  Object.assign(entry, presentationBySlug[entry.slug] ?? {});
  const composition = compositionCatalog[entry.slug];
  if (!composition) continue;
  const demos = composition.demos.map((demo) => ({
    id: demo.id,
    label: demo.label,
    type: 'registry:composition',
    composition: resolvedCompositionNode(demo.composition)
  }));
  entry.demo = demos[0]?.id;
  entry.demos = demos;
  entry.acceptsChildren = false;
  Object.assign(entry, {
    layout: composition.layout ?? entry.layout,
    previewHeight: composition.previewHeight ?? entry.previewHeight,
    previewSpan: composition.previewSpan ?? entry.previewSpan
  });
}

files.set('cn.ts', `import { clsx, type ClassValue } from "clsx";\nimport { twMerge } from "tailwind-merge";\nexport function cn(...inputs: ClassValue[]) { return twMerge(clsx(inputs)); }\n`);
await Promise.all(fetched.values());
await writeRegistryFiles({ outputRoot, files, alias: '@shadcn-registry' });

const modulePaths = entries.flatMap((entry) => [entry.rootPath, entry.previewPath, ...entry.demos.map((demo) => demo.path).filter(Boolean)]);
const runtimeManifest = `// Generated by scripts/sync-shadcn-registry.mjs from the official MIT-licensed shadcn/ui registry.\n` +
  `import type { ReactRegistryEntry } from './types';\n` +
  `export const SHADCN_REGISTRY_ENTRIES = ${JSON.stringify(entries, null, 2)} as const satisfies readonly ReactRegistryEntry[];\n` +
  `export const SHADCN_REGISTRY_BY_SLUG = Object.fromEntries(SHADCN_REGISTRY_ENTRIES.map((entry) => [entry.slug, entry])) as Record<string, ReactRegistryEntry>;\n` +
  `export const SHADCN_REGISTRY_MODULES = ${serializeModuleMap(generatedModuleMap(modulePaths))};\n`;

const officialIds = entries.map((entry) => componentId(entry.slug));
const variants = Object.fromEntries(entries.map((entry) => [componentId(entry.slug), entry.demos.length
  ? entry.demos.map((demo) => ({ id: demo.id, label: demo.label, props: { registryDemo: demo.id } }))
  : [{ id: 'official-default', label: '官方组件', props: {} }]]));
const catalogManifest = `// Generated by scripts/sync-shadcn-registry.mjs from the official shadcn/ui registry.\n` +
  `import type { UiComponentVariant } from './ui-library.js';\n` +
  `export const SHADCN_OFFICIAL_COMPONENT_IDS = ${JSON.stringify(officialIds, null, 2)} as const;\n` +
  `export const SHADCN_OFFICIAL_COMPONENT_VARIANTS = ${JSON.stringify(variants, null, 2)} as const satisfies Record<string, readonly UiComponentVariant[]>;\n`;

await mkdir(path.dirname(runtimeManifestFile), { recursive: true });
await writeFile(runtimeManifestFile, runtimeManifest, 'utf8');
await writeFile(catalogManifestFile, catalogManifest, 'utf8');
await writeFile(cssFile, registryCss(await Promise.all(fetched.values())), 'utf8');
const officialPreviewCount = entries.flatMap((entry) => entry.demos).filter((demo) => demo.type !== 'registry:composition').length;
const compositionCount = entries.flatMap((entry) => entry.demos).filter((demo) => demo.type === 'registry:composition').length;
console.log(`Synced ${entries.length} shadcn/ui components, ${officialPreviewCount} official examples/blocks, ${compositionCount} official primitive compositions, and ${files.size} source files.`);
console.log(`Runtime dependencies: ${[...dependencies].sort().join(', ')}`);
