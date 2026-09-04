import assert from 'node:assert/strict';
import test from 'node:test';
import {
  prepareGenerationPermit,
  verifyGenerationPermit
} from '../dist/generation-guides.test.mjs';

const architectureChecklist = ['single_architecture_viewpoint', 'boundaries_show_ownership', 'primary_path_is_visible', 'implementation_detail_is_excluded', 'independent_concerns_are_split', 'code_evidence_is_mapped'];
const sequenceChecklist = ['single_runtime_scenario', 'participants_have_distinct_roles', 'message_order_is_causal', 'activation_intervals_are_bounded', 'fragments_do_not_hide_content', 'independent_scenarios_are_split'];

test('generation permits are bound to skill contract, kind, artifact, and runtime scope', async () => {
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
    kind: 'architecture',
    mode: 'overview',
    artifactKey: 'system-overview',
    operation: 'create',
    title: 'System Overview',
    plan,
    scopeFingerprint: scopeA
  });
  const permit = verifyGenerationPermit(prepared.generationPermit, {
    scopeFingerprint: scopeA,
    kind: 'architecture',
    artifactKey: 'system-overview',
    title: 'System Overview'
  });
  assert.equal(permit.qualityProfile, 'architecture-overview');
  assert.match(prepared.planHash, /^[a-f0-9]{64}$/);

  assert.throws(() => verifyGenerationPermit(prepared.generationPermit, {
    scopeFingerprint: scopeB,
    kind: 'architecture',
    artifactKey: 'system-overview'
  }), /different ChatOS user or project scope/);
  assert.throws(() => verifyGenerationPermit(prepared.generationPermit, {
    scopeFingerprint: scopeA,
    kind: 'sequence',
    artifactKey: 'system-overview'
  }), /permit is for architecture/);
  assert.throws(() => verifyGenerationPermit(prepared.generationPermit, {
    scopeFingerprint: scopeA,
    kind: 'architecture',
    artifactKey: 'another-artifact'
  }), /artifactKey/);
});

test('generation planning rejects an over-budget plan and incomplete skill checklist', async () => {
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
    kind: 'sequence',
    artifactKey: 'payment-callback',
    operation: 'create',
    title: 'Payment Callback',
    plan: { ...basePlan, estimatedPrimaryItemCount: 9 },
    scopeFingerprint: scope
  }), /exceeding.*budget|exceeds.*budget/i);
  await assert.rejects(() => prepareGenerationPermit({
    kind: 'sequence',
    artifactKey: 'payment-callback',
    operation: 'create',
    title: 'Payment Callback',
    plan: { ...basePlan, checklistAcknowledgements: sequenceChecklist.slice(0, -1) },
    scopeFingerprint: scope
  }), /Checklist acknowledgement mismatch/);
});
