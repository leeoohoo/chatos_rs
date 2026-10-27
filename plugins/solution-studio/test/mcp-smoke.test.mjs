import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { SolutionWorkspaceStore } from '../dist/store.mjs';

const now = () => new Date().toISOString();
const packageRoot = process.env.CHATOS_PLUGIN_TEST_PACKAGE_ROOT || path.resolve(import.meta.dirname, '..');

test('MCP exposes gated planning tools and rejects an invalid dependency graph before persistence', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-mcp-'));
  const projectRoot = path.join(root, 'host-project');
  await mkdir(projectRoot);
  const client = new Client({ name: 'solution-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.join(packageRoot, 'bin/chatos-solution-studio'), 'mcp-authoring'],
    env: { ...process.env, CHATOS_PLUGIN_DATA_DIR: path.join(root, 'data'), CHATOS_PLUGIN_ARTIFACT_DIR: path.join(root, 'exports'), CHATOS_CONTEXT_SCOPE: 'project', CHATOS_CONTEXT_SCOPE_ID: 'scope-mcp-123', CHATOS_PROJECT_ID: 'project-mcp-123', CHATOS_PROJECT_NAME: '示例项目', CHATOS_WORKSPACE_ID: 'workspace-mcp-456', CHATOS_WORKSPACE: projectRoot }
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    const tools = new Map(listed.tools.map((tool) => [tool.name, tool]));
    for (const name of ['solution_get_active_context', 'solution_upsert_requirements', 'solution_upsert_design', 'solution_upsert_design_block', 'solution_upsert_execution_plan', 'solution_append_execution_tasks', 'solution_validate', 'solution_export', 'solution_finalize']) assert.ok(tools.has(name));
    assert.deepEqual(tools.get('solution_upsert_execution_plan')._meta['chatos/skillGate'].allOf, ['solution-studio', 'solution-execution-plan']);
    assert.deepEqual(tools.get('solution_append_execution_tasks')._meta['chatos/skillGate'].allOf, ['solution-studio', 'solution-execution-plan']);
    const planSchema = tools.get('solution_upsert_execution_plan').inputSchema.properties.executionPlan;
    assert.deepEqual(planSchema.required, ['status', 'basedOnDesignRevision', 'objective', 'tasks']);
    assert.ok(planSchema.properties.tasks.items.required.includes('dependsOn'));
    assert.equal(planSchema.properties.statusEvents.maxItems, 0);

    const context = await call(client, 'solution_get_active_context', {});
    assert.equal(context.scope.hasProjectContext, true);
    assert.equal(context.scope.projectId, 'project-mcp-123');
    assert.equal(context.scope.projectName, '示例项目');
    assert.equal(context.scope.connectorWorkspaceId, 'workspace-mcp-456');
    assert.equal(context.scope.projectRoot, projectRoot);
    assert.equal(context.workspace, null);

    const requirementResult = await call(client, 'solution_upsert_requirements', {
      title: 'MCP 方案',
      description: '验证 Solution Studio 完整交付链路。',
      projectProfile: { background: '现有项目需要可追踪规划。', overview: '输出需求、设计与执行计划。', projectType: '软件开发', deliveryForm: 'Web 应用', targetPlatforms: ['Web'] },
      requirements: { status: 'approved', revision: 0, summary: '需求', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], modules: [{ id: 'MOD-CORE', title: '核心业务', kind: 'business', description: '核心能力', order: 0 }], items: ['R-001', 'R-002'].map(id => ({ id, moduleId: 'MOD-CORE', kind: 'capability', title: `能力 ${id}`, description: '提供能力。', priority: 'must', acceptanceCriteria: ['可以验证'], evidenceIds: [], selectedDesignSectionId: 'D-001' })), updatedAt: now() }
    });
    assert.equal(requirementResult.workspace.hostProjectId, 'project-mcp-123');
    assert.equal(requirementResult.validation.valid, false);
    assert.ok(requirementResult.validation.issues.some(issue => issue.code === 'design_empty'));
    assert.ok(requirementResult.validation.issues.some(issue => issue.code === 'plan_empty'));
    assert.equal(requirementResult.continuation.kind, 'authoring-work-remaining');
    assert.deepEqual(requirementResult.continuation.suggestedActions.map(action => action.tool),
      ['solution_upsert_design', 'solution_upsert_execution_plan', 'solution_finalize']);
    await call(client, 'solution_upsert_design', {
      title: 'MCP 方案', sourceMode: 'existing-project',
      design: { status: 'approved', revision: 0, basedOnRequirementsRevision: 1, summary: '方案', blocks: [{ id: 'D-000-B-001', type: 'text', title: '技术基线', content: 'TypeScript' }, { id: 'D-000-B-002', type: 'architecture', title: '总体架构', content: '<svg viewBox="0 0 100 100"><text x="5" y="10">Architecture</text></svg>' }], sections: [{ id: 'D-001', moduleId: 'MOD-CORE', title: '核心设计', body: '设计内容', requirementIds: ['R-001', 'R-002'], evidenceIds: [], blocks: [{ id: 'D-001-B-001', type: 'text', title: '详细设计', content: '模块与接口。' }, { id: 'D-001-B-002', type: 'flowchart', title: '核心流程', content: '<svg viewBox="0 0 100 100"><text x="5" y="10">Flow</text></svg>' }] }], decisions: [], risks: [], validationStrategy: [], updatedAt: now() }
    });
    const afterDesign = await call(client, 'solution_get_active_context', {});
    assert.equal(afterDesign.workspace.workspaceId, requirementResult.workspace.workspaceId);

    const invalidPlan = {
      status: 'approved', revision: 0, basedOnDesignRevision: 1, objective: '执行', statusEvents: [], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: now(),
      tasks: [
        { id: 'T-001', moduleId: 'MOD-CORE', title: 'A', description: '', type: 'task', phase: '实现', dependsOn: ['T-002'], status: 'planned', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: [], acceptanceCriteria: ['A 完成'], sourceReferences: [] },
        { id: 'T-002', moduleId: 'MOD-CORE', title: 'B', description: '', type: 'task', phase: '实现', dependsOn: ['T-001'], status: 'planned', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: [], acceptanceCriteria: ['B 完成'], sourceReferences: [] }
      ]
    };
    const rejected = await client.callTool({ name: 'solution_upsert_execution_plan', arguments: { title: 'MCP 方案', executionPlan: invalidPlan } });
    assert.equal(rejected.isError, true);
    assert.match(rejected.structuredContent.error, /循环依赖/);
    const afterReject = await call(client, 'solution_get_workspace', {});
    assert.equal(afterReject.workspace.executionPlan.tasks.length, 0);

    const task = (id, requirementId, dependsOn) => ({ id, moduleId: 'MOD-CORE', title: `任务 ${id}`, description: '仅用于隔离验收。', type: 'task', phase: '实现', dependsOn, status: 'planned', requirementIds: [requirementId], designSectionIds: ['D-001'], deliverables: [`${id} 交付物`], acceptanceCriteria: [`${id} 可验证`], sourceReferences: [] });
    const validPlan = { ...invalidPlan, tasks: [
      task('T-001', 'R-001', []),
      task('T-002', 'R-001', ['T-001']),
      task('T-003', 'R-002', ['T-001']),
      task('T-004', 'R-002', ['T-002', 'T-003'])
    ] };
    // Reproduce the production request: business content without server metadata/history/layout.
    const { revision: _revision, updatedAt: _updatedAt, statusEvents: _events, positions: _positions, viewport: _viewport, ...businessPlan } = validPlan;
    for (const [executionPlan, expectedError] of [
      [null, /executionPlan must be an object/],
      [{ ...businessPlan, tasks: null }, /executionPlan.tasks/],
      [{ ...businessPlan, tasks: [null] }, /executionPlan.tasks\[0\]/],
      [{ ...businessPlan, tasks: [{ ...businessPlan.tasks[0], dependsOn: undefined }] }, /task.T-001.dependsOn/],
      [{ ...businessPlan, statusEvents: null }, /executionPlan.statusEvents/],
      [{ ...businessPlan, positions: { 'T-001': null } }, /executionPlan.positions.T-001/],
      [{ ...businessPlan, positions: [] }, /executionPlan.positions/],
      [{ ...businessPlan, viewport: null }, /executionPlan.viewport/],
      [{ ...businessPlan, statusEvents: [{ id: 'EV-1', taskId: 'T-001', from: 'planned', to: 'in_progress', occurredAt: now(), runtimeSessionId: 'test-session', evidence: [] }] }, /不能提交任务状态事件/],
      [{ ...businessPlan, tasks: [{ ...businessPlan.tasks[0], status: 'done' }] }, /状态必须是 planned/]
    ]) {
      const invalid = await client.callTool({ name: 'solution_upsert_execution_plan', arguments: { title: 'MCP 方案', executionPlan } });
      assert.equal(invalid.isError, true);
      assert.match(invalid.structuredContent.error, expectedError);
      const unchanged = await call(client, 'solution_get_workspace', {});
      assert.deepEqual(unchanged.workspace, afterReject.workspace, 'Invalid input must not change the persisted workspace');
    }
    const accepted = await call(client, 'solution_upsert_execution_plan', { title: 'MCP 方案', executionPlan: businessPlan });
    assert.deepEqual(accepted.executionPlan.statusEvents, []);
    assert.deepEqual(accepted.executionPlan.positions, {});
    assert.deepEqual(accepted.executionPlan.viewport, { x: 0, y: 0, zoom: 1 });
    assert.equal(accepted.executionPlan.revision, 1);
    assert.ok(Number.isFinite(Date.parse(accepted.executionPlan.updatedAt)));
    assert.equal(accepted.validation.valid, true, JSON.stringify(accepted.validation));
    assert.deepEqual(accepted.validation.topologicalOrder, ['T-001', 'T-002', 'T-003', 'T-004']);
    const persisted = await call(client, 'solution_get_workspace', {});
    assert.deepEqual(persisted.workspace.executionPlan.tasks.map(({ id, dependsOn, requirementIds, deliverables, acceptanceCriteria }) => ({ id, dependsOn, requirementIds, deliverables, acceptanceCriteria })), validPlan.tasks.map(({ id, dependsOn, requirementIds, deliverables, acceptanceCriteria }) => ({ id, dependsOn, requirementIds, deliverables, acceptanceCriteria })));
    assert.equal(persisted.workspace.executionPlan.tasks.length, 4);
    assert.deepEqual(persisted.workspace.executionPlan.tasks[3].dependsOn, ['T-002', 'T-003']);
    assert.deepEqual(persisted.workspace.executionPlan.tasks[3].requirementIds, ['R-002']);
    assert.equal(accepted.completionRequirement.verifier, 'solution_finalize');

    const incompleteFinalization = await client.callTool({
      name: 'solution_finalize',
      arguments: { workspaceId: accepted.workspace.workspaceId, expectedRevision: accepted.workspace.revision, scope: 'full', expectedDesignBlocks: [{ id: 'D-001-B-999', type: 'ui-svg' }] }
    });
    assert.equal(incompleteFinalization.isError, true);
    assert.match(incompleteFinalization.structuredContent.error, /未找到设计块 D-001-B-999/);

    const finalizedResponse = await client.callTool({ name: 'solution_finalize', arguments: {
      workspaceId: accepted.workspace.workspaceId,
      expectedRevision: accepted.workspace.revision,
      scope: 'full',
      expectedDesignBlocks: [{ id: 'D-000-B-002', type: 'architecture' }, { id: 'D-001-B-002', type: 'flowchart' }]
    } });
    assert.equal(finalizedResponse.isError, false);
    const finalized = finalizedResponse.structuredContent;
    assert.equal(finalized.completionProof.id, accepted.completionRequirement.id);
    assert.equal(finalized.completionProof.verifier, 'solution_finalize');
    assert.equal(finalized.continuation, undefined, 'Final delivery must not request another authoring iteration');
    assert.deepEqual(finalized.verifiedDesignBlocks, [{ id: 'D-000-B-002', type: 'architecture' }, { id: 'D-001-B-002', type: 'flowchart' }]);
    assert.deepEqual(finalizedResponse._meta['chatos/artifacts'], [{
      producer_artifact_id: `solution_${finalized.sha256}`,
      relative_path: finalized.relativePath,
      display_name: finalized.relativePath,
      media_type: 'text/markdown',
      size_bytes: finalized.size,
      sha256: finalized.sha256
    }]);

    const revisedRequirements = await call(client, 'solution_upsert_requirements', {
      title: '仅需求与计划复测',
      description: '修改需求后只交付 requirements + implementation plan。',
      projectProfile: { background: '现有项目需要可追踪规划。', overview: '本轮仅修订需求与执行计划。', projectType: '软件开发', deliveryForm: 'Web 应用', targetPlatforms: ['Web'] },
      requirements: {
        status: 'draft', revision: 0, summary: '修订后需求', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [],
        modules: [{ id: 'MOD-CORE', title: '核心业务', kind: 'business', description: '核心能力', order: 0 }],
        items: ['R-001', 'R-002'].map(id => ({ id, moduleId: 'MOD-CORE', kind: 'capability', title: `能力 ${id}`, description: '修订后需求。', priority: 'must', acceptanceCriteria: ['可以验证'], evidenceIds: [] })),
        updatedAt: now()
      }
    });
    assert.ok(revisedRequirements.validation.issues.some(issue => issue.code === 'stale_design'));
    assert.ok(revisedRequirements.validation.issues.some(issue => issue.code === 'design_link_mismatch'));

    const revisedPlan = await call(client, 'solution_upsert_execution_plan', {
      title: '仅需求与计划复测',
      executionPlan: { ...validPlan, status: 'draft', basedOnDesignRevision: 1, updatedAt: now() }
    });
    const planOnlyFinalized = await client.callTool({ name: 'solution_finalize', arguments: {
      workspaceId: revisedPlan.workspace.workspaceId,
      expectedRevision: revisedPlan.workspace.revision,
      scope: 'plan',
      expectedDesignBlocks: []
    } });
    assert.equal(planOnlyFinalized.isError, false,
      `Plan-scope finalization must ignore stale, out-of-scope design state: ${planOnlyFinalized.structuredContent?.error ?? ''}`);
    assert.equal(planOnlyFinalized.structuredContent.scope, 'plan');
    assert.equal(planOnlyFinalized.structuredContent.completionProof.revision, revisedPlan.workspace.revision);
    assert.deepEqual(await readdir(projectRoot), []);
  } finally {
    await client.close();
    await rm(root, { recursive: true, force: true });
  }
});

test('executor exposes only task execution tools and enforces prerequisites and transitions', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-executor-'));
  const dataRoot = path.join(root, 'data');
  const store = new SolutionWorkspaceStore(dataRoot);
  const nowValue = now();
  const created = await store.create('执行器方案', 'existing-project');
  const workspace = structuredClone(created);
  workspace.projectProfile = { background: '验证执行状态。', overview: '两项有依赖的任务。', projectType: '运营流程', deliveryForm: '执行清单', targetPlatforms: ['本地'] };
  workspace.requirements = { status: 'approved', revision: 1, summary: '执行两项任务', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], modules: [{ id: 'MOD-OPS', title: '执行模块', kind: 'business', description: '', order: 0 }], items: [{ id: 'R-001', moduleId: 'MOD-OPS', kind: 'capability', title: '顺序执行', description: '按依赖完成。', priority: 'must', acceptanceCriteria: ['两项任务完成'], evidenceIds: [], selectedDesignSectionId: 'D-001' }], updatedAt: nowValue };
  workspace.design = { status: 'approved', revision: 1, basedOnRequirementsRevision: 1, summary: '顺序执行设计', blocks: [{ id: 'D-000-B-001', type: 'text', title: '总体设计', content: '按任务前置执行。' }], sections: [{ id: 'D-001', moduleId: 'MOD-OPS', title: '执行顺序', body: '先一后二。', requirementIds: ['R-001'], evidenceIds: [], blocks: [{ id: 'D-001-B-001', type: 'text', title: '详细设计', content: 'T-002 依赖 T-001。' }] }], decisions: [], risks: [], validationStrategy: [], updatedAt: nowValue };
  const task = (id, dependsOn) => ({ id, moduleId: 'MOD-OPS', title: id, description: '执行任务', type: 'task', phase: '执行', dependsOn, status: 'planned', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: [id], acceptanceCriteria: [`${id} 完成`], sourceReferences: [] });
  workspace.executionPlan = { status: 'approved', revision: 1, basedOnDesignRevision: 1, objective: '顺序完成', tasks: [task('T-001', []), task('T-002', ['T-001'])], statusEvents: [], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: nowValue };
  const saved = await store.replace(workspace, created.revision);

  const client = new Client({ name: 'solution-executor-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.join(packageRoot, 'bin/chatos-solution-studio'), 'mcp-executor'],
    env: { ...process.env, CHATOS_PLUGIN_DATA_DIR: dataRoot, CHATOS_PLUGIN_RUNTIME_SESSION_ID: 'executor-session-1' }
  });
  try {
    await client.connect(transport);
    const names = (await client.listTools()).tools.map((tool) => tool.name);
    assert.ok(names.includes('solution_claim_task'));
    assert.ok(names.includes('solution_complete_task'));
    assert.equal(names.some((name) => name.includes('upsert') || name === 'solution_finalize' || name === 'solution_export'), false);

    const blockedClaim = await client.callTool({ name: 'solution_claim_task', arguments: { workspaceId: saved.workspaceId, taskId: 'T-002', expectedRevision: saved.revision } });
    assert.equal(blockedClaim.isError, true);
    assert.match(blockedClaim.structuredContent.error, /前置任务尚未全部完成/);

    const claimed = await call(client, 'solution_claim_task', { workspaceId: saved.workspaceId, taskId: 'T-001', expectedRevision: saved.revision });
    assert.equal(claimed.task.status, 'in_progress');
    const completed = await call(client, 'solution_complete_task', { workspaceId: saved.workspaceId, taskId: 'T-001', expectedRevision: claimed.workspace.revision, summary: '验收通过', evidence: ['test:T-001'] });
    assert.equal(completed.task.status, 'done');
    assert.deepEqual(completed.readyTaskIds, ['T-002']);
    const second = await call(client, 'solution_claim_task', { workspaceId: saved.workspaceId, taskId: 'T-002', expectedRevision: completed.workspace.revision });
    assert.equal(second.task.status, 'in_progress');
    const persisted = await store.read(saved.workspaceId);
    assert.equal(persisted.executionPlan.statusEvents.length, 3);
    assert.equal(persisted.executionPlan.statusEvents[0].runtimeSessionId, 'executor-session-1');

    // Authoring updates may omit executor-owned history, but must never erase or forge it.
    persisted.executionPlan.positions = { 'T-001': { x: 12, y: 34 } };
    persisted.executionPlan.viewport = { x: 5, y: 6, zoom: 0.8 };
    const withLayout = await store.replace(persisted, persisted.revision);
    const author = new Client({ name: 'solution-authoring-history-test', version: '1.0.0' });
    try {
      await author.connect(new StdioClientTransport({
        command: process.execPath,
        args: [path.join(packageRoot, 'bin/chatos-solution-studio'), 'mcp-authoring'],
        env: { ...process.env, CHATOS_PLUGIN_DATA_DIR: dataRoot }
      }));
      const businessPlan = { status: 'approved', basedOnDesignRevision: 1, objective: '更新目标', tasks: workspace.executionPlan.tasks };
      const updated = await call(author, 'solution_upsert_execution_plan', { title: workspace.title, executionPlan: businessPlan });
      assert.deepEqual(updated.executionPlan.statusEvents, withLayout.executionPlan.statusEvents);
      assert.deepEqual(updated.executionPlan.tasks.map(task => task.status), ['done', 'in_progress']);
      assert.deepEqual(updated.executionPlan.positions, withLayout.executionPlan.positions);
      assert.deepEqual(updated.executionPlan.viewport, withLayout.executionPlan.viewport);
      const reorderedTasks = businessPlan.tasks.map(task => Object.fromEntries(Object.entries(task).reverse()));
      const reordered = await call(author, 'solution_upsert_execution_plan', {
        title: workspace.title,
        executionPlan: { ...businessPlan, tasks: reorderedTasks }
      });
      assert.deepEqual(reordered.executionPlan.tasks.map(task => task.status), ['done', 'in_progress']);

      const appendedTask = task('T-003', ['T-001']);
      const appended = await call(author, 'solution_append_execution_tasks', {
        workspaceId: saved.workspaceId,
        expectedRevision: reordered.workspace.revision,
        basedOnDesignRevision: 1,
        tasks: [appendedTask]
      });
      assert.deepEqual(appended.appendedTaskIds, ['T-003']);
      assert.deepEqual(appended.executionPlan.tasks.map(task => task.status), ['done', 'in_progress', 'planned']);
      assert.deepEqual(appended.executionPlan.statusEvents, withLayout.executionPlan.statusEvents);
      assert.deepEqual(appended.executionPlan.positions, withLayout.executionPlan.positions);
      assert.deepEqual(appended.executionPlan.viewport, withLayout.executionPlan.viewport);
      const duplicate = await author.callTool({
        name: 'solution_append_execution_tasks',
        arguments: {
          workspaceId: saved.workspaceId,
          expectedRevision: appended.workspace.revision,
          basedOnDesignRevision: 1,
          tasks: [task('T-001', [])]
        }
      });
      assert.equal(duplicate.isError, true);
      assert.match(duplicate.structuredContent.error, /已存在/);
      for (const tasks of [[], businessPlan.tasks.map(task => ({ ...task, title: '篡改定义' }))]) {
        const rejected = await author.callTool({ name: 'solution_upsert_execution_plan', arguments: { title: workspace.title, executionPlan: { ...businessPlan, tasks } } });
        assert.equal(rejected.isError, true);
        assert.match(rejected.structuredContent.error, /不能删除|不能修改/);
      }
      const afterAuthoring = await store.read(saved.workspaceId);
      assert.equal(afterAuthoring.revision, appended.workspace.revision);
      assert.deepEqual(afterAuthoring.executionPlan.statusEvents, withLayout.executionPlan.statusEvents);
      assert.equal(afterAuthoring.executionPlan.tasks.length, 3);
    } finally {
      await author.close();
    }
  } finally {
    await client.close();
    await rm(root, { recursive: true, force: true });
  }
});

async function call(client, name, args) {
  const response = await client.callTool({ name, arguments: args });
  assert.equal(response.isError, false, `${name} failed: ${response.structuredContent?.error ?? response.content?.[0]?.text}`);
  return response.structuredContent;
}
