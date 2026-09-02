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
