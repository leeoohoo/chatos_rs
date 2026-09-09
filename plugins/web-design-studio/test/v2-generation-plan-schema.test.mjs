import assert from 'node:assert/strict';
import test from 'node:test';
import { assertGenerationPlan } from '../dist/v2-generation-plan-schema.test.mjs';
import { generationPlanFixture } from './helpers/v2-generation-plan-fixture.mjs';

test('generation plan models visual design work before optional interaction work', () => {
  const plan = generationPlanFixture();
  assert.equal(plan.status, 'draft');
  assert.equal(plan.mode, 'auto-current-page');
  assert.equal(plan.pageRuns.length, 2);
  assert.equal(plan.pageRuns[0].steps.find((step) => step.kind === 'interaction').required, false);
  assert.deepEqual(plan.sitePlan.pages.map((page) => page.pageId), ['home', 'pricing']);
  assert.doesNotThrow(() => assertGenerationPlan(plan));
});

test('generation plan rejects interaction that can run before the design gate', () => {
  const plan = generationPlanFixture();
  plan.pageRuns[0].steps.find((step) => step.kind === 'interaction').dependsOn = ['home-visual'];
  assert.throws(() => assertGenerationPlan(plan), /interaction steps must depend on the design gate/);
});

test('generation plan requires handoff to cover every required design step', () => {
  const plan = generationPlanFixture();
  plan.pageRuns[0].steps.find((step) => step.kind === 'handoff').dependsOn = ['home-interaction'];
  plan.pageRuns[0].steps.find((step) => step.kind === 'interaction').dependsOn = [];
  assert.throws(() => assertGenerationPlan(plan), /interaction steps must depend on the design gate|handoff must depend/);
});

test('generation plan rejects forged or incomplete visual design intent', () => {
  const plan = generationPlanFixture();
  plan.pageRuns[0].design.designAcceptanceCriteria = ['Looks fine'];
  assert.throws(() => assertGenerationPlan(plan), /designAcceptanceCriteria needs at least 2/);
});
