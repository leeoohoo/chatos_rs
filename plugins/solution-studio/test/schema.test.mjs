import assert from 'node:assert/strict';
import test from 'node:test';
import { createSolutionWorkspace, validateWorkspace, workspaceToMarkdown } from '../dist/schema.mjs';

function completeWorkspace() {
  const workspace = createSolutionWorkspace('登录改造', 'existing-project', 'login-redesign');
  const now = new Date().toISOString();
  workspace.revision = 1;
  workspace.projectProfile = { background: '现有登录流程需要提升稳定性。', overview: '改造会话刷新与失败回退。', projectType: '软件开发', deliveryForm: 'Web 应用', targetPlatforms: ['Web'] };
  workspace.requirements = {
    status: 'approved', revision: 1, summary: '改进登录可靠性', goals: ['稳定登录'], users: ['注册用户'], inScope: ['会话刷新'], outOfScope: ['第三方登录'], constraints: [], assumptions: [], openQuestions: [],
    evidence: [{ id: 'E-001', label: '认证入口', source: 'src/auth.ts', confidence: 'verified' }],
    items: [{ id: 'R-001', title: '刷新会话', description: '用户会话过期前自动刷新。', priority: 'must', acceptanceCriteria: ['有效会话不中断'], evidenceIds: ['E-001'], selectedDesignSectionId: 'D-001' }], updatedAt: now
  };
  workspace.design = {
    status: 'approved', revision: 1, basedOnRequirementsRevision: 1, summary: '集中刷新流程',
    blocks: [
      { id: 'D-000-B-001', type: 'text', title: '技术基线', content: 'TypeScript 与 Web 运行时。' },
      { id: 'D-000-B-002', type: 'architecture', title: '总体架构', content: '<svg viewBox="0 0 400 200"><text x="20" y="30">Overall</text></svg>' }
    ],
    sections: [{ id: 'D-001', title: '刷新协调器', body: '统一刷新并合并并发请求。', requirementIds: ['R-001'], evidenceIds: ['E-001'], blocks: [{ id: 'D-001-B-001', type: 'text', title: '详细设计', content: '协调器接口与失败处理。' }, { id: 'D-001-B-002', type: 'flowchart', title: '刷新流程', content: '<svg viewBox="0 0 400 200"><text x="20" y="30">Refresh flow</text></svg>' }] }],
    decisions: [], risks: ['刷新失败需要退出'], validationStrategy: ['并发刷新测试'], updatedAt: now
  };
  workspace.executionPlan = {
    status: 'approved', revision: 1, basedOnDesignRevision: 1, objective: '交付可验证的刷新流程',
    tasks: [
      { id: 'T-001', title: '实现协调器', description: '实现并发合并。', type: 'task', phase: '实现', dependsOn: [], status: 'done', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: ['刷新协调器'], acceptanceCriteria: ['并发请求只刷新一次'], sourceReferences: ['src/auth.ts'] },
      { id: 'T-002', title: '补充集成测试', description: '覆盖成功和失败路径。', type: 'task', phase: '验证', dependsOn: ['T-001'], status: 'planned', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: ['集成测试'], acceptanceCriteria: ['成功和失败用例通过'], sourceReferences: [] }
    ], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: now
  };
  return workspace;
}

test('validates traceability, topological order, and ready tasks', () => {
  const workspace = completeWorkspace();
  const result = validateWorkspace(workspace);
  assert.equal(result.valid, true);
  assert.equal(result.ready, true);
  assert.deepEqual(result.topologicalOrder, ['T-001', 'T-002']);
  assert.deepEqual(result.readyTaskIds, ['T-002']);
  assert.deepEqual(result.traceability, { requirementCount: 1, requirementsWithDesign: 1, requirementsWithTasks: 1 });
});

test('detects dependency cycles without treating canvas order as execution order', () => {
  const workspace = completeWorkspace();
  workspace.executionPlan.tasks[0].dependsOn = ['T-002'];
  workspace.executionPlan.positions = { 'T-001': { x: 900, y: 20 }, 'T-002': { x: 10, y: 20 } };
  const result = validateWorkspace(workspace);
  assert.equal(result.valid, false);
  assert.ok(result.issues.some((issue) => issue.code === 'dependency_cycle'));
  assert.deepEqual(result.topologicalOrder, []);
});

test('flags stale revisions and incomplete requirement coverage', () => {
  const workspace = completeWorkspace();
  workspace.requirements.revision = 2;
  workspace.executionPlan.tasks[0].requirementIds = [];
  workspace.executionPlan.tasks[1].requirementIds = [];
  const result = validateWorkspace(workspace);
  assert.ok(result.issues.some((issue) => issue.code === 'stale_design'));
  assert.ok(result.issues.some((issue) => issue.code === 'requirement_without_task'));
});

test('exports requirements, design, and dependency details to Markdown', () => {
  const workspace = completeWorkspace();
  workspace.hostProject = { projectId: 'project-export-123', projectName: '登录项目', connectorWorkspaceId: 'workspace-export-456' };
  workspace.design.sections[0].blocks = [
    { id: 'D-001-B-001', type: 'text', title: '方案说明', content: '使用统一协调器。' },
    { id: 'D-001-B-002', type: 'architecture', title: '组件架构', content: '<svg viewBox="0 0 400 200"><text x="20" y="30">Coordinator</text></svg>' }
  ];
  const markdown = workspaceToMarkdown(workspace);
  assert.match(markdown, /## 项目总需求/);
  assert.match(markdown, /## ChatOS 项目绑定/);
  assert.match(markdown, /项目 ID：project-export-123/);
  assert.match(markdown, /## 项目总体设计/);
  assert.match(markdown, /## 需求详细设计/);
  assert.match(markdown, /组件架构（架构图）/);
  assert.match(markdown, /```svg/);
  assert.match(markdown, /## 执行计划/);
  assert.match(markdown, /前置：T-001/);
});

test('requires standalone SVG for visual design blocks', () => {
  const workspace = completeWorkspace();
  workspace.design.sections[0].blocks = [{ id: 'D-001-B-001', type: 'flowchart', title: '刷新流程', content: 'not an svg' }];
  assert.throws(() => validateWorkspace(workspace), /must contain standalone SVG code/);
});

test('rejects empty SVG shells and SVGs without a viewBox', () => {
  const empty = completeWorkspace();
  empty.design.blocks[1].content = '<svg viewBox="0 0 400 200"></svg>';
  assert.throws(() => validateWorkspace(empty), /does not contain visible diagram content/);

  const noViewBox = completeWorkspace();
  noViewBox.design.blocks[1].content = '<svg><rect width="100" height="100"/></svg>';
  assert.throws(() => validateWorkspace(noViewBox), /must declare a viewBox/);
});

export { completeWorkspace };
