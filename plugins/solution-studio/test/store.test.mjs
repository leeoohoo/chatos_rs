import assert from 'node:assert/strict';
import { mkdtemp, readdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { RevisionConflictError, SolutionWorkspaceStore } from '../dist/store.mjs';

test('store provides one workspace and optimistic revisions', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-store-'));
  const store = new SolutionWorkspaceStore(root);
  try {
    const created = await store.create('认证方案', 'existing-project');
    const duplicate = await store.create('另一标题', 'existing-project');
    assert.equal(duplicate.workspaceId, created.workspaceId);

    const edited = structuredClone(created);
    edited.description = '可追踪的认证改造';
    const saved = await store.replace(edited, created.revision);
    assert.equal(saved.revision, created.revision + 1);
    assert.equal((await store.getCurrent()).description, '可追踪的认证改造');

    await assert.rejects(() => store.replace(edited, created.revision), (error) => error instanceof RevisionConflictError && error.actualRevision === saved.revision);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('upsert preserves stable workspace identity', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-upsert-'));
  const store = new SolutionWorkspaceStore(root);
  try {
    const first = await store.upsert('新产品', 'greenfield', (workspace) => workspace);
    const second = await store.upsert('新产品 v2', 'greenfield', (workspace) => {
      workspace.description = '第二版';
      return workspace;
    });
    assert.equal(first.created, true);
    assert.equal(second.created, false);
    assert.equal(second.workspace.workspaceId, first.workspace.workspaceId);
    assert.equal(second.workspace.revision, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('one project keeps exactly one plan', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-single-plan-'));
  const store = new SolutionWorkspaceStore(root, { projectId: 'project-single', projectName: '唯一项目' });
  try {
    const first = await store.upsert('第一份名称', 'existing-project', (workspace) => workspace);
    const second = await store.upsert('更新后的项目规划', 'existing-project', (workspace) => workspace);
    const duplicateCreate = await store.create('不应创建第二份', 'greenfield');
    assert.equal(second.created, false);
    assert.equal(second.workspace.workspaceId, first.workspace.workspaceId);
    assert.equal(duplicateCreate.workspaceId, first.workspace.workspaceId);
    assert.equal((await store.list()).length, 1);
    assert.equal((await store.getCurrent()).title, '更新后的项目规划');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('archive and restart preserves immutable history and creates a blank active plan', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-archive-'));
  const store = new SolutionWorkspaceStore(root, { projectId: 'project-archive', projectName: '归档项目' });
  try {
    const created = await store.create('旧方案', 'existing-project');
    const edited = structuredClone(created);
    edited.requirements.summary = '必须保留的旧需求';
    edited.executionPlan.tasks = [{ id: 'T-001', moduleId: 'MOD-BUSINESS', title: '旧任务', description: '', type: 'task', phase: '执行', dependsOn: [], status: 'done', requirementIds: [], designSectionIds: [], deliverables: [], acceptanceCriteria: ['已验收'], sourceReferences: [] }];
    edited.executionPlan.statusEvents = [{ id: 'EV-001', taskId: 'T-001', from: 'in_progress', to: 'done', occurredAt: new Date().toISOString(), runtimeSessionId: 'test', summary: '完成', evidence: ['proof'] }];
    const saved = await store.replace(edited, created.revision);

    const result = await store.archiveAndCreate({ workspaceId: saved.workspaceId, expectedRevision: saved.revision, title: '全新方案', sourceMode: 'existing-project', reason: '用户明确要求重新规划' });
    assert.notEqual(result.workspace.workspaceId, saved.workspaceId);
    assert.equal(result.workspace.revision, 1);
    assert.equal(result.workspace.title, '全新方案');
    assert.deepEqual(result.workspace.requirements.items, []);
    assert.deepEqual(result.workspace.design.sections, []);
    assert.deepEqual(result.workspace.executionPlan.tasks, []);
    assert.deepEqual(result.workspace.executionPlan.statusEvents, []);
    await assert.rejects(() => store.read(saved.workspaceId), /ENOENT/);

    const archives = await store.listArchives();
    assert.equal(archives.length, 1);
    assert.equal(archives[0].archiveReason, '用户明确要求重新规划');
    assert.equal(archives[0].completedTaskCount, 1);
    const archived = await store.readArchive(archives[0].archiveId);
    assert.equal(archived.workspace.requirements.summary, '必须保留的旧需求');
    assert.equal(archived.workspace.executionPlan.statusEvents[0].summary, '完成');
    assert.equal((await store.getCurrent()).workspaceId, result.workspace.workspaceId);

    await assert.rejects(() => store.archiveAndCreate({ workspaceId: result.workspace.workspaceId, expectedRevision: 999, title: '冲突方案', sourceMode: 'existing-project', reason: '冲突' }), (error) => error instanceof RevisionConflictError);
    assert.equal((await store.listArchives()).length, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('store stamps the host project binding and rejects cross-project reads', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-binding-'));
  const firstProject = { projectId: 'project-a', projectName: '项目 A', connectorWorkspaceId: 'workspace-a', contextScopeId: 'scope-a' };
  try {
    const store = new SolutionWorkspaceStore(root, firstProject);
    const created = await store.create('项目方案', 'existing-project');
    assert.deepEqual(created.hostProject, firstProject);
    assert.deepEqual((await store.list())[0].hostProjectId, 'project-a');

    const otherProjectStore = new SolutionWorkspaceStore(root, { projectId: 'project-b', projectName: '项目 B' });
    await assert.rejects(() => otherProjectStore.read(created.workspaceId), /different ChatOS project/);
    const deviceStore = new SolutionWorkspaceStore(root, undefined);
    await assert.rejects(() => deviceStore.read(created.workspaceId), /no project context is active/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('store deletes legacy and structurally invalid workspace files instead of migrating them', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-no-legacy-'));
  try {
    await writeFile(path.join(root, 'legacy.solution.json'), JSON.stringify({ schemaVersion: 1, workspaceId: 'legacy' }));
    await writeFile(path.join(root, 'invalid-v2.solution.json'), JSON.stringify({ schemaVersion: 2, workspaceId: 'invalid-v2' }));
    await writeFile(path.join(root, 'broken.solution.json'), '{');
    const store = new SolutionWorkspaceStore(root);
    await store.initialize();
    assert.deepEqual((await readdir(root)).filter((name) => name.endsWith('.solution.json')), []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
