import assert from 'node:assert/strict';
import test from 'node:test';
import { transitionGenerationPlan } from '../dist/v2-generation-state-machine.test.mjs';
import { generationPlanFixture } from './helpers/v2-generation-plan-fixture.mjs';

const timestamp = '2026-09-08T09:00:00.000Z';

function transition(plan, action) {
  return transitionGenerationPlan(plan, action, timestamp).plan;
}

function visualArtifacts(stepId, revision) {
  return [
    { artifactId: `${stepId}-snapshot-r${revision}`, kind: 'page-snapshot', revision, viewportWidth: 1440, uri: `artifact://${stepId}/snapshot`, createdAt: timestamp },
    { artifactId: `${stepId}-quality-r${revision}`, kind: 'quality-report', revision, uri: `artifact://${stepId}/quality`, createdAt: timestamp }
  ];
}

function acceptCurrentStep(plan, pageId, stepId, baseRevision) {
  const attemptId = `${stepId}-attempt-${baseRevision}`;
  plan = transition(plan, { type: 'start-step', pageId, stepId, attemptId, idempotencyKey: `${stepId}:base:${baseRevision}`, baseRevision });
  plan = transition(plan, { type: 'begin-validation', pageId, stepId, attemptId });
  plan = transition(plan, { type: 'await-review', pageId, stepId, attemptId, artifacts: visualArtifacts(stepId, baseRevision) });
  return transition(plan, { type: 'accept-step', pageId, stepId, attemptId, committedRevision: baseRevision + 1 });
}

test('state machine advances one bounded step and never starts another page automatically', () => {
  let plan = generationPlanFixture();
  plan = transition(plan, { type: 'mark-ready' });
  plan = transition(plan, { type: 'start-page', pageId: 'home' });
  assert.equal(plan.activePageId, 'home');
  assert.equal(plan.pageRuns.find((page) => page.pageId === 'home').steps[0].status, 'ready');
  assert.throws(() => transition(plan, { type: 'start-page', pageId: 'pricing' }), /not ready|already active/);

  plan = acceptCurrentStep(plan, 'home', 'home-structure', 1);
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-visual').status, 'ready');
  assert.equal(plan.pageRuns[0].activeStepId, undefined);
  assert.equal(plan.pageRuns[1].status, 'planned');
});

test('state machine requires rendered visual evidence before review', () => {
  let plan = transition(generationPlanFixture(), { type: 'mark-ready' });
  plan = transition(plan, { type: 'start-page', pageId: 'home' });
  plan = transition(plan, { type: 'start-step', pageId: 'home', stepId: 'home-structure', attemptId: 'attempt-no-image', idempotencyKey: 'key-no-image', baseRevision: 1 });
  plan = transition(plan, { type: 'begin-validation', pageId: 'home', stepId: 'home-structure', attemptId: 'attempt-no-image' });
  assert.throws(
    () => transition(plan, {
      type: 'await-review', pageId: 'home', stepId: 'home-structure', attemptId: 'attempt-no-image',
      artifacts: [{ artifactId: 'quality-only', kind: 'quality-report', revision: 1, createdAt: timestamp }]
    }),
    /rendered page snapshot or region crop/
  );
});

test('state machine enforces one active step and idempotent attempt start', () => {
  let plan = transition(generationPlanFixture(), { type: 'mark-ready' });
  plan = transition(plan, { type: 'start-page', pageId: 'home' });
  const action = { type: 'start-step', pageId: 'home', stepId: 'home-structure', attemptId: 'attempt-idempotent', idempotencyKey: 'home-structure:1', baseRevision: 1 };
  const started = transitionGenerationPlan(plan, action, timestamp);
  assert.equal(started.changed, true);
  const replay = transitionGenerationPlan(started.plan, action, timestamp);
  assert.equal(replay.changed, false);
  assert.equal(replay.replayed, true);
  assert.equal(replay.plan.pageRuns[0].steps[0].attempts.length, 1);
  assert.throws(
    () => transition(started.plan, { type: 'start-step', pageId: 'home', stepId: 'home-visual', attemptId: 'other-attempt', idempotencyKey: 'other-key', baseRevision: 1 }),
    /already active|cannot start/
  );
});

test('a completed page stops at its boundary until the next page is explicitly started', () => {
  let plan = transition(generationPlanFixture(), { type: 'mark-ready' });
  plan = transition(plan, { type: 'start-page', pageId: 'home' });
  plan = acceptCurrentStep(plan, 'home', 'home-structure', 1);
  plan = acceptCurrentStep(plan, 'home', 'home-visual', 2);
  plan = acceptCurrentStep(plan, 'home', 'home-design-gate', 3);
  plan = transition(plan, { type: 'skip-step', pageId: 'home', stepId: 'home-interaction' });
  plan = acceptCurrentStep(plan, 'home', 'home-handoff', 4);
  plan = transition(plan, { type: 'complete-page', pageId: 'home' });

  assert.equal(plan.status, 'ready');
  assert.equal(plan.activePageId, undefined);
  assert.equal(plan.pageRuns[0].status, 'completed');
  assert.equal(plan.pageRuns[1].status, 'planned');
  assert.equal(plan.pageRuns[1].steps.every((step) => step.status === 'planned'), true);
});

test('retryable failure keeps accepted work and retries only the failed step', () => {
  let plan = transition(generationPlanFixture(), { type: 'mark-ready' });
  plan = transition(plan, { type: 'start-page', pageId: 'home' });
  plan = acceptCurrentStep(plan, 'home', 'home-structure', 1);
  plan = transition(plan, { type: 'start-step', pageId: 'home', stepId: 'home-visual', attemptId: 'visual-attempt-1', idempotencyKey: 'visual-key-1', baseRevision: 2 });
  plan = transition(plan, {
    type: 'fail-step', pageId: 'home', stepId: 'home-visual', attemptId: 'visual-attempt-1',
    error: { code: 'quality_reject', message: 'Visual hierarchy is too flat.', retryable: true, issueIds: ['hierarchy-flat'] }
  });
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-structure').status, 'accepted');
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-visual').status, 'retryable');
  assert.equal(plan.status, 'running');
  plan = transition(plan, { type: 'start-step', pageId: 'home', stepId: 'home-visual', attemptId: 'visual-attempt-2', idempotencyKey: 'visual-key-2', baseRevision: 2 });
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-visual').attempts.length, 2);
});

test('rolling back accepted work marks its dependent design work stale', () => {
  let plan = transition(generationPlanFixture(), { type: 'mark-ready' });
  plan = transition(plan, { type: 'start-page', pageId: 'home' });
  plan = acceptCurrentStep(plan, 'home', 'home-structure', 1);
  plan = acceptCurrentStep(plan, 'home', 'home-visual', 2);
  plan = transition(plan, { type: 'rollback-step', pageId: 'home', stepId: 'home-structure' });
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-structure').status, 'rolled-back');
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-visual').status, 'stale');
  assert.equal(plan.pageRuns[0].steps.find((step) => step.stepId === 'home-design-gate').status, 'stale');
});
