import express from 'express';
import { execFile } from 'node:child_process';
import { promises as fs, watch } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { assertSolutionWorkspace, validateWorkspace, workspaceToMarkdown, type SourceMode } from './schema.js';
import { RevisionConflictError, SolutionWorkspaceStore } from './store.js';
import { readHostRuntimeContext } from './runtime-context.js';

const port = Number.parseInt(process.env.CHATOS_PLUGIN_APP_PORT ?? process.env.SOLUTION_STUDIO_PORT ?? '4198', 10);
const host = process.env.CHATOS_PLUGIN_APP_HOST ?? process.env.SOLUTION_STUDIO_HOST ?? '127.0.0.1';
const store = new SolutionWorkspaceStore();
const runtimeContext = readHostRuntimeContext();
const execFileAsync = promisify(execFile);
await store.initialize();

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '12mb' }));

const subscribers = new Set<express.Response>();
function publishChange() {
  const payload = `event: workspaces-changed\ndata: ${JSON.stringify({ changedAt: new Date().toISOString() })}\n\n`;
  for (const response of subscribers) response.write(payload);
}

app.get('/api/health', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ok: true, service: 'solution-studio', dataDirectory: store.rootDirectory });
});

app.get('/api/context', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json(runtimeContext);
});

app.get('/api/workspaces', async (_request, response, next) => {
  try { response.setHeader('Cache-Control', 'no-store'); response.json({ items: await store.list() }); }
  catch (error) { next(error); }
});

app.post('/api/workspaces', async (request, response, next) => {
  try {
    const title = typeof request.body?.title === 'string' ? request.body.title : '未命名方案';
    const mode: SourceMode = request.body?.sourceMode === 'greenfield' ? 'greenfield' : 'existing-project';
    const artifactKey = typeof request.body?.artifactKey === 'string' ? request.body.artifactKey : undefined;
    const workspace = await store.create(title, mode, artifactKey);
    publishChange();
    response.status(201).json(workspace);
  } catch (error) { next(error); }
});

app.get('/api/workspaces/:workspaceId', async (request, response, next) => {
  try { response.setHeader('Cache-Control', 'no-store'); response.json(await store.read(request.params.workspaceId)); }
  catch (error) { next(error); }
});

app.get('/api/workspaces/:workspaceId/validation', async (request, response, next) => {
  try { response.setHeader('Cache-Control', 'no-store'); response.json(validateWorkspace(await store.read(request.params.workspaceId))); }
  catch (error) { next(error); }
});

app.get('/api/workspaces/:workspaceId/markdown', async (request, response, next) => {
  try {
    const workspace = await store.read(request.params.workspaceId);
    response.setHeader('Content-Disposition', `attachment; filename="${encodeURIComponent(safeMarkdownName(workspace.title))}"`);
    response.type('text/markdown; charset=utf-8').send(workspaceToMarkdown(workspace));
  } catch (error) { next(error); }
});

app.post('/api/workspaces/:workspaceId/markdown/copy-file', async (request, response, next) => {
  try {
    if (process.platform !== 'darwin') throw new Error('当前系统暂不支持把文件直接复制到剪贴板。');
    const workspace = await store.read(request.params.workspaceId);
    const directory = path.resolve(process.env.CHATOS_PLUGIN_ARTIFACT_DIR ?? process.env.SOLUTION_STUDIO_EXPORT_DIR ?? path.join(os.tmpdir(), 'solution-studio-exports'));
    await fs.mkdir(directory, { recursive: true });
    const fileName = safeMarkdownName(workspace.title);
    const filePath = path.join(directory, fileName);
    await fs.writeFile(filePath, workspaceToMarkdown(workspace), { encoding: 'utf8', mode: 0o600 });
    await execFileAsync('/usr/bin/osascript', ['-e', 'on run argv', '-e', 'set the clipboard to (POSIX file (item 1 of argv) as alias)', '-e', 'end run', filePath]);
    response.json({ copied: true, fileName });
  } catch (error) { next(error); }
});

function safeMarkdownName(title: string) {
  const base = title.replace(/[^a-zA-Z0-9\u4e00-\u9fff_-]+/g, '-').replace(/^-+|-+$/g, '') || 'solution-studio-plan';
  return `${base}.md`;
}

app.put('/api/workspaces/:workspaceId', async (request, response, next) => {
  try {
    const expectedRevision = request.body?.expectedRevision;
    if (!Number.isSafeInteger(expectedRevision)) throw new Error('expectedRevision is required.');
    const workspace: unknown = request.body?.workspace;
    assertSolutionWorkspace(workspace);
    if (workspace.workspaceId !== request.params.workspaceId) throw new Error('Workspace identity mismatch.');
    const validation = validateWorkspace(workspace);
    if (validation.issues.some((issue) => ['unknown_dependency', 'self_dependency', 'dependency_cycle'].includes(issue.code))) {
      throw new Error(validation.issues.filter((issue) => issue.blocking).map((issue) => issue.message).join(' '));
    }
    const saved = await store.replace(workspace, expectedRevision);
    publishChange();
    response.json(saved);
  } catch (error) { next(error); }
});

app.delete('/api/workspaces/:workspaceId', async (request, response, next) => {
  try { await store.remove(request.params.workspaceId); publishChange(); response.status(204).end(); }
  catch (error) { next(error); }
});

app.get('/api/events', (request, response) => {
  response.setHeader('Content-Type', 'text/event-stream');
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('Connection', 'keep-alive');
  response.flushHeaders();
  response.write('event: ready\ndata: {}\n\n');
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
  response.status(status).json({ error: error instanceof Error ? error.message : String(error), ...(error instanceof RevisionConflictError ? { actualRevision: error.actualRevision } : {}) });
});

watch(store.rootDirectory, { persistent: false }, (_event, fileName) => { if (fileName?.endsWith('.solution.json')) publishChange(); });
app.listen(port, host, () => process.stdout.write(`Solution Studio is available at http://${host}:${port}\n`));
