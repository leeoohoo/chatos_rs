import express from 'express';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { WebDesignDocumentStore, RevisionConflictError } from './document-store.js';
import { assertWebDesignDocument, type WebDesignDocument } from './schema.js';

const port = Number.parseInt(process.env.CHATOS_PLUGIN_APP_PORT ?? process.env.WEB_DESIGN_STUDIO_PORT ?? '4188', 10);
const host = process.env.CHATOS_PLUGIN_APP_HOST ?? process.env.WEB_DESIGN_STUDIO_HOST ?? '127.0.0.1';
const store = new WebDesignDocumentStore();
await store.initialize();
await store.ensureLegacyProject();

const contextKind = process.env.CHATOS_CONTEXT_SCOPE ?? 'device';
const runtimeContext = {
  kind: contextKind,
  shared: contextKind === 'device',
  ...(process.env.CHATOS_PROJECT_ID ? { chatosProjectId: process.env.CHATOS_PROJECT_ID } : {}),
  ...(process.env.CHATOS_PROJECT_NAME ? { chatosProjectName: process.env.CHATOS_PROJECT_NAME } : {}),
  ...(process.env.CHATOS_WORKSPACE_ID ? { workspaceId: process.env.CHATOS_WORKSPACE_ID } : {})
};
let defaultProjectId: string | undefined;
if (contextKind === 'project' && process.env.CHATOS_PROJECT_ID) {
  const projects = await store.listProjects();
  if (projects.length === 0) {
    defaultProjectId = (await store.createProject(process.env.CHATOS_PROJECT_NAME?.trim() || 'ChatOS 网站项目')).projectId;
  } else if (projects.length === 1) {
    defaultProjectId = projects[0].projectId;
  }
}

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '50mb' }));

app.get('/api/health', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ok: true, service: 'web-design-studio', dataDirectory: store.rootDirectory });
});

app.get('/api/context', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ...runtimeContext, ...(defaultProjectId ? { defaultProjectId } : {}) });
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
    const description = typeof request.body?.description === 'string' ? request.body.description : undefined;
    response.status(201).json(await store.createProject(name, description));
  } catch (error) {
    next(error);
  }
});

app.patch('/api/projects/:projectId', async (request, response, next) => {
  try {
    response.json(await store.updateProject(request.params.projectId, {
      name: typeof request.body?.name === 'string' ? request.body.name : undefined,
      description: typeof request.body?.description === 'string' ? request.body.description : undefined
    }));
  } catch (error) {
    next(error);
  }
});

app.delete('/api/projects/:projectId', async (request, response, next) => {
  try {
    await store.deleteProject(request.params.projectId, request.query.deleteDocuments === 'true');
    response.status(204).end();
  } catch (error) {
    next(error);
  }
});

app.post('/api/projects/:projectId/documents', async (request, response, next) => {
  try {
    const title = typeof request.body?.title === 'string' ? request.body.title : undefined;
    response.status(201).json(await store.createInProject(request.params.projectId, title, request.body?.blank === true));
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
  etag: false,
  lastModified: false,
  fallthrough: true,
  setHeaders(response) {
    response.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
    response.setHeader('Pragma', 'no-cache');
    response.setHeader('Expires', '0');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('Referrer-Policy', 'no-referrer');
  }
}));
app.get('*path', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
  response.setHeader('Pragma', 'no-cache');
  response.setHeader('Expires', '0');
  response.sendFile(path.join(uiDirectory, 'index.html'));
});

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
