import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  prepareGenerationPermit,
  runtimeDataScopeFingerprint,
  runtimeGenerationScopeFingerprint,
  verifyGenerationPermit
} from '../dist/generation-guides.test.mjs';

const architectureChecklist = ['single_architecture_viewpoint', 'boundaries_show_ownership', 'primary_path_is_visible', 'implementation_detail_is_excluded', 'independent_concerns_are_split', 'code_evidence_is_mapped'];
const sequenceChecklist = ['single_runtime_scenario', 'participants_have_distinct_roles', 'message_order_is_causal', 'activation_intervals_are_bounded', 'fragments_do_not_hide_content', 'independent_scenarios_are_split'];

test('persistent diagram scope is stable across runtime sessions while generation plans are session-bound', () => {
  const names = ['CHATOS_CONTEXT_SCOPE', 'CHATOS_CONTEXT_SCOPE_ID', 'CHATOS_PROJECT_ID', 'CHATOS_WORKSPACE_ID', 'CHATOS_USER_ID', 'CHATOS_ACCOUNT_ID', 'CHATOS_PLUGIN_RUNTIME_SESSION_ID'];
  const original = Object.fromEntries(names.map((name) => [name, process.env[name]]));
  try {
    process.env.CHATOS_CONTEXT_SCOPE = 'project';
    process.env.CHATOS_CONTEXT_SCOPE_ID = 'scope-a';
    process.env.CHATOS_PROJECT_ID = 'project-a';
    process.env.CHATOS_WORKSPACE_ID = 'workspace-a';
    process.env.CHATOS_USER_ID = 'user-a';
    process.env.CHATOS_PLUGIN_RUNTIME_SESSION_ID = 'runtime-a';
    const dataA = runtimeDataScopeFingerprint('/tmp/diagram-data-scope');
    const generationA = runtimeGenerationScopeFingerprint('/tmp/diagram-data-scope');
    process.env.CHATOS_PLUGIN_RUNTIME_SESSION_ID = 'runtime-b';
    const dataB = runtimeDataScopeFingerprint('/tmp/diagram-data-scope');
    const generationB = runtimeGenerationScopeFingerprint('/tmp/diagram-data-scope');
    assert.equal(dataB, dataA);
    assert.notEqual(generationB, generationA);
  } finally {
    for (const name of names) {
      if (original[name] === undefined) delete process.env[name];
      else process.env[name] = original[name];
    }
  }
});

test('generation permits are bound to skill contract, kind, artifact, and runtime scope', async () => {
  const storeDirectory = await mkdtemp(path.join(os.tmpdir(), 'diagram-generation-plan-'));
  try {
  const scopeA = 'a'.repeat(64);
  const scopeB = 'b'.repeat(64);
  const goal = 'Show one bounded system architecture overview';
  const plan = {
    goal,
    scope: 'Major client, entry, domain, and data boundaries only.',
    excludedDetails: ['Controllers, repositories, tables, pods, and unrelated workflows.'],
    estimatedPrimaryItemCount: 8,
    estimatedEdgeCount: 10,
    structure: ['Client', 'Entry', 'Business', 'Data'],
    splitPlan: ['Create detail diagrams for domains that need internal expansion.'],
    splitRationale: 'The overview remains at one abstraction level.',
    checklistAcknowledgements: architectureChecklist
  };
  const prepared = await prepareGenerationPermit({
    storeDirectory,
    kind: 'architecture',
    mode: 'overview',
    artifactKey: 'system-overview',
    operation: 'create',
    title: 'System Overview',
    plan,
    scopeFingerprint: scopeA
  });
  const permit = await verifyGenerationPermit(storeDirectory, {
    scopeFingerprint: scopeA,
    kind: 'architecture',
    artifactKey: 'system-overview',
    title: 'System Overview'
  });
  assert.equal(permit.qualityProfile, 'architecture-overview');
  assert.match(prepared.planHash, /^[a-f0-9]{64}$/);

  await assert.rejects(() => verifyGenerationPermit(storeDirectory, {
    scopeFingerprint: scopeB,
    kind: 'architecture',
    artifactKey: 'system-overview'
  }), /No active generation plan/);
  await assert.rejects(() => verifyGenerationPermit(storeDirectory, {
    scopeFingerprint: scopeA,
    kind: 'sequence',
    artifactKey: 'system-overview'
  }), /permit is for architecture/);
  await assert.rejects(() => verifyGenerationPermit(storeDirectory, {
    scopeFingerprint: scopeA,
    kind: 'architecture',
    artifactKey: 'another-artifact'
  }), /No active generation plan/);
  } finally {
    await rm(storeDirectory, { recursive: true, force: true });
  }
});

test('generation planning rejects an over-budget plan and incomplete skill checklist', async () => {
  const storeDirectory = await mkdtemp(path.join(os.tmpdir(), 'diagram-generation-plan-'));
  try {
  const scope = 'c'.repeat(64);
  const goal = 'Show one payment callback sequence';
  const basePlan = {
    goal,
    scope: 'Callback receipt through final acknowledgement.',
    excludedDetails: ['Checkout and refund scenarios.'],
    estimatedPrimaryItemCount: 4,
    estimatedEdgeCount: 8,
    structure: ['Provider', 'API', 'Payment Service', 'Store'],
    splitPlan: ['Refund handling remains a separate sequence.'],
    splitRationale: 'Only the callback transaction belongs in this diagram.',
    checklistAcknowledgements: sequenceChecklist
  };
  await assert.rejects(() => prepareGenerationPermit({
    storeDirectory,
    kind: 'sequence',
    artifactKey: 'payment-callback',
    operation: 'create',
    title: 'Payment Callback',
    plan: { ...basePlan, estimatedPrimaryItemCount: 9 },
    scopeFingerprint: scope
  }), /exceeding.*budget|exceeds.*budget/i);
  await assert.rejects(() => prepareGenerationPermit({
    storeDirectory,
    kind: 'sequence',
    artifactKey: 'payment-callback',
    operation: 'create',
    title: 'Payment Callback',
    plan: { ...basePlan, checklistAcknowledgements: sequenceChecklist.slice(0, -1) },
    scopeFingerprint: scope
  }), /Checklist acknowledgement mismatch/);
  } finally {
    await rm(storeDirectory, { recursive: true, force: true });
  }
});
