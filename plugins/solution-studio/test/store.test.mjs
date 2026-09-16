import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { RevisionConflictError, SolutionWorkspaceStore } from '../dist/store.mjs';

test('store provides idempotent artifact keys and optimistic revisions', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-store-'));
  const store = new SolutionWorkspaceStore(root);
  try {
    const created = await store.create('认证方案', 'existing-project', 'auth-plan');
    const duplicate = await store.create('另一标题', 'existing-project', 'auth-plan');
    assert.equal(duplicate.workspaceId, created.workspaceId);

    const edited = structuredClone(created);
    edited.description = '可追踪的认证改造';
    const saved = await store.replace(edited, created.revision);
    assert.equal(saved.revision, created.revision + 1);
    assert.equal((await store.findByArtifactKey('auth-plan')).description, '可追踪的认证改造');

    await assert.rejects(() => store.replace(edited, created.revision), (error) => error instanceof RevisionConflictError && error.actualRevision === saved.revision);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('upsert preserves stable workspace identity', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-upsert-'));
  const store = new SolutionWorkspaceStore(root);
  try {
    const first = await store.upsert('greenfield-plan', '新产品', 'greenfield', (workspace) => workspace);
    const second = await store.upsert('greenfield-plan', '新产品 v2', 'greenfield', (workspace) => {
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

test('one project keeps one plan even when callers use different artifact keys', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-single-plan-'));
  const store = new SolutionWorkspaceStore(root, { projectId: 'project-single', projectName: '唯一项目' });
  try {
    const first = await store.upsert('first-plan', '第一份名称', 'existing-project', (workspace) => workspace);
    const second = await store.upsert('another-plan', '更新后的项目规划', 'existing-project', (workspace) => workspace);
    const duplicateCreate = await store.create('不应创建第二份', 'greenfield', 'third-plan');
    assert.equal(second.created, false);
    assert.equal(second.workspace.workspaceId, first.workspace.workspaceId);
    assert.equal(duplicateCreate.workspaceId, first.workspace.workspaceId);
    assert.equal((await store.list()).length, 1);
    assert.equal((await store.getCurrent()).title, '更新后的项目规划');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('store stamps the host project binding and rejects cross-project reads', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-binding-'));
  const firstProject = { projectId: 'project-a', projectName: '项目 A', connectorWorkspaceId: 'workspace-a', contextScopeId: 'scope-a' };
  try {
    const store = new SolutionWorkspaceStore(root, firstProject);
    const created = await store.create('项目方案', 'existing-project', 'project-plan');
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
