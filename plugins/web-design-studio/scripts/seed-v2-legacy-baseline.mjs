import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { V2_WEBSITE_BENCHMARKS } from '../dist/v2-phase0-baseline.test.mjs';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

async function readJson(url, options) {
  const response = await fetch(url, options);
  if (!response.ok) throw new Error(`${options?.method ?? 'GET'} ${url} failed with ${response.status}: ${await response.text()}`);
  return response.json();
}

const baseUrl = new URL(argument('--base-url') ?? 'http://127.0.0.1:4288/');
if (baseUrl.protocol !== 'http:' && baseUrl.protocol !== 'https:') throw new Error('--base-url must use HTTP or HTTPS.');
const outputPath = path.resolve(argument('--output') ?? '.web-design-studio-baselines/legacy-current-run.json');
const previewBaseUrl = new URL(argument('--preview-base-url') ?? 'http://127.0.0.1:4289/');
if (previewBaseUrl.protocol !== 'http:' && previewBaseUrl.protocol !== 'https:') throw new Error('--preview-base-url must use HTTP or HTTPS.');
const runId = argument('--run-id') ?? 'v3.0.1-legacy-current';
if (!/^[a-z0-9][a-z0-9._-]{0,119}$/i.test(runId)) throw new Error('--run-id must be a safe file name segment.');

const context = await readJson(new URL('/api/context', baseUrl));
if (!context.defaultProjectId) throw new Error('The baseline server did not expose a default project id.');
const existing = await readJson(new URL('/api/documents', baseUrl));
const existingByTitle = new Map(existing.items.map((document) => [document.title, document]));
const designs = [];

for (const benchmark of V2_WEBSITE_BENCHMARKS) {
  const title = `基线 · ${benchmark.name}`;
  let document = existingByTitle.get(title);
  if (!document) {
    document = await readJson(new URL('/api/documents', baseUrl), {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ title, blank: false })
    });
  }
  const fullDocument = await readJson(new URL(`/api/documents/${encodeURIComponent(document.documentId)}`, baseUrl));
  const componentTypes = Object.fromEntries([...new Set(fullDocument.components.map((component) => component.type))]
    .sort()
    .map((type) => [type, fullDocument.components.filter((component) => component.type === type).length]));
  const studioUrl = new URL('/', baseUrl);
  studioUrl.searchParams.set('studio-project', context.defaultProjectId);
  studioUrl.searchParams.set('studio-design', document.documentId);
  const previewUrl = new URL(`/benchmark/${benchmark.id}`, previewBaseUrl);
  designs.push({
    benchmarkId: benchmark.id,
    url: previewUrl.href,
    captureMode: 'html',
    studioUrl: studioUrl.href,
    projectId: context.defaultProjectId,
    documentId: document.documentId,
    structure: {
      pageCount: fullDocument.pages?.length ?? 1,
      componentCount: fullDocument.components.length,
      parentedComponentCount: fullDocument.components.filter((component) => component.parentId).length,
      componentTypes,
      breakpoints: fullDocument.breakpoints
        ? Object.fromEntries(Object.entries(fullDocument.breakpoints).map(([device, value]) => [device, value.width]))
        : undefined
    }
  });
  process.stdout.write(`${benchmark.id} ${document.documentId}\n`);
}

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify({
  runId,
  sourceVersion: '3.0.1-phase0-legacy-generator',
  createdAt: new Date().toISOString(),
  generator: {
    id: 'legacy-built-in-landing-template',
    description: 'The deterministic landing-page fallback available before Schema v2 and the AI design protocol.'
  },
  designs
}, null, 2)}\n`);
process.stdout.write(`manifest ${outputPath}\n`);
