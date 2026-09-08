import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { createPhase2LayoutBenchmarks } from '../dist/v2-phase2-layout-benchmarks.test.mjs';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const origin = argument('--origin') ?? 'http://127.0.0.1:4290';
const output = path.resolve(argument('--output') ?? '.web-design-studio-baselines/phase2-current-run.json');
const runId = argument('--run-id') ?? 'v3.0.1-phase2-layout';
const createdAt = new Date().toISOString();
const designs = createPhase2LayoutBenchmarks().map((benchmark) => ({
  benchmarkId: benchmark.benchmarkId,
  url: `${origin}/benchmark/${benchmark.benchmarkId}`,
  captureMode: 'html',
  viewportQuery: 'width',
  projectId: 'project-v3.0.1-phase2',
  documentId: benchmark.document.documentId,
  structure: {
    pattern: benchmark.pattern,
    signature: benchmark.structuralSignature
  }
}));
const definition = { runId, sourceVersion: '3.0.1-phase2', createdAt, designs };
await mkdir(path.dirname(output), { recursive: true });
await writeFile(output, `${JSON.stringify(definition, null, 2)}\n`);
process.stdout.write(`${output}\n`);
