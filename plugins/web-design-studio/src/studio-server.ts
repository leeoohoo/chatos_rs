import express from 'express';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { WebDesignDocumentStore, RevisionConflictError } from './document-store.js';
import { assertWebDesignDocument, type WebDesignDocument } from './schema.js';

const port = Number.parseInt(process.env.CHATOS_PLUGIN_APP_PORT ?? process.env.WEB_DESIGN_STUDIO_PORT ?? '4188', 10);
const host = process.env.CHATOS_PLUGIN_APP_HOST ?? process.env.WEB_DESIGN_STUDIO_HOST ?? '127.0.0.1';
const store = new WebDesignDocumentStore();
await store.initialize();

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '50mb' }));

app.get('/api/health', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ok: true, service: 'web-design-studio', dataDirectory: store.rootDirectory });
});

app.get('/api/documents', async (_request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json({ items: await store.list() });
  } catch (error) {
    next(error);
  }
});

app.post('/api/documents', async (request, response, next) => {
  try {
    const title = typeof request.body?.title === 'string' ? request.body.title : undefined;
    response.status(201).json(await store.create(title));
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

app.put('/api/documents/:documentId', async (request, response, next) => {
  try {
    const expectedRevision = request.body?.expectedRevision;
    const submitted: unknown = request.body?.document;
    if (!Number.isSafeInteger(expectedRevision)) throw new Error('expectedRevision is required.');
    assertWebDesignDocument(submitted);
    const document = submitted as WebDesignDocument;
    if (document.documentId !== request.params.documentId) throw new Error('Document identity mismatch.');
    response.json(await store.replace(document, expectedRevision));
  } catch (error) {
    next(error);
  }
});

app.delete('/api/documents/:documentId', async (request, response, next) => {
  try {
    await store.remove(request.params.documentId);
    response.status(204).end();
  } catch (error) {
    next(error);
  }
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

app.listen(port, host, () => {
  process.stdout.write(`Web Design Studio is available at http://${host}:${port}\n`);
});
