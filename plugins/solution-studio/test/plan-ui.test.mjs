import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ReactFlowProvider } from '@xyflow/react';
import { createSolutionWorkspace } from '../dist/schema.mjs';
import { crossRequirementPlan } from './fixtures/cross-requirement-plan.mjs';

// Compile the real UI component in memory, without starting any desktop app.
const built = await build({
  entryPoints: ['ui-src/studio/SolutionStudioApp.tsx'], bundle: true, platform: 'node', format: 'esm', write: false, jsx: 'automatic',
  plugins: [{ name: 'shared-test-packages', setup(builder) {
    builder.onResolve({ filter: /^[^./]/ }, ({ path: specifier }) => ({ path: import.meta.resolve(specifier), external: true }));
  } }]
});
const { RequirementSheet } = await import(`data:text/javascript;base64,${Buffer.from(built.outputFiles[0].text).toString('base64')}`);

function markup(selectedTaskId) {
  const workspace = createSolutionWorkspace('隔离展示回归');
  workspace.executionPlan = crossRequirementPlan();
  workspace.requirements.items = ['R-001', 'R-002', 'R-003'].map((id) => ({ id, title: `需求 ${id}`, description: '', priority: 'must', acceptanceCriteria: [], evidenceIds: [], selectedDesignSectionId: 'D-002' }));
  workspace.design.sections = [{ id: 'D-002', title: '审批设计', body: '', requirementIds: ['R-002'], evidenceIds: [], blocks: [] }];
  const noop = () => {};
  return renderToStaticMarkup(React.createElement(ReactFlowProvider, null, React.createElement(RequirementSheet, {
    workspace, requirement: workspace.requirements.items[1], tab: 'plan', selectedTaskId,
    onTab: noop, onSelectTask: noop, onOpenRequirement: noop, onClose: noop, onAddDesign: noop,
    onAddDesignBlock: noop, onAddTask: noop, onUpdateDesign: noop, onUpdateTask: noop, onPositionsChange: noop
  })));
}

test('real requirement sheet defaults to the current requirement mainline and keeps complete dependencies opt-in', () => {
  const html = markup();
  assert.ok(html.includes('class="active">本需求主线 · 3</button>'));
  assert.ok(!html.includes('class="active">完整依赖 · 5</button>'));
  assert.ok(html.includes('<details class="supporting-task-links"><summary>跨需求前置 · 2（按需展开）</summary>'));
  assert.ok(!html.includes('<details class="supporting-task-links" open=""'));
  const counts = html.match(/本需求\s*1\s*个任务具备开始条件/g) ?? [];
  assert.equal(counts.length, 2, (html.match(/本需求[^<]{0,90}/g) ?? []).join('\n'));
  assert.ok(html.includes('T-004 · T-004</button>'));
});

test('cross-requirement task selection exposes its owner, prerequisites and output read-only', () => {
  const html = markup('T-004');
  assert.ok(html.includes('跨需求支撑任务'));
  assert.ok(html.includes('R-003 · 需求 R-003'));
  assert.ok(html.includes('T-003 · T-003 · 未开始'));
  assert.ok(html.includes('预期输出'));
  assert.ok(html.includes('验收标准'));
  assert.ok(html.includes('只读查看；进入所属需求后可编辑'));
});

test('primary task editor includes prerequisite choices from the entire workspace', () => {
  const html = markup('T-003');
  assert.ok(html.includes('<b>T-002</b>T-002'));
  assert.ok(html.includes('<b>T-004</b>T-004'));
});
