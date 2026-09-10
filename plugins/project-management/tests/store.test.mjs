import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { PlanningStore, readContext } from '../dist/store.mjs';

function fixture(t) {
  const dir = mkdtempSync(path.join(tmpdir(), 'chatos-planning-test-'));
  const env = {
    CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'client-project',
    CHATOS_CONTEXT_SCOPE_ID: 'a'.repeat(64), CHATOS_PLUGIN_DATA_DIR: dir
  };
  const store = new PlanningStore(env);
  t.after(() => { store.close(); rmSync(dir, { recursive: true }); });
  const change = fields => store.mutate({ requestId: randomUUID(), expectedRevision: store.read().revision, ...fields });
  const requirement = (title = '需求', parentId = null) => change({
    operation: 'requirement.create', title, detail: `${title}范围`, acceptanceCriteria: `${title}验收`, parentId, status: 'draft'
  }).result.id;
  const updateRequirement = (id, patch) => {
    const current = store.read().requirements.find(value => value.id === id);
    return change({ operation: 'requirement.update', ...current, ...patch });
  };
  const document = (requirementIds, title = '技术方案', status = 'published', content = '# 技术方案\n实现与验证') => change({
    operation: 'document.create', title, kind: 'technical', format: 'markdown', status, requirementIds, content
  }).result.id;
  const workItem = (requirementId, title = '实现') => change({
    operation: 'work_item.create', title, detail: `${title}步骤`, acceptanceCriteria: `${title}单测通过`, requirementId, status: 'todo'
  }).result.id;
  const updateWorkItem = (id, patch) => {
    const current = store.read().workItems.find(value => value.id === id);
    return change({ operation: 'work_item.update', ...current, ...patch });
  };
  const ready = (requirementId, title = '实现') => {
    document([requirementId]);
    updateRequirement(requirementId, { status: 'approved' });
    const id = workItem(requirementId, title);
    updateWorkItem(id, { status: 'ready' });
    return id;
  };
  return { env, store, dir, change, requirement, updateRequirement, document, workItem, updateWorkItem, ready };
}

test('requires strict host-bound project context and never falls back to cwd or device scope', () => {
  assert.throws(() => readContext({}));
  const env = { CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'p', CHATOS_CONTEXT_SCOPE_ID: 'a'.repeat(64), CHATOS_PLUGIN_DATA_DIR: tmpdir() };
  for (const patch of [{ CHATOS_CONTEXT_SCOPE: 'device' }, { CHATOS_PROJECT_ID: ' p' }, { CHATOS_CONTEXT_SCOPE_ID: '' }, { CHATOS_PLUGIN_DATA_DIR: '.' }]) {
    assert.throws(() => readContext({ ...env, ...patch }));
  }
});

test('plugin owns business data and independent connections share one scoped SQLite state', t => {
  const { store, env, requirement } = fixture(t);
  const id = requirement();
  const other = new PlanningStore(env);
  try {
    assert.equal(other.read().requirements[0].id, id);
    assert.equal(other.read().revision, 1);
  } finally { other.close(); }
  assert.equal(store.read().requirements[0].projectId, undefined);
  assert.equal(store.read().projects, undefined);
  assert.equal(store.read().runs, undefined);
});

test('rejects another project or account-derived scope reopening the same data directory', t => {
  const { env, requirement } = fixture(t); requirement();
  assert.throws(() => new PlanningStore({ ...env, CHATOS_PROJECT_ID: 'other' }), /another bound context/);
  assert.throws(() => new PlanningStore({ ...env, CHATOS_CONTEXT_SCOPE_ID: 'b'.repeat(64) }), /another bound context/);
});

test('strict commands reject project CRUD, Git, execution status and owner injection', t => {
  const { store } = fixture(t);
  const base = { operation: 'requirement.create', requestId: randomUUID(), expectedRevision: 0, title: 'x', detail: '', acceptanceCriteria: '', parentId: null, status: 'draft' };
  for (const field of ['project_id', 'projectId', 'owner', 'rootPath', 'gitBranch', 'taskRunnerStatus']) {
    assert.throws(() => store.mutate({ ...base, [field]: 'injected' }));
  }
  for (const operation of ['project.create', 'git.commit', 'task.execute', 'execution.update']) {
    assert.throws(() => store.mutate({ operation, requestId: randomUUID(), expectedRevision: 0 }));
  }
  assert.equal(store.read().revision, 0);
});

test('CAS and persistent idempotency prevent lost updates and duplicate creation', t => {
  const { store, env } = fixture(t);
  const command = { operation: 'requirement.create', requestId: randomUUID(), expectedRevision: 0, title: 'x', detail: '', acceptanceCriteria: '', parentId: null, status: 'draft' };
  const first = store.mutate(command);
  const other = new PlanningStore(env);
  try {
    assert.deepEqual(other.mutate(Object.fromEntries(Object.entries(command).reverse())), first);
    assert.throws(() => other.mutate({ ...command, title: 'different' }), /different content/);
    assert.throws(() => other.mutate({ ...command, requestId: randomUUID() }), /reload/);
  } finally { other.close(); }
  assert.equal(store.read().requirements.length, 1);
});

test('requirement tree exposes ancestors, descendants, related work and documents', t => {
  const { store, requirement, document, workItem } = fixture(t);
  const root = requirement('根需求');
  const child = requirement('子需求', root);
  const grandchild = requirement('孙需求', child);
  const doc = document([child], '子需求方案');
  const work = workItem(child, '子工作项');
  const graph = store.scope('requirement', child);
  assert.deepEqual(graph.ancestors, [root]);
  assert.deepEqual(graph.descendants, [grandchild]);
  assert.ok(graph.requirementIds.includes(child) && graph.requirementIds.includes(grandchild));
  assert.deepEqual(graph.workItemIds, [work]);
  assert.deepEqual(graph.documentIds, [doc]);
  assert.ok(graph.edges.some(value => value.relation === 'implements' && value.to === work));
});

test('requirement and work-item dependencies expose transitive forward and reverse closure', t => {
  const { store, requirement, workItem, change } = fixture(t);
  const a = requirement('A'), b = requirement('B'), c = requirement('C');
  change({ operation: 'dependencies.set', kind: 'requirement', id: b, prerequisiteIds: [a] });
  change({ operation: 'dependencies.set', kind: 'requirement', id: c, prerequisiteIds: [b] });
  assert.deepEqual(new Set(store.scope('requirement', c).prerequisites), new Set([a, b]));
  assert.deepEqual(new Set(store.scope('requirement', a).dependants), new Set([b, c]));
  assert.throws(() => change({ operation: 'dependencies.set', kind: 'requirement', id: a, prerequisiteIds: [c] }), /cycle/);
  const wa = workItem(a, 'WA'), wb = workItem(b, 'WB');
  change({ operation: 'dependencies.set', kind: 'work_item', id: wb, prerequisiteIds: [wa] });
  assert.deepEqual(store.scope('work_item', wb).prerequisites, [wa]);
  assert.deepEqual(store.scope('work_item', wa).dependants, [wb]);
});

test('entity body and dependency changes commit atomically under one revision', t => {
  const { store, requirement, updateRequirement } = fixture(t);
  const a = requirement('A'), b = requirement('B');
  const revision = store.read().revision;
  updateRequirement(b, { title: 'B updated', prerequisiteIds: [a] });
  assert.equal(store.read().revision, revision + 1);
  assert.equal(store.read().requirements.find(value => value.id === b).title, 'B updated');
  const before = store.read();
  assert.throws(() => updateRequirement(a, { title: 'Must roll back', prerequisiteIds: [b] }), /cycle/);
  assert.deepEqual(store.read(), before);
});

test('documents have independent identities, immutable versions, multiple kinds and many-to-many links', t => {
  const { store, requirement, document, change } = fixture(t);
  const a = requirement('A'), b = requirement('B');
  const id = document([a, b], '接口方案', 'draft', '# v1');
  const first = store.getDocument(id);
  assert.equal(first.versions.length, 1);
  assert.deepEqual(new Set(first.requirementIds), new Set([a, b]));
  change({ operation: 'document.revise', id, title: '接口方案', kind: 'api', format: 'markdown', status: 'published', requirementIds: [a], content: '# v2' });
  const revised = store.getDocument(id);
  assert.equal(revised.currentVersion, 2);
  assert.deepEqual(revised.versions.map(value => value.content), ['# v2', '# v1']);
  assert.notEqual(revised.versions[0].contentSha256, revised.versions[1].contentSha256);
  assert.deepEqual(revised.requirementIds, [a]);
});

test('published documents and acceptance criteria gate requirement approval and work readiness', t => {
  const { requirement, updateRequirement, document, workItem, updateWorkItem } = fixture(t);
  const id = requirement();
  assert.throws(() => updateRequirement(id, { status: 'approved' }), /published planning document/);
  document([id], '草稿', 'draft');
  assert.throws(() => updateRequirement(id, { status: 'approved' }), /published planning document/);
  document([id], '发布方案', 'published');
  assert.throws(() => updateRequirement(id, { status: 'approved', acceptanceCriteria: '' }), /criteria/);
  updateRequirement(id, { status: 'approved' });
  const work = workItem(id);
  updateWorkItem(work, { status: 'ready' });
});

test('approved linked requirements must reopen before document revision', t => {
  const { store, requirement, document, updateRequirement, change } = fixture(t);
  const requirementId = requirement();
  const documentId = document([requirementId]);
  updateRequirement(requirementId, { status: 'approved' });
  const current = store.getDocument(documentId);
  assert.throws(() => change({
    operation: 'document.revise', id: documentId, title: current.title, kind: current.kind,
    format: current.format, status: current.status, requirementIds: current.requirementIds, content: '# changed'
  }), /Reopen approved/);
});

test('plan automatically closes prerequisite scope and pins exact document versions', t => {
  const { store, requirement, ready, change, updateRequirement } = fixture(t);
  const a = requirement('A'), b = requirement('B');
  const wa = ready(a, 'WA'), wb = ready(b, 'WB');
  change({ operation: 'dependencies.set', kind: 'requirement', id: b, prerequisiteIds: [a] });
  change({ operation: 'dependencies.set', kind: 'work_item', id: wb, prerequisiteIds: [wa] });
  const planId = change({ operation: 'plan.create', title: '闭包规划', workItemIds: [wb] }).result.id;
  const frozen = store.read().plans.find(value => value.id === planId);
  assert.deepEqual(new Set(frozen.workItems.map(value => value.id)), new Set([wa, wb]));
  assert.deepEqual(new Set(frozen.requirements.map(value => value.id)), new Set([a, b]));
  assert.ok(frozen.documents.every(value => value.pinnedVersion.version === 1));
  const originalContent = frozen.documents[0].pinnedVersion.content;
  updateRequirement(a, { status: 'draft' });
  const doc = store.getDocument(frozen.documents.find(value => value.requirementIds.includes(a)).id);
  change({ operation: 'document.revise', id: doc.id, title: doc.title, kind: doc.kind, format: doc.format, status: doc.status, requirementIds: doc.requirementIds, content: '# revised' });
  assert.equal(store.read().plans.find(value => value.id === planId).documents[0].pinnedVersion.content, originalContent);
});

test('execution intent requires separate approval and stores only opaque Task Runner references', t => {
  const { store, requirement, ready, change } = fixture(t);
  const requirementId = requirement(); const workId = ready(requirementId);
  const planId = change({ operation: 'plan.create', title: 'v1', workItemIds: [workId] }).result.id;
  assert.throws(() => change({ operation: 'execution.prepare', planId, title: '执行 v1' }), /approved frozen plan/);
  change({ operation: 'plan.approve', id: planId });
  const intentId = change({ operation: 'execution.prepare', planId, title: '执行 v1' }).result.id;
  assert.throws(() => change({ operation: 'execution.link', intentId, referenceType: 'batch', externalId: 'batch-1', url: null, revision: null }), /approved intent/);
  change({ operation: 'execution.approve', id: intentId });
  const referenceId = change({ operation: 'execution.link', intentId, referenceType: 'batch', externalId: 'batch-1', url: 'https://tasks.example/batch-1', revision: '7' }).result.id;
  const reference = store.read().executionReferences.find(value => value.id === referenceId);
  assert.equal(reference.provider, 'task-runner');
  for (const field of ['status', 'running', 'failedAt', 'completedAt']) assert.equal(reference[field], undefined);
  assert.throws(() => change({ operation: 'execution.link', intentId, referenceType: 'run', externalId: 'run-1', url: null, revision: null, status: 'running' }));
});

test('host batch references persist atomically and retry without duplicating opaque ids', t => {
  const { store, requirement, ready, change } = fixture(t);
  const requirementId = requirement(); const workId = ready(requirementId);
  const planId = change({ operation: 'plan.create', title: 'v1', workItemIds: [workId] }).result.id;
  change({ operation: 'plan.approve', id: planId });
  const intentId = change({ operation: 'execution.prepare', planId, title: '执行 v1' }).result.id;
  change({ operation: 'execution.approve', id: intentId });
  const command = { operation: 'execution.link_batch', intentId, batchId: 'host-batch-1', revision: 'digest-1', tasks: [{ clientRef: workId, taskId: 'task-1' }] };
  change(command); change(command);
  const references = store.read().executionReferences;
  assert.equal(references.length, 2);
  assert.deepEqual(new Set(references.map(value => value.referenceType)), new Set(['batch', 'task']));
  assert.ok(references.every(value => value.provider === 'task-runner' && value.status === undefined));
});

test('old v1 databases fail closed and require explicit import instead of compatibility migration', t => {
  const root = mkdtempSync(path.join(tmpdir(), 'chatos-planning-v1-'));
  t.after(() => rmSync(root, { recursive: true }));
  const database = new DatabaseSync(path.join(root, 'planning.sqlite3'));
  database.exec('CREATE TABLE legacy(id TEXT); PRAGMA user_version=1;'); database.close();
  const env = { CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'p', CHATOS_CONTEXT_SCOPE_ID: 'a'.repeat(64), CHATOS_PLUGIN_DATA_DIR: root };
  assert.throws(() => new PlanningStore(env), /explicit import/);
});

test('corrupt and symlink databases fail closed without initializing replacement data', t => {
  const root = mkdtempSync(path.join(tmpdir(), 'chatos-planning-corruption-'));
  t.after(() => rmSync(root, { recursive: true }));
  const env = { CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'p', CHATOS_CONTEXT_SCOPE_ID: 'a'.repeat(64), CHATOS_PLUGIN_DATA_DIR: root };
  writeFileSync(path.join(root, 'planning.sqlite3'), 'not a database');
  assert.throws(() => new PlanningStore(env));
  const link = path.join(root, 'link'); symlinkSync(root, link, 'dir');
  assert.throws(() => new PlanningStore({ ...env, CHATOS_PLUGIN_DATA_DIR: link }), /symbolic link/);
});
