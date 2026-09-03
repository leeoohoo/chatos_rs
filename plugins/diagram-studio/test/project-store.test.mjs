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
