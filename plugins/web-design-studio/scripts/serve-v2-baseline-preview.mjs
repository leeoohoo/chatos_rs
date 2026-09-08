import http from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { WebDesignDocumentStore } from '../dist/document-store.test.mjs';
import { exportPageHtml } from '../dist/html-exporter.test.mjs';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const manifestPath = path.resolve(argument('--manifest') ?? '.web-design-studio-baselines/legacy-current-run.json');
const dataDirectory = path.resolve(argument('--data-dir') ?? '.web-design-studio-baselines/legacy-data');
const port = Number(argument('--port') ?? 4289);
const host = argument('--host') ?? '127.0.0.1';
if (!Number.isSafeInteger(port) || port < 1024 || port > 65535) throw new Error('--port must be between 1024 and 65535.');

const definition = JSON.parse(await readFile(manifestPath, 'utf8'));
const designs = new Map(definition.designs.map((design) => [design.benchmarkId, design]));
const store = new WebDesignDocumentStore(dataDirectory);
await store.initialize();

const server = http.createServer(async (request, response) => {
  try {
    const requestUrl = new URL(request.url ?? '/', `http://${request.headers.host ?? `${host}:${port}`}`);
    response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    if (requestUrl.pathname === '/health') {
      response.setHeader('Content-Type', 'application/json; charset=utf-8');
      response.end(JSON.stringify({ ok: true, runId: definition.runId, designCount: designs.size }));
      return;
    }
    const match = requestUrl.pathname.match(/^\/benchmark\/([a-z0-9._-]+)$/i);
    if (!match) {
      response.statusCode = 404;
      response.end('Not found');
      return;
    }
    const design = designs.get(match[1]);
    if (!design) {
      response.statusCode = 404;
      response.end('Unknown benchmark');
      return;
    }
    const document = await store.read(design.documentId);
    const pageId = document.pages?.[0]?.id ?? 'home';
    const html = exportPageHtml(document, pageId, 'desktop');
    response.setHeader('Content-Type', 'text/html; charset=utf-8');
    response.end(html);
  } catch (error) {
    response.statusCode = 500;
    response.setHeader('Content-Type', 'text/plain; charset=utf-8');
    response.end(error instanceof Error ? error.message : String(error));
  }
});

server.listen(port, host, () => {
  process.stdout.write(`Web Design Studio baseline preview is available at http://${host}:${port}\n`);
});
