import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fetchJson, fetchText } from './lib/registry-sync.mjs';

const projectRoot = path.resolve(import.meta.dirname, '..');
const catalogFile = path.join(projectRoot, 'src/inspira-library.ts');
const outputRoot = path.join(projectRoot, 'ui-src/library-runtime/vendor/inspira');
const manifestFile = path.join(projectRoot, 'ui-src/library-runtime/inspira-registry.generated.ts');
const catalogManifestFile = path.join(projectRoot, 'src/inspira-registry.generated.ts');
const docsRoot = 'https://raw.githubusercontent.com/rahulv-official/inspira-ui/main/content/en/2.components';
const localDocsRoot = process.env.INSPIRA_DOCS_ROOT;
const localSourceRoot = process.env.INSPIRA_SOURCE_ROOT;
const registryRoot = 'https://registry.inspira-ui.com';

async function readDocumentation(section, slug) {
  if (localDocsRoot) return readFile(path.join(localDocsRoot, section, `${slug}.md`), 'utf8');
  return fetchText(`${docsRoot}/${section}/${slug}.md`);
}

function catalogEntries(source) {
  const entries = [];
  for (const match of source.matchAll(/\{\s*category:\s*'([^']+)',\s*section:\s*'([^']+)',\s*slugs:\s*\[([^\]]+)\]\s*\}/g)) {
    const [, category, section, list] = match;
    for (const slug of [...list.matchAll(/'([^']+)'/g)].map((entry) => entry[1])) entries.push({ category, section, slug });
  }
  if (!entries.length) throw new Error('Unable to find the configured Inspira catalog sections.');
  return entries;
}

function registryIdFromDoc(markdown, slug) {
  const id = markdown.match(/componentId="([^"]+)"/)?.[1];
  if (!id) throw new Error(`Unable to find componentId in the ${slug} documentation.`);
  return id;
}

function demoFileFromDoc(markdown, slug) {
  const file = markdown.match(/demoFile="([^"]+)"/)?.[1];
  if (!file) throw new Error(`Unable to find demoFile in the ${slug} documentation.`);
  return file;
}

function componentId(slug) {
  return slug.split('-').map((part) => {
    if (/^3d$/i.test(part)) return 'ThreeD';
    if (/^[0-9]/.test(part)) return `N${part}`;
    return `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`;
  }).join('');
}

function demoId(slug, file, defaultDemo) {
  const name = file === defaultDemo ? 'basic' : path.posix.basename(file, '.vue')
    .replace(/Demo/gi, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/[^a-z0-9]+/gi, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();
  const strippedSuffix = name.startsWith(`${slug}-`) ? name.slice(slug.length + 1) : name;
  // The documented default demo owns the stable `basic` id. Some upstream
  // directories also contain a distinct `ComponentBasicDemo.vue`; retaining
  // the component prefix keeps that official file independently addressable
  // instead of silently routing both variants to the first matching demo.
  const suffix = file !== defaultDemo && strippedSuffix === 'basic' ? name : strippedSuffix;
  const id = `inspira-${slug}-${suffix || 'basic'}`;
  if (id.length <= 64) return id;
  const hash = createHash('sha256').update(id).digest('hex').slice(0, 8);
  return `${id.slice(0, 55).replace(/-+$/g, '')}-${hash}`;
}

function demoLabel(file, defaultDemo) {
  if (file === defaultDemo) return '官方基础示例';
  const name = path.posix.basename(file, '.vue')
    .replace(/Demo/gi, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/([A-Za-z])(\d+)/g, '$1 $2')
    .trim();
  return `官方 · ${name || path.posix.basename(file, '.vue')}`;
}

function rewriteInspiraSource(source) {
  return source
    .replaceAll('~/composables/useMouseState', './useMouseState')
    .replaceAll('~/components/content/inspira/ui/', '../../ui/')
    .replaceAll('~/components/inspira/ui/', '../../ui/')
    .replaceAll('/logo-dark.svg', '../../assets/logo-dark.svg');
}

async function copyOfficialTree(sourceDirectory, destinationDirectory) {
  await mkdir(destinationDirectory, { recursive: true });
  for (const item of await readdir(sourceDirectory, { withFileTypes: true })) {
    const source = path.join(sourceDirectory, item.name);
    const destination = path.join(destinationDirectory, item.name);
    if (item.isDirectory()) {
      await copyOfficialTree(source, destination);
      continue;
    }
    const content = await readFile(source);
    if (/\.(?:vue|ts|js|json|svg|css|html|txt|md)$/i.test(item.name)) {
      const rewritten = rewriteInspiraSource(content.toString('utf8'));
      await writeFile(destination, item.name.endsWith('.ts') ? `// @ts-nocheck\n${rewritten}` : rewritten, 'utf8');
    } else {
      await writeFile(destination, content);
    }
  }
}

function normalizedRegistryUrl(value) {
  if (value.startsWith('http')) return value.replace('/docs/r/', '/r/');
  return `${registryRoot}/${value.replace(/\.json$/, '')}.json`;
}

async function syncRegistryItem(url, collected) {
  if (collected.has(url)) return collected.get(url);
  const pending = (async () => {
    const payload = await fetchJson(url);
    await Promise.all((payload.registryDependencies ?? []).map((dependency) =>
      syncRegistryItem(normalizedRegistryUrl(dependency), collected)));
    return payload;
  })();
  collected.set(url, pending);
  return pending;
}

async function mapWithConcurrency(items, concurrency, mapper) {
  const results = new Array(items.length);
  let cursor = 0;
  async function worker() {
    while (cursor < items.length) {
      const index = cursor;
      cursor += 1;
      results[index] = await mapper(items[index], index);
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, worker));
  return results;
}

function safeVendorPath(relativePath) {
  const normalized = path.posix.normalize(relativePath).replace(/^\.\.\//, '');
  if (!normalized.startsWith('ui/') && !normalized.startsWith('lib/') && !normalized.startsWith('hooks/')) {
    throw new Error(`Unsupported Inspira registry path: ${relativePath}`);
  }
  return normalized;
}

const catalogSource = await readFile(catalogFile, 'utf8');
const catalog = catalogEntries(catalogSource);
const registryItems = new Map();
const repositoryRef = await fetchJson('https://api.github.com/repos/rahulv-official/inspira-ui/git/ref/heads/main');
const repositoryCommit = repositoryRef.object.sha;
const repositoryTree = await fetchJson(`https://api.github.com/repos/rahulv-official/inspira-ui/git/trees/${repositoryCommit}?recursive=1`);
if (repositoryTree.truncated) throw new Error('The Inspira UI source tree response was truncated.');
const exampleFilesByRegistryId = new Map();
for (const file of repositoryTree.tree) {
  const match = file.type === 'blob' && file.path.match(/^app\/components\/inspira\/examples\/([^/]+)\/(.+)$/);
  if (!match) continue;
  const [, registryId, relativePath] = match;
  const files = exampleFilesByRegistryId.get(registryId) ?? [];
  files.push(relativePath);
  exampleFilesByRegistryId.set(registryId, files);
}

const entries = await mapWithConcurrency(catalog, 6, async ({ category, section, slug }, index) => {
  const markdown = await readDocumentation(section, slug);
  const registryId = registryIdFromDoc(markdown, slug);
  const demoFile = demoFileFromDoc(markdown, slug);
  const registryUrl = `${registryRoot}/${registryId}.json`;
  const payload = await syncRegistryItem(registryUrl, registryItems);
  const rootFile = payload.files?.find((file) => file.path.endsWith('.vue'));
  if (!rootFile) throw new Error(`${registryId} does not publish a Vue component file.`);
  const availableExampleFiles = exampleFilesByRegistryId.get(registryId) ?? [];
  if (!availableExampleFiles.includes(demoFile)) throw new Error(`${registryId} does not contain its documented demo ${demoFile}.`);
  const demoFiles = [demoFile, ...availableExampleFiles
    .filter((file) => file !== demoFile && /Demo[^/]*\.vue$/i.test(file))
    .sort((a, b) => a.localeCompare(b))];
  const demos = demoFiles.map((file) => ({
    id: demoId(slug, file, demoFile),
    label: demoLabel(file, demoFile),
    path: `./vendor/inspira/examples/${registryId}/${file}`
  }));
  if (new Set(demos.map((demo) => demo.id)).size !== demos.length) {
    throw new Error(`${registryId} generated duplicate official demo ids.`);
  }
  if ((index + 1) % 10 === 0 || index + 1 === catalog.length) console.log(`Resolved ${index + 1}/${catalog.length} Inspira catalog entries.`);
  return {
    slug,
    category,
    section,
    registryId,
    rootPath: `./vendor/inspira/${safeVendorPath(rootFile.path)}`,
    previewPath: demos[0].path,
    demos,
    componentPaths: (payload.files ?? []).filter((file) => file.path.endsWith('.vue')).map((file) => `./vendor/inspira/${safeVendorPath(file.path)}`),
    registryDependencies: payload.registryDependencies ?? [],
    docsUrl: `https://inspira-ui.com/docs/en/components/${section}/${slug}`
  };
});

const resolvedRegistryItems = new Map(await Promise.all([...registryItems].map(async ([url, pending]) => [url, await pending])));

await rm(outputRoot, { recursive: true, force: true });
await mkdir(outputRoot, { recursive: true });

for (const payload of resolvedRegistryItems.values()) {
  for (const file of payload.files ?? []) {
    if (typeof file.content !== 'string') continue;
    const relativePath = safeVendorPath(file.path);
    const destination = path.join(outputRoot, relativePath);
    await mkdir(path.dirname(destination), { recursive: true });
    const rewritten = rewriteInspiraSource(file.content);
    const checked = relativePath.endsWith('.ts') ? `// @ts-nocheck\n${rewritten}` : rewritten;
    await writeFile(destination, checked, 'utf8');
  }
}

await mapWithConcurrency(entries, 6, async (entry) => {
  const relativeDemoDirectory = `app/components/inspira/examples/${entry.registryId}`;
  const destinationDirectory = path.join(outputRoot, 'examples', entry.registryId);
  if (localSourceRoot) {
    await copyOfficialTree(path.join(localSourceRoot, relativeDemoDirectory), destinationDirectory);
  } else {
    await mapWithConcurrency(exampleFilesByRegistryId.get(entry.registryId) ?? [], 4, async (relativePath) => {
      const destination = path.join(destinationDirectory, relativePath);
      await mkdir(path.dirname(destination), { recursive: true });
      const source = await fetchText(`https://raw.githubusercontent.com/rahulv-official/inspira-ui/${repositoryCommit}/${relativeDemoDirectory}/${relativePath}`);
      const rewritten = rewriteInspiraSource(source);
      await writeFile(destination, relativePath.endsWith('.ts') ? `// @ts-nocheck\n${rewritten}` : rewritten, 'utf8');
    });
  }
});

// The official github-globe registry item imports globe.json but deliberately
// omits it and instructs consumers to download the asset beside the component.
// Keep the runtime reproducible by sourcing that missing file from the same
// MIT-licensed upstream repository instead of inventing replacement geometry.
const githubGlobeData = localSourceRoot
  ? await readFile(path.join(localSourceRoot, 'app/components/inspira/ui/github-globe/globe.json'), 'utf8')
  : await fetchText(`https://raw.githubusercontent.com/rahulv-official/inspira-ui/${repositoryCommit}/app/components/inspira/ui/github-globe/globe.json`);
const githubGlobeDataPath = path.join(outputRoot, 'ui/github-globe/globe.json');
await mkdir(path.dirname(githubGlobeDataPath), { recursive: true });
await writeFile(githubGlobeDataPath, githubGlobeData, 'utf8');

const officialLogo = localSourceRoot
  ? await readFile(path.join(localSourceRoot, 'public/logo-dark.svg'), 'utf8')
  : await fetchText(`https://raw.githubusercontent.com/rahulv-official/inspira-ui/${repositoryCommit}/public/logo-dark.svg`);
const officialLogoPath = path.join(outputRoot, 'assets/logo-dark.svg');
await mkdir(path.dirname(officialLogoPath), { recursive: true });
await writeFile(officialLogoPath, officialLogo, 'utf8');

const dependencyRoots = new Map(
  [...resolvedRegistryItems.entries()].map(([url, payload]) => [url, (payload.files ?? []).filter((file) => file.path.endsWith('.vue')).map((file) => `./vendor/inspira/${safeVendorPath(file.path)}`)])
);
for (const entry of entries) {
  for (const dependency of entry.registryDependencies) {
    entry.componentPaths.push(...(dependencyRoots.get(normalizedRegistryUrl(dependency)) ?? []));
  }
  entry.componentPaths = [...new Set(entry.componentPaths)];
  delete entry.registryDependencies;
}

const generated = `// Generated by scripts/sync-inspira-registry.mjs from the official MIT-licensed Inspira UI registry.\n` +
  `export interface InspiraRegistryDemo { id: string; label: string; path: string; }\n` +
  `export interface InspiraRegistryEntry { slug: string; category: string; section: string; registryId: string; rootPath: string; previewPath: string; demos: InspiraRegistryDemo[]; componentPaths: string[]; docsUrl: string; }\n` +
  `export const INSPIRA_REGISTRY_ENTRIES = ${JSON.stringify(entries, null, 2)} as const satisfies readonly InspiraRegistryEntry[];\n` +
  `export const INSPIRA_REGISTRY_BY_SLUG = Object.fromEntries(INSPIRA_REGISTRY_ENTRIES.map((entry) => [entry.slug, entry])) as Record<string, InspiraRegistryEntry>;\n`;

const officialVariants = Object.fromEntries(entries.map((entry) => [componentId(entry.slug), entry.demos.map((demo) => ({
  id: demo.id,
  label: demo.label,
  props: { componentSlug: entry.slug, registryDemo: demo.id }
}))]));
const catalogGenerated = `// Generated by scripts/sync-inspira-registry.mjs from official Inspira UI examples.\n` +
  `import type { UiComponentVariant } from './ui-library.js';\n` +
  `export const INSPIRA_OFFICIAL_COMPONENT_VARIANTS = ${JSON.stringify(officialVariants, null, 2)} as const satisfies Record<string, readonly UiComponentVariant[]>;\n`;

await mkdir(path.dirname(manifestFile), { recursive: true });
await writeFile(manifestFile, generated, 'utf8');
await writeFile(catalogManifestFile, catalogGenerated, 'utf8');
console.log(`Synced ${entries.length} Inspira components and ${entries.flatMap((entry) => entry.demos).length} official examples from ${repositoryCommit}.`);
console.log(`Runtime dependencies: ${[...new Set([...resolvedRegistryItems.values()].flatMap((payload) => payload.dependencies ?? []))].sort().join(', ')}`);
