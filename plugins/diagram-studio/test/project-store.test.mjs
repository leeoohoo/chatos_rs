import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { DiagramDocumentStore } from '../dist/document-store.test.mjs';

test('projects contain separately named diagrams, including sequence diagrams', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-project-test-'));
  try {
    const store = new DiagramDocumentStore(root);
    const project = await store.createProject('Chatos 客户端');
    assert.equal(project.name, 'Chatos 客户端');
    assert.deepEqual(project.diagramIds, []);

    const diagram = await store.createInProject(project.projectId, 'sequence', '登录认证时序', true);
    assert.equal(diagram.kind, 'sequence');
    assert.equal(diagram.title, '登录认证时序');
    assert.deepEqual(diagram.nodes, []);
    assert.deepEqual(diagram.edges, []);
    assert.deepEqual(diagram.viewport, { x: 0, y: 0, zoom: 1 });

    const updatedProject = await store.readProject(project.projectId);
    assert.deepEqual(updatedProject.diagramIds, [diagram.documentId]);
    const summaries = await store.listProjects();
    assert.equal(summaries[0].diagramCount, 1);
    assert.deepEqual(summaries[0].diagramIds, [diagram.documentId]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('stable artifact keys make AI retries idempotent inside one project', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-artifact-test-'));
  try {
    const store = new DiagramDocumentStore(root);
    const project = await store.createProject('Relay');
    const attempts = await Promise.all([
      store.createOrGetInProject(project.projectId, 'architecture', '系统架构', true, 'system-architecture'),
      store.createOrGetInProject(project.projectId, 'architecture', '系统架构', true, 'system-architecture'),
      store.createOrGetInProject(project.projectId, 'architecture', '系统架构', true, 'system-architecture')
    ]);
    assert.equal(new Set(attempts.map((attempt) => attempt.document.documentId)).size, 1);
    assert.equal(attempts.filter((attempt) => attempt.created).length, 1);
    assert.equal((await store.readProject(project.projectId)).diagramIds.length, 1);

    const original = attempts[0].document;
    const unchanged = await store.upsertInProject(project.projectId, structuredClone(original), 'system-architecture');
    assert.equal(unchanged.reused, true);
    assert.equal(unchanged.document.revision, 1);

    const updated = await store.upsertInProject(project.projectId, { ...structuredClone(original), title: 'Relay 系统架构' }, 'system-architecture');
    assert.equal(updated.created, false);
    assert.equal(updated.reused, false);
    assert.equal(updated.document.documentId, original.documentId);
    assert.equal(updated.document.revision, 2);
    assert.equal((await store.readProject(project.projectId)).diagramIds.length, 1);

    const copyOne = await store.createNewInProjectIdempotent(project.projectId, 'architecture', '系统架构副本', true, 'create-copy-1');
    const copyOneRetry = await store.createNewInProjectIdempotent(project.projectId, 'architecture', '系统架构副本', true, 'create-copy-1');
    const copyTwo = await store.createNewInProjectIdempotent(project.projectId, 'architecture', '系统架构副本', true, 'create-copy-2');
    assert.equal(copyOneRetry.document.documentId, copyOne.document.documentId);
    assert.notEqual(copyTwo.document.documentId, copyOne.document.documentId);
    assert.equal((await store.readProject(project.projectId)).diagramIds.length, 3);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('deleting a diagram also removes it from its project', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-delete-test-'));
  try {
    const store = new DiagramDocumentStore(root);
    const project = await store.createProject('可删除图形');
    const first = await store.createInProject(project.projectId, 'architecture', '系统架构', true);
    const second = await store.createInProject(project.projectId, 'sequence', '调用时序', true);

    await store.remove(first.documentId);

    const updatedProject = await store.readProject(project.projectId);
    assert.deepEqual(updatedProject.diagramIds, [second.documentId]);
    assert.equal((await store.list()).some((item) => item.documentId === first.documentId), false);
    await assert.rejects(() => store.read(first.documentId));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('scope-bound projects and documents cannot be read or moved across ChatOS scopes', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-scope-test-'));
  try {
    const store = new DiagramDocumentStore(root);
    const scopeA = 'a'.repeat(64);
    const scopeB = 'b'.repeat(64);
    const projectA = await store.createProject('User A', undefined, scopeA);
    const projectB = await store.createProject('User B', undefined, scopeB);
    const documentA = await store.createInProject(projectA.projectId, 'architecture', 'A architecture', true);
    assert.deepEqual((await store.listProjects(scopeA)).map((project) => project.projectId), [projectA.projectId]);
    assert.deepEqual((await store.listInScope(scopeB)), []);
    await assert.rejects(() => store.readProjectInScope(projectA.projectId, scopeB), /different ChatOS user or project scope/);
    await assert.rejects(() => store.readInScope(documentA.documentId, scopeB), /different ChatOS user or project scope/);
    await assert.rejects(() => store.moveDocument(documentA.documentId, projectB.projectId, projectA.projectId, scopeA), /different ChatOS user or project scope/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('a transmitted scope automatically gets one stable default project, including public scope', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-public-scope-test-'));
  try {
    const store = new DiagramDocumentStore(root);
    const publicScope = 'd'.repeat(64);
    const first = await store.ensureScopedProject(publicScope, '公共图形');
    const second = await store.ensureScopedProject(publicScope, 'Ignored replacement name');
    assert.equal(second.projectId, first.projectId);
    assert.equal(first.name, '公共图形');
    assert.equal(first.isScopeDefault, true);
    assert.equal(first.scopeKey, publicScope);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('all legacy projects in an already isolated data root are migrated without disappearing', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-legacy-scope-test-'));
  try {
    const store = new DiagramDocumentStore(root);
    const first = await store.createProject('Legacy A');
    const second = await store.createProject('Legacy B');
    const scope = 'e'.repeat(64);
    const defaultProject = await store.ensureScopedProject(scope, 'Unused');
    const migrated = await store.listProjects(scope);
    assert.equal(migrated.length, 2);
    assert.deepEqual(new Set(migrated.map((project) => project.projectId)), new Set([first.projectId, second.projectId]));
    assert.ok([first.projectId, second.projectId].includes(defaultProject.projectId));
    assert.equal((await store.readProject(defaultProject.projectId)).isScopeDefault, true);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
