import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const now = () => new Date().toISOString();
const packageRoot = process.env.CHATOS_PLUGIN_TEST_PACKAGE_ROOT || path.resolve(import.meta.dirname, '..');

test('MCP exposes gated planning tools and rejects an invalid dependency graph before persistence', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-mcp-'));
  const projectRoot = path.join(root, 'host-project');
  await mkdir(projectRoot);
  const client = new Client({ name: 'solution-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.join(packageRoot, 'bin/chatos-solution-studio'), 'mcp'],
    env: { ...process.env, SOLUTION_STUDIO_DATA_DIR: path.join(root, 'data'), CHATOS_PLUGIN_ARTIFACT_DIR: path.join(root, 'exports'), CHATOS_CONTEXT_SCOPE: 'project', CHATOS_CONTEXT_SCOPE_ID: 'scope-mcp-123', CHATOS_PROJECT_ID: 'project-mcp-123', CHATOS_PROJECT_NAME: '示例项目', CHATOS_WORKSPACE_ID: 'workspace-mcp-456', CHATOS_WORKSPACE: projectRoot }
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    const tools = new Map(listed.tools.map((tool) => [tool.name, tool]));
    for (const name of ['solution_get_active_context', 'solution_upsert_requirements', 'solution_upsert_design', 'solution_upsert_design_block', 'solution_upsert_execution_plan', 'solution_validate', 'solution_export', 'solution_finalize']) assert.ok(tools.has(name));
    assert.deepEqual(tools.get('solution_upsert_execution_plan')._meta['chatos/skillGate'].allOf, ['solution-studio', 'solution-execution-plan']);

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
      requirements: { status: 'approved', revision: 0, summary: '需求', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], items: ['R-001', 'R-002'].map(id => ({ id, title: `能力 ${id}`, description: '提供能力。', priority: 'must', acceptanceCriteria: ['可以验证'], evidenceIds: [], selectedDesignSectionId: 'D-001' })), updatedAt: now() }
    });
    assert.equal(requirementResult.workspace.hostProjectId, 'project-mcp-123');
    assert.equal(requirementResult.validation.valid, false);
    assert.ok(requirementResult.validation.issues.some(issue => issue.code === 'design_empty'));
    assert.ok(requirementResult.validation.issues.some(issue => issue.code === 'plan_empty'));
    assert.equal(requirementResult.continuation.kind, 'authoring-work-remaining');
    assert.deepEqual(requirementResult.continuation.suggestedActions.map(action => action.tool),
      ['solution_upsert_design', 'solution_upsert_execution_plan', 'solution_finalize']);
    await call(client, 'solution_upsert_design', {
      artifactKey: 'ignored-parallel-plan', title: 'MCP 方案', sourceMode: 'existing-project',
      design: { status: 'approved', revision: 0, basedOnRequirementsRevision: 1, summary: '方案', blocks: [{ id: 'D-000-B-001', type: 'text', title: '技术基线', content: 'TypeScript' }, { id: 'D-000-B-002', type: 'architecture', title: '总体架构', content: '<svg viewBox="0 0 100 100"><text x="5" y="10">Architecture</text></svg>' }], sections: [{ id: 'D-001', title: '核心设计', body: '设计内容', requirementIds: ['R-001', 'R-002'], evidenceIds: [], blocks: [{ id: 'D-001-B-001', type: 'text', title: '详细设计', content: '模块与接口。' }, { id: 'D-001-B-002', type: 'flowchart', title: '核心流程', content: '<svg viewBox="0 0 100 100"><text x="5" y="10">Flow</text></svg>' }] }], decisions: [], risks: [], validationStrategy: [], updatedAt: now() }
    });
    const afterDesign = await call(client, 'solution_get_active_context', {});
    assert.equal(afterDesign.workspace.workspaceId, requirementResult.workspace.workspaceId);

    const invalidPlan = {
      status: 'approved', revision: 0, basedOnDesignRevision: 1, objective: '执行', positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: now(),
      tasks: [
        { id: 'T-001', title: 'A', description: '', type: 'task', phase: '实现', dependsOn: ['T-002'], status: 'planned', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: [], acceptanceCriteria: ['A 完成'], sourceReferences: [] },
        { id: 'T-002', title: 'B', description: '', type: 'task', phase: '实现', dependsOn: ['T-001'], status: 'planned', requirementIds: ['R-001'], designSectionIds: ['D-001'], deliverables: [], acceptanceCriteria: ['B 完成'], sourceReferences: [] }
      ]
    };
    const rejected = await client.callTool({ name: 'solution_upsert_execution_plan', arguments: { title: 'MCP 方案', executionPlan: invalidPlan } });
    assert.equal(rejected.isError, true);
    assert.match(rejected.structuredContent.error, /循环依赖/);
    const afterReject = await call(client, 'solution_get_workspace', {});
    assert.equal(afterReject.workspace.executionPlan.tasks.length, 0);

    const task = (id, requirementId, dependsOn) => ({ id, title: `任务 ${id}`, description: '仅用于隔离验收。', type: 'task', phase: '实现', dependsOn, status: 'planned', requirementIds: [requirementId], designSectionIds: ['D-001'], deliverables: [`${id} 交付物`], acceptanceCriteria: [`${id} 可验证`], sourceReferences: [] });
    const validPlan = { ...invalidPlan, tasks: [
      task('T-001', 'R-001', []),
      task('T-002', 'R-001', ['T-001']),
      task('T-003', 'R-002', ['T-001']),
      task('T-004', 'R-002', ['T-002', 'T-003'])
    ] };
    const accepted = await call(client, 'solution_upsert_execution_plan', { title: 'MCP 方案', executionPlan: validPlan });
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
        items: ['R-001', 'R-002'].map(id => ({ id, title: `能力 ${id}`, description: '修订后需求。', priority: 'must', acceptanceCriteria: ['可以验证'], evidenceIds: [] })),
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

async function call(client, name, args) {
  const response = await client.callTool({ name, arguments: args });
  assert.equal(response.isError, false, `${name} failed: ${response.structuredContent?.error ?? response.content?.[0]?.text}`);
  return response.structuredContent;
}
