import express from 'express';
import { execFile } from 'node:child_process';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import {
  readPluginLocalHttpEndpoint,
  startPluginDirectoryMonitor
} from '../../shared/local-http-application-runtime.js';
import { assertSolutionWorkspace, validateWorkspace, workspaceToMarkdown, type SourceMode } from './schema.js';
import { RevisionConflictError, SolutionWorkspaceStore } from './store.js';
import { readHostRuntimeContext } from './runtime-context.js';

const { host, port } = readPluginLocalHttpEndpoint({
  defaultPort: 4198
});
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
    const workspace = await store.create(title, mode);
    publishChange();
    response.status(201).json(workspace);
  } catch (error) { next(error); }
});

app.post('/api/workspaces/:workspaceId/archive-and-create', async (request, response, next) => {
  try {
    const expectedRevision = request.body?.expectedRevision;
    if (!Number.isSafeInteger(expectedRevision)) throw new Error('expectedRevision is required.');
    const title = typeof request.body?.title === 'string' ? request.body.title : '';
    const reason = typeof request.body?.reason === 'string' ? request.body.reason : '';
    const mode: SourceMode = request.body?.sourceMode === 'greenfield' ? 'greenfield' : 'existing-project';
    const result = await store.archiveAndCreate({ workspaceId: request.params.workspaceId, expectedRevision, title, sourceMode: mode, reason });
    publishChange();
    response.status(201).json(result);
  } catch (error) { next(error); }
});

app.get('/api/archives', async (_request, response, next) => {
  try { response.setHeader('Cache-Control', 'no-store'); response.json({ items: await store.listArchives() }); }
  catch (error) { next(error); }
});

app.get('/api/archives/:archiveId', async (request, response, next) => {
  try { response.setHeader('Cache-Control', 'no-store'); response.json(await store.readArchive(request.params.archiveId)); }
  catch (error) { next(error); }
});

app.get('/api/archives/:archiveId/markdown', async (request, response, next) => {
  try {
    const record = await store.readArchive(request.params.archiveId);
    response.setHeader('Content-Disposition', `attachment; filename="${encodeURIComponent(safeMarkdownName(record.workspace.title))}"`);
    response.type('text/markdown; charset=utf-8').send(workspaceToMarkdown(record.workspace));
  } catch (error) { next(error); }
});

app.post('/api/archives/:archiveId/markdown/copy-file', async (request, response, next) => {
  try {
    if (process.platform !== 'darwin') throw new Error('当前系统暂不支持把文件直接复制到剪贴板。');
    const record = await store.readArchive(request.params.archiveId);
    const directory = path.resolve(process.env.CHATOS_PLUGIN_ARTIFACT_DIR ?? path.join(os.tmpdir(), 'solution-studio-exports'));
    await fs.mkdir(directory, { recursive: true });
    const fileName = safeMarkdownName(`${record.workspace.title}-历史方案`);
    const filePath = path.join(directory, fileName);
    await fs.writeFile(filePath, workspaceToMarkdown(record.workspace), { encoding: 'utf8', mode: 0o600 });
    await execFileAsync('/usr/bin/osascript', ['-e', 'on run argv', '-e', 'set the clipboard to (POSIX file (item 1 of argv) as alias)', '-e', 'end run', filePath]);
    response.json({ copied: true, fileName });
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
    const directory = path.resolve(process.env.CHATOS_PLUGIN_ARTIFACT_DIR ?? path.join(os.tmpdir(), 'solution-studio-exports'));
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
    const current = await store.read(workspace.workspaceId);
    const currentById = new Map(current.executionPlan.tasks.map((task) => [task.id, task]));
    if (JSON.stringify(workspace.executionPlan.statusEvents) !== JSON.stringify(current.executionPlan.statusEvents)) {
      throw new Error('Workbench 不能修改任务状态历史，请使用任务执行器。');
    }
    for (const task of workspace.executionPlan.tasks) {
      const previous = currentById.get(task.id);
      if (!previous && task.status !== 'planned') throw new Error(`新任务 ${task.id} 的状态必须是 planned。`);
      if (previous && (task.status !== previous.status || task.blockedReason !== previous.blockedReason)) {
        throw new Error(`Workbench 不能修改任务 ${task.id} 的执行状态，请使用任务执行器。`);
      }
    }
    const proposedById = new Map(workspace.executionPlan.tasks.map((task) => [task.id, task]));
    for (const previous of current.executionPlan.tasks.filter((task) => task.status !== 'planned')) {
      const proposed = proposedById.get(previous.id);
      if (!proposed) throw new Error(`任务 ${previous.id} 已进入 ${previous.status}，Workbench 不能删除。`);
      const { status: _previousStatus, blockedReason: _previousReason, ...previousDefinition } = previous;
      const { status: _proposedStatus, blockedReason: _proposedReason, ...proposedDefinition } = proposed;
      if (JSON.stringify(previousDefinition) !== JSON.stringify(proposedDefinition)) {
        throw new Error(`任务 ${previous.id} 已进入 ${previous.status}，Workbench 不能修改其定义或依赖。`);
      }
    }
    const validation = validateWorkspace(workspace);
    const rejectedCodes = new Set([
      'unknown_module',
      'unknown_dependency',
      'self_dependency',
      'dependency_cycle',
      'redundant_dependency',
      'unknown_parent_requirement',
      'self_parent_requirement',
      'requirement_hierarchy_cycle',
      'parent_module_mismatch'
    ]);
    const rejectedIssues = validation.issues.filter((issue) => rejectedCodes.has(issue.code));
    if (rejectedIssues.length > 0) {
      throw new Error(rejectedIssues.map((issue) => issue.message).join(' '));
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

startPluginDirectoryMonitor({
  directory: store.rootDirectory,
  accepts: (fileName) => fileName.endsWith('.solution.json'),
  onChange: publishChange
});
app.listen(port, host, () => process.stdout.write(`Solution Studio is available at http://${host}:${port}\n`));
