import { createServer, type IncomingMessage } from 'node:http';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { z } from 'zod';
import { PlanningStore, DomainError } from './store.js';

async function body(request: IncomingMessage): Promise<unknown> {
  if (request.headers['content-type']?.split(';')[0] !== 'application/json') throw new DomainError('invalid_request', 'JSON is required');
  const chunks: Buffer[] = []; let length = 0;
  for await (const chunk of request) {
    length += chunk.length;
    if (length > 1024 * 1024) throw new DomainError('too_large', 'Request exceeds 1 MiB');
    chunks.push(chunk);
  }
  try { return JSON.parse(Buffer.concat(chunks).toString('utf8')); }
  catch { throw new DomainError('invalid_request', 'Invalid JSON'); }
}

export async function startPlanningServer(store: PlanningStore, port: number) {
  const token = randomBytes(32).toString('hex');
  const assets = new Map([
    ['/', { type: 'text/html; charset=utf-8', value: readFileSync(fileURLToPath(new URL('../ui/index.html', import.meta.url)), 'utf8').replace('__SESSION_TOKEN__', token) }],
    ['/app.js', { type: 'text/javascript; charset=utf-8', value: readFileSync(fileURLToPath(new URL('../ui/app.js', import.meta.url)), 'utf8') }],
    ['/style.css', { type: 'text/css; charset=utf-8', value: readFileSync(fileURLToPath(new URL('../ui/style.css', import.meta.url)), 'utf8') }]
  ]);
  let origin = '';
  const server = createServer(async (request, response) => {
    const json = (code: number, value: unknown) => { response.writeHead(code, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(value)); };
    response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('Content-Security-Policy', "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'");
    try {
      if (`http://${request.headers.host}` !== origin || (request.headers.origin && request.headers.origin !== origin)) { json(403, { error: 'Untrusted origin' }); return; }
      const target = new URL(request.url ?? '/', origin);
      if (request.method === 'GET' && target.pathname === '/api/health') { json(200, { ok: true }); return; }
      const asset = assets.get(target.pathname);
      if (asset && request.method === 'GET') { response.writeHead(200, { 'Content-Type': asset.type }); response.end(asset.value); return; }
      const supplied = request.headers['x-planning-session'];
      if (typeof supplied !== 'string' || Buffer.byteLength(supplied) !== token.length || !timingSafeEqual(Buffer.from(supplied), Buffer.from(token))) { json(403, { error: 'Invalid application session' }); return; }
      if (request.method === 'GET' && target.pathname === '/api/state') {
        json(200, { context: { projectId: store.context.projectId, projectName: store.context.projectName }, ...store.read() }); return;
      }
      if (request.method === 'GET' && target.pathname.startsWith('/api/documents/')) {
        const id = decodeURIComponent(target.pathname.slice('/api/documents/'.length));
        const document = store.getDocument(id);
        if (!document) { json(404, { error: 'Document not found' }); return; }
        json(200, document); return;
      }
      if (request.method === 'GET' && target.pathname === '/api/scope') {
        const kind = target.searchParams.get('kind');
        const id = target.searchParams.get('id') ?? '';
        if (kind !== 'requirement' && kind !== 'work_item') { json(400, { error: 'Invalid scope kind' }); return; }
        json(200, store.scope(kind, id)); return;
      }
      if (request.method === 'POST' && target.pathname === '/api/changes') { json(200, store.mutate(await body(request))); return; }
      json(404, { error: 'Not found' });
    } catch (error) {
      if (error instanceof DomainError) json(error.code === 'revision_conflict' ? 409 : 400, { error: error.message, code: error.code });
      else if (error instanceof z.ZodError) json(400, { error: 'Unknown or invalid planning fields' });
      else json(500, { error: 'Local planning data could not be read or saved' });
    }
  });
  server.requestTimeout = 10_000; server.headersTimeout = 10_000;
  await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', resolve); });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Loopback listener unavailable');
  origin = `http://127.0.0.1:${address.port}`;
  return { server, origin };
}
