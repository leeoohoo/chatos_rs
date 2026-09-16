import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const now = () => new Date().toISOString();

test('MCP exposes gated planning tools and rejects an invalid dependency graph before persistence', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-mcp-'));
  const client = new Client({ name: 'solution-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp'],
    env: { ...process.env, SOLUTION_STUDIO_DATA_DIR: path.join(root, 'data'), CHATOS_PLUGIN_ARTIFACT_DIR: path.join(root, 'exports'), CHATOS_CONTEXT_SCOPE: 'project', CHATOS_CONTEXT_SCOPE_ID: 'scope-mcp-123', CHATOS_PROJECT_ID: 'project-mcp-123', CHATOS_PROJECT_NAME: '示例项目', CHATOS_WORKSPACE_ID: 'workspace-mcp-456', CHATOS_WORKSPACE: '/workspace/example' }
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    const tools = new Map(listed.tools.map((tool) => [tool.name, tool]));
    for (const name of ['solution_get_active_context', 'solution_upsert_requirements', 'solution_upsert_design', 'solution_upsert_execution_plan', 'solution_validate', 'solution_export']) assert.ok(tools.has(name));
    assert.deepEqual(tools.get('solution_upsert_execution_plan')._meta['chatos/skillGate'].allOf, ['solution-studio', 'solution-execution-plan']);

    const context = await call(client, 'solution_get_active_context', {});
    assert.equal(context.scope.hasProjectContext, true);
    assert.equal(context.scope.projectId, 'project-mcp-123');
    assert.equal(context.scope.projectName, '示例项目');
    assert.equal(context.scope.connectorWorkspaceId, 'workspace-mcp-456');
    assert.equal(context.scope.projectRoot, '/workspace/example');
    assert.equal(context.workspace, null);

    const requirementResult = await call(client, 'solution_upsert_requirements', {
      title: 'MCP 方案',
      requirements: { status: 'approved', revision: 0, summary: '需求', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], items: [{ id: 'R-001', title: '能力', description: '提供能力。', priority: 'must', acceptanceCriteria: ['可以验证'], evidenceIds: [], selectedDesignSectionId: 'D-001' }], updatedAt: now() }
    });
    assert.equal(requirementResult.workspace.hostProjectId, 'project-mcp-123');
    await call(client, 'solution_upsert_design', {
      artifactKey: 'ignored-parallel-plan', title: 'MCP 方案', sourceMode: 'existing-project',
      design: { status: 'approved', revision: 0, basedOnRequirementsRevision: 1, summary: '方案', blocks: [{ id: 'D-000-B-001', type: 'text', title: '技术基线', content: 'TypeScript' }, { id: 'D-000-B-002', type: 'architecture', title: '总体架构', content: '<svg viewBox="0 0 100 100"><text x="5" y="10">Architecture</text></svg>' }], sections: [{ id: 'D-001', title: '核心设计', body: '设计内容', requirementIds: ['R-001'], evidenceIds: [], blocks: [{ id: 'D-001-B-001', type: 'text', title: '详细设计', content: '模块与接口。' }] }], decisions: [], risks: [], validationStrategy: [], updatedAt: now() }
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

    invalidPlan.tasks[0].dependsOn = [];
    const accepted = await call(client, 'solution_upsert_execution_plan', { title: 'MCP 方案', executionPlan: invalidPlan });
    assert.equal(accepted.validation.valid, true);
    assert.deepEqual(accepted.validation.topologicalOrder, ['T-001', 'T-002']);
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
