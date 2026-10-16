import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('studio serves the Apple-style workbench and persists workspaces', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-server-'));
  const port = await availablePort();
  const child = spawn(process.execPath, ['bin/chatos-solution-studio', 'studio'], {
    cwd: process.cwd(),
    env: { ...process.env, CHATOS_PLUGIN_APP_HOST: '127.0.0.1', CHATOS_PLUGIN_APP_PORT: String(port), CHATOS_PLUGIN_DATA_DIR: root, CHATOS_CONTEXT_SCOPE: 'project', CHATOS_CONTEXT_SCOPE_ID: 'scope-test-123', CHATOS_PROJECT_ID: 'project-test-123', CHATOS_PROJECT_NAME: 'chatos_rs', CHATOS_WORKSPACE_ID: 'workspace-test-456', CHATOS_WORKSPACE: '/workspace/chatos_rs' },
    stdio: ['ignore', 'pipe', 'pipe']
  });
  try {
    await waitForReady(child, port);
    const base = `http://127.0.0.1:${port}`;
    const page = await fetch(base);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Solution Studio/);
    const css = await fetch(`${base}/assets/app.css`).then((response) => response.text());
    assert.match(css, /-apple-system/);
    assert.match(css, /prefers-color-scheme:dark/);

    const context = await fetch(`${base}/api/context`).then((response) => response.json());
    assert.equal(context.hasProjectContext, true);
    assert.equal(context.projectId, 'project-test-123');
    assert.equal(context.projectName, 'chatos_rs');
    assert.equal(context.connectorWorkspaceId, 'workspace-test-456');
    assert.equal(context.projectRoot, '/workspace/chatos_rs');

    const created = await fetch(`${base}/api/workspaces`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title: '插件能力升级', sourceMode: 'existing-project' }) }).then((response) => response.json());
    assert.equal(created.title, '插件能力升级');
    assert.equal(created.revision, 1);
    assert.deepEqual(created.hostProject, { projectId: 'project-test-123', projectName: 'chatos_rs', connectorWorkspaceId: 'workspace-test-456', contextScopeId: 'scope-test-123' });
    const listed = await fetch(`${base}/api/workspaces`).then((response) => response.json());
    assert.equal(listed.items.length, 1);
    const markdownResponse = await fetch(`${base}/api/workspaces/${created.workspaceId}/markdown`);
    assert.match(markdownResponse.headers.get('content-disposition') ?? '', /^attachment; filename=/);
    const markdown = await markdownResponse.text();
    assert.match(markdown, /^# 插件能力升级/);

    const plannedTask = (id, dependsOn = []) => ({
      id,
      moduleId: 'MOD-CORE',
      title: id,
      description: '',
      type: 'task',
      phase: '实现',
      dependsOn,
      status: 'planned',
      requirementIds: [],
      designSectionIds: [],
      deliverables: ['产物'],
      acceptanceCriteria: ['可验收'],
      sourceReferences: []
    });
    const valid = structuredClone(created);
    valid.requirements.modules = [{ id: 'MOD-CORE', title: '核心', kind: 'business', description: '', order: 0 }];
    valid.executionPlan.tasks = [plannedTask('T-001')];
    const savedResponse = await saveWorkspace(base, valid, created.revision);
    assert.equal(savedResponse.status, 200);
    const saved = await savedResponse.json();

    const unknownModule = structuredClone(saved);
    unknownModule.executionPlan.tasks[0].moduleId = 'MOD-MISSING';
    const unknownModuleResponse = await saveWorkspace(base, unknownModule, saved.revision);
    assert.equal(unknownModuleResponse.status, 400);
    assert.match((await unknownModuleResponse.json()).error, /模块|module/i);

    const redundant = structuredClone(saved);
    redundant.executionPlan.tasks = [
      plannedTask('T-001'),
      plannedTask('T-002', ['T-001']),
      plannedTask('T-003', ['T-001', 'T-002'])
    ];
    const redundantResponse = await saveWorkspace(base, redundant, saved.revision);
    assert.equal(redundantResponse.status, 400);
    assert.match((await redundantResponse.json()).error, /冗余|redundant/i);

    const forgedHistory = structuredClone(saved);
    forgedHistory.executionPlan.statusEvents.push({
      id: 'EVT-001', taskId: 'T-001', from: 'planned', to: 'in_progress',
      occurredAt: new Date().toISOString(), runtimeSessionId: 'runtime-1', evidence: []
    });
    const forgedHistoryResponse = await saveWorkspace(base, forgedHistory, saved.revision);
    assert.equal(forgedHistoryResponse.status, 400);
    assert.match((await forgedHistoryResponse.json()).error, /状态历史/);

    const running = structuredClone(saved);
    running.executionPlan.tasks[0].status = 'in_progress';
    running.executionPlan.statusEvents = [{
      id: 'EVT-001', taskId: 'T-001', from: 'planned', to: 'in_progress',
      occurredAt: new Date().toISOString(), runtimeSessionId: 'runtime-1', evidence: []
    }];
    await writeFile(path.join(root, `${running.workspaceId}.solution.json`), `${JSON.stringify(running, null, 2)}\n`);

    const changedRunningTask = structuredClone(running);
    changedRunningTask.executionPlan.tasks[0].title = '绕过冻结定义';
    const changedRunningResponse = await saveWorkspace(base, changedRunningTask, running.revision);
    assert.equal(changedRunningResponse.status, 400);
    assert.match((await changedRunningResponse.json()).error, /不能修改其定义或依赖/);

    const deletedRunningTask = structuredClone(running);
    deletedRunningTask.executionPlan.tasks = [];
    const deletedRunningResponse = await saveWorkspace(base, deletedRunningTask, running.revision);
    assert.equal(deletedRunningResponse.status, 400);
    assert.match((await deletedRunningResponse.json()).error, /不能删除/);
  } finally {
    if (child.exitCode === null) {
      child.kill('SIGTERM');
      await new Promise((resolve) => child.once('exit', resolve));
    }
    await rm(root, { recursive: true, force: true });
  }
});

function saveWorkspace(base, workspace, expectedRevision) {
  return fetch(`${base}/api/workspaces/${workspace.workspaceId}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ expectedRevision, workspace })
  });
}

async function availablePort() {
  const server = createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function waitForReady(child, port) {
  const deadline = Date.now() + 8_000;
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Studio exited before startup: ${stderr}`);
    try { if ((await fetch(`http://127.0.0.1:${port}/api/health`)).ok) return; } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out waiting for Solution Studio: ${stderr}`);
}
