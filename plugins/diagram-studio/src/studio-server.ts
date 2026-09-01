import express from 'express';
import { watch } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { DiagramDocumentStore, RevisionConflictError } from './document-store.js';
import { assertDiagramDocument, type DiagramKind } from './schema.js';

const port = Number.parseInt(process.env.DIAGRAM_STUDIO_PORT ?? '4178', 10);
const host = process.env.DIAGRAM_STUDIO_HOST ?? '127.0.0.1';
const store = new DiagramDocumentStore();
await store.initialize();

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '8mb' }));

const subscribers = new Set<express.Response>();
function publishChange(): void {
  const payload = `event: documents-changed\ndata: ${JSON.stringify({ changedAt: new Date().toISOString() })}\n\n`;
  for (const response of subscribers) response.write(payload);
}

app.get('/api/health', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ok: true, service: 'diagram-studio', dataDirectory: store.rootDirectory });
});

app.get('/api/documents', async (_request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json({ items: await store.list() });
  } catch (error) {
    next(error);
  }
});

app.get('/api/projects', async (_request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json({ items: await store.listProjects() });
  } catch (error) {
    next(error);
  }
});

app.get('/api/projects/:projectId', async (request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json(await store.readProject(request.params.projectId));
  } catch (error) {
    next(error);
  }
});

app.post('/api/projects', async (request, response, next) => {
  try {
    const name = typeof request.body?.name === 'string' ? request.body.name : '';
    const project = await store.createProject(name);
    publishChange();
    response.status(201).json(project);
  } catch (error) {
    next(error);
  }
});

app.post('/api/projects/:projectId/documents', async (request, response, next) => {
  try {
    const kind = request.body?.kind as DiagramKind;
    const title = typeof request.body?.title === 'string' ? request.body.title : undefined;
    const document = await store.createInProject(request.params.projectId, kind, title, request.body?.blank === true);
    publishChange();
    response.status(201).json(document);
  } catch (error) {
    next(error);
  }
});

app.get('/api/documents/:documentId', async (request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json(await store.read(request.params.documentId));
  } catch (error) {
    next(error);
  }
});

app.post('/api/documents', async (request, response, next) => {
  try {
    const kind = request.body?.kind as DiagramKind;
    const document = await store.create(kind, typeof request.body?.title === 'string' ? request.body.title : undefined);
    publishChange();
    response.status(201).json(document);
  } catch (error) {
    next(error);
  }
});

app.put('/api/documents/:documentId', async (request, response, next) => {
  try {
    const expectedRevision = request.body?.expectedRevision;
    const document: unknown = request.body?.document;
    if (!Number.isSafeInteger(expectedRevision)) throw new Error('expectedRevision is required.');
    assertDiagramDocument(document);
    if (document.documentId !== request.params.documentId) throw new Error('Document identity mismatch.');
    const saved = await store.replace(document, expectedRevision);
    publishChange();
    response.json(saved);
  } catch (error) {
    next(error);
  }
});

app.post('/api/documents/:documentId/layout', async (request, response, next) => {
  try {
    const expectedRevision = request.body?.expectedRevision;
    if (!Number.isSafeInteger(expectedRevision)) throw new Error('expectedRevision is required.');
    const direction = request.body?.direction === 'DOWN' ? 'DOWN' : request.body?.direction === 'RIGHT' ? 'RIGHT' : undefined;
    const saved = await store.autoLayout(request.params.documentId, expectedRevision, direction);
    publishChange();
    response.json(saved);
  } catch (error) {
    next(error);
  }
});

app.delete('/api/documents/:documentId', async (request, response, next) => {
  try {
    await store.remove(request.params.documentId);
    publishChange();
    response.status(204).end();
  } catch (error) {
    next(error);
  }
});

app.get('/api/events', (request, response) => {
  response.setHeader('Content-Type', 'text/event-stream');
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('Connection', 'keep-alive');
  response.flushHeaders();
  response.write(`event: ready\ndata: {}\n\n`);
  subscribers.add(response);
  request.on('close', () => subscribers.delete(response));
});

const currentDirectory = path.dirname(fileURLToPath(import.meta.url));
const uiDirectory = path.resolve(currentDirectory, '../ui');
app.use(express.static(uiDirectory, {
  etag: true,
  fallthrough: true,
  setHeaders(response) {
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('Referrer-Policy', 'no-referrer');
  }
}));
app.get('*path', (_request, response) => response.sendFile(path.join(uiDirectory, 'index.html')));

app.use((error: unknown, _request: express.Request, response: express.Response, _next: express.NextFunction) => {
  const status = error instanceof RevisionConflictError ? 409 : (error as NodeJS.ErrnoException).code === 'ENOENT' ? 404 : 400;
  response.status(status).json({
    error: error instanceof Error ? error.message : String(error),
    ...(error instanceof RevisionConflictError ? { actualRevision: error.actualRevision } : {})
  });
});

watch(store.rootDirectory, { persistent: false }, (_event, fileName) => {
  if (fileName?.endsWith('.diagram.json') || fileName?.endsWith('.project.json')) publishChange();
});

app.listen(port, host, () => {
  process.stdout.write(`Diagram Studio is available at http://${host}:${port}\n`);
});
