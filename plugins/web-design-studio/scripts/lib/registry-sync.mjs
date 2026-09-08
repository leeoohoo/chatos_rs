import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

export async function fetchResponse(url, attempts = 4, timeoutMs = 15000) {
  let lastError;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const response = await fetch(url, {
        headers: { 'user-agent': 'ChatOS-Web-Design-Studio' },
        signal: AbortSignal.timeout(timeoutMs)
      });
      if (response.ok || response.status === 404) return response;
      lastError = new Error(`Unable to fetch ${url}: ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250 * (attempt + 1)));
  }
  throw lastError;
}

export async function fetchText(url) {
  const response = await fetchResponse(url);
  if (!response.ok) throw new Error(`Unable to fetch ${url}: ${response.status}`);
  return response.text();
}

export async function fetchJson(url) {
  return JSON.parse(await fetchText(url));
}

export function safeRegistryPath(value) {
  const normalized = path.posix.normalize(value).replace(/^(\.\.\/)+/, '');
  if (normalized.startsWith('/') || normalized.includes('/../')) throw new Error(`Unsafe registry path: ${value}`);
  return normalized;
}

export function rewriteScopedImports(source, alias) {
  return source.replace(/(["'])@\//g, `$1${alias}/`);
}

export async function writeRegistryFiles({ outputRoot, files, alias }) {
  await rm(outputRoot, { recursive: true, force: true });
  await mkdir(outputRoot, { recursive: true });
  for (const [relativePath, source] of files) {
    const destination = path.join(outputRoot, safeRegistryPath(relativePath));
    await mkdir(path.dirname(destination), { recursive: true });
    const rewritten = rewriteScopedImports(source, alias);
    const checked = /\.[jt]sx?$/.test(relativePath) ? `// @ts-nocheck\n${rewritten}` : rewritten;
    await writeFile(destination, checked, 'utf8');
  }
}

export function generatedModuleMap(paths) {
  return Object.fromEntries([...new Set(paths)].sort().map((modulePath) => [modulePath, `() => import(${JSON.stringify(modulePath)})`]));
}

export function serializeModuleMap(map) {
  return `{\n${Object.entries(map).map(([modulePath, loader]) => `  ${JSON.stringify(modulePath)}: ${loader}`).join(',\n')}\n}`;
}

export function registryComponentId(slug) {
  return String(slug).split('-').map((part) => {
    if (/^3d$/i.test(part)) return 'ThreeD';
    if (/^[0-9]/.test(part)) return `N${part}`;
    return `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`;
  }).join('');
}

export function stableRegistryVariantId(library, slug, sourceName) {
  const suffix = String(sourceName)
    .replace(/\.[^.]+$/, '')
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/[^a-z0-9]+/gi, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase() || 'official';
  const raw = `${library}-${slug}-${suffix}`;
  if (raw.length <= 64) return raw;
  const hash = createHash('sha256').update(raw).digest('hex').slice(0, 8);
  return `${raw.slice(0, 55).replace(/-+$/g, '')}-${hash}`;
}

export function officialCatalogVariants(entries, library) {
  return Object.fromEntries(entries.map((entry) => {
    const demos = entry.demos?.length ? entry.demos.map((demo) => ({
      id: demo.id,
      label: demo.label,
      props: { componentSlug: entry.slug, registryDemo: demo.id }
    })) : [{
      id: stableRegistryVariantId(library, entry.slug, 'official-source'),
      label: '官方组件示例',
      props: { componentSlug: entry.slug }
    }];
    return [registryComponentId(entry.slug), demos];
  }));
}

function kebab(value) {
  return value.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
}

function declarationBlock(declarations, indent = '  ') {
  return Object.entries(declarations ?? {}).map(([property, value]) => `${indent}${kebab(property)}:${value};`).join('\n');
}

export function registryCss(items) {
  const theme = new Map();
  const light = new Map();
  const dark = new Map();
  const rules = [];
  for (const item of items) {
    for (const [name, value] of Object.entries(item.cssVars?.theme ?? {})) theme.set(name, value);
    for (const [name, value] of Object.entries(item.cssVars?.light ?? {})) light.set(name, value);
    for (const [name, value] of Object.entries(item.cssVars?.dark ?? {})) dark.set(name, value);
    for (const [selector, frames] of Object.entries(item.css ?? {})) {
      if (selector.startsWith('@keyframes ')) {
        const steps = Object.entries(frames).map(([step, declarations]) => `  ${step}{\n${declarationBlock(declarations, '    ')}\n  }`).join('\n');
        rules.push(`${selector}{\n${steps}\n}`);
      } else {
        rules.push(`${selector}{\n${declarationBlock(frames)}\n}`);
      }
    }
  }
  const sections = [];
  if (theme.size) sections.push(`@theme inline {\n${[...theme].map(([name, value]) => `  --${name}:${value};`).join('\n')}\n}`);
  if (light.size) sections.push(`:root{\n${[...light].map(([name, value]) => `  --${name}:${value};`).join('\n')}\n}`);
  if (dark.size) sections.push(`.dark{\n${[...dark].map(([name, value]) => `  --${name}:${value};`).join('\n')}\n}`);
  sections.push(...new Set(rules));
  return `${sections.join('\n\n')}\n`;
}
