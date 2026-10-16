import assert from 'node:assert/strict';
import test from 'node:test';
import { readyTaskIds, requirementPlanView, visibleDependencyEdges } from '../dist/plan-view.mjs';
import { createSolutionWorkspace, validateWorkspace } from '../dist/schema.mjs';
import { crossRequirementPlan } from './fixtures/cross-requirement-plan.mjs';

test('requirement mainline preserves hidden prerequisites and canonical readiness without mutating data', () => {
  const plan = crossRequirementPlan();
  const original = structuredClone(plan);
  const view = requirementPlanView(plan, 'R-002', 'requirement');
  assert.deepEqual(view.plan.tasks.map((task) => task.id), ['T-001', 'T-003', 'T-005']);
  assert.deepEqual(view.plan.tasks[1].dependsOn, ['T-002']);
  assert.deepEqual(view.plan.tasks[2].dependsOn, ['T-004']);
  assert.equal(view.readyCount, 1);
  assert.deepEqual(readyTaskIds(plan.tasks), ['T-001']);
  assert.deepEqual(view.supportingTasks.map((task) => task.id), ['T-002', 'T-004']);
  assert.deepEqual(plan, original);
});

test('complete scope includes transitive support and hides redundant prerequisite edges by default', () => {
  const plan = crossRequirementPlan();
  const view = requirementPlanView(plan, 'R-002', 'complete');
  assert.equal(view.plan.tasks.length, 5);
  assert.deepEqual(visibleDependencyEdges(view.plan.tasks).map((edge) => edge.id), ['T-001->T-002', 'T-002->T-003', 'T-003->T-004', 'T-004->T-005']);
  assert.deepEqual(visibleDependencyEdges(view.plan.tasks, true).map((edge) => edge.id), ['T-001->T-002', 'T-002->T-003', 'T-001->T-004', 'T-003->T-004', 'T-004->T-005']);
  assert.equal(view.readyCount, 1);
  const workspace = createSolutionWorkspace('回归');
  workspace.executionPlan = plan;
  assert.deepEqual(validateWorkspace(workspace).readyTaskIds, readyTaskIds(plan.tasks));
});

test('completed hidden prerequisites unlock tasks equally in both scopes', () => {
  const plan = crossRequirementPlan();
  plan.tasks[0].status = 'done';
  plan.tasks[1].status = 'done';
  assert.deepEqual(readyTaskIds(plan.tasks), ['T-003']);
  for (const scope of ['requirement', 'complete']) assert.equal(requirementPlanView(plan, 'R-002', scope).readyCount, 1);
});

test('missing and cancelled prerequisites never count as completed', () => {
  const plan = crossRequirementPlan();
  plan.tasks[1].status = 'cancelled';
  plan.tasks[4].dependsOn = ['T-404'];
  const view = requirementPlanView(plan, 'R-002', 'complete');
  assert.deepEqual(view.missingDependencyIds, ['T-404']);
  assert.deepEqual(readyTaskIds(plan.tasks), ['T-001']);
  assert.ok(visibleDependencyEdges(view.plan.tasks).every((edge) => edge.source !== 'T-404'));
});

test('closure terminates on cycles and filters presentation positions only', () => {
  const plan = crossRequirementPlan();
  plan.tasks[0].dependsOn = ['T-005'];
  plan.positions = { 'T-001': { x: 3, y: 4 }, 'T-004': { x: 8, y: 9 } };
  const view = requirementPlanView(plan, 'R-002', 'requirement');
  assert.deepEqual(view.plan.positions, { 'T-001': { x: 3, y: 4 } });
  assert.equal(view.supportingTasks.length, 2);
  assert.equal(view.readyCount, 0);
});

test('supporting ready tasks do not inflate requirement-specific counts', () => {
  const plan = crossRequirementPlan();
  plan.tasks[0].requirementIds = ['R-001'];
  assert.deepEqual(readyTaskIds(plan.tasks), ['T-001']);
  assert.equal(requirementPlanView(plan, 'R-002', 'complete').readyCount, 0);
});

test('rendered edges deduplicate references and omit only hidden endpoints', () => {
  const plan = crossRequirementPlan();
  plan.tasks[1].dependsOn.push('T-001');
  assert.equal(visibleDependencyEdges(plan.tasks).filter((edge) => edge.id === 'T-001->T-002').length, 1);
  assert.deepEqual(visibleDependencyEdges(requirementPlanView(plan, 'R-002', 'requirement').plan.tasks), []);
  assert.deepEqual(requirementPlanView(plan, 'R-unknown', 'complete').plan.tasks, []);
});
