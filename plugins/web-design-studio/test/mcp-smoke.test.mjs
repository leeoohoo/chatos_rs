import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const sceneV3Tools = [
  'web_design_get_active_context',
  'web_design_plan_site',
  'web_design_plan_page',
  'web_design_get_plan',
  'web_design_start_page',
  'web_design_capture_page',
  'web_design_capture_region',
  'web_design_prepare_annotation_task',
  'web_design_get_visual_grounding',
  'web_design_compare_snapshots',
  'web_design_inspect_at_point',
  'web_design_query_scene',
  'web_design_edit_scene',
  'web_design_run_next_step',
  'web_design_retry_step',
  'web_design_repair_step',
  'web_design_inspect_step',
  'web_design_accept_step',
  'web_design_reject_step',
  'web_design_skip_step',
  'web_design_rollback_step',
  'web_design_complete_page',
  'web_design_pause_plan',
  'web_design_resume_plan',
  'web_design_list_documents',
  'web_design_create_document',
  'web_design_get_catalog',
  'web_design_search_components',
  'web_design_get_component_contract',
  'web_design_list_requests'
];

test('MCP exposes only the AI-first Scene 3.0.1 surface and preserves host project scope', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-studio-test-'));
  const client = new Client({ name: 'web-design-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp'],
    env: {
      ...process.env,
      WEB_DESIGN_STUDIO_DATA_DIR: root,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'host-project-through-123',
      CHATOS_PROJECT_NAME: '宿主产品项目',
      CHATOS_WORKSPACE_ID: 'workspace-through-456'
    }
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    assert.deepEqual(listed.tools.map((tool) => tool.name), sceneV3Tools);
    assert.equal(listed.tools.some((tool) => tool.name.includes('export')), false);
    assert.equal(listed.tools.some((tool) => tool.name === 'web_design_apply_patch'), false);
    assert.equal(listed.tools.some((tool) => tool.name === 'web_design_apply_page_template'), false);
    assert.equal(listed.tools.some((tool) => tool.name === 'web_design_insert_section'), false);
    assert.equal(listed.tools.some((tool) => tool.name.includes('project')), false);
    assert.equal(JSON.stringify(listed.tools.map((tool) => tool.inputSchema)).includes('projectId'), false);
    assert.equal(JSON.stringify(listed.tools.map((tool) => tool.inputSchema)).includes('project_id'), false);
    for (const tool of listed.tools) {
      assert.ok(tool._meta['chatos/skillGate'].allOf.length >= 2);
      assert.equal(Object.hasOwn(tool.inputSchema.properties ?? {}, 'skillEvidence'), false);
    }

    const initialDocuments = await client.callTool({ name: 'web_design_list_documents', arguments: {} });
    assert.deepEqual(initialDocuments.structuredContent.documents, []);

    const catalog = await client.callTool({ name: 'web_design_get_catalog', arguments: {} });
    assert.deepEqual(catalog.structuredContent.libraries.map((item) => item.id), ['antd', 'chakra', 'shadcn', 'magicui', 'spell', 'inspira', 'daisyui']);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'antd').componentCount, 72);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'chakra').componentCount, 113);

    const search = await client.callTool({
      name: 'web_design_search_components',
      arguments: { libraryId: 'antd', query: 'input', limit: 5 }
    });
    assert.equal(search.structuredContent.candidates.every((item) => item.libraryId === 'antd'), true);
    assert.equal(search.structuredContent.candidates.some((item) => item.componentId === 'Input'), true);

    const created = await client.callTool({ name: 'web_design_create_document', arguments: { title: 'AI-first Website' } });
    const documentId = created.structuredContent.document.documentId;
    assert.equal(created.isError, false);
    assert.equal(Object.hasOwn(created.structuredContent, 'scope'), false);

    const context = await client.callTool({ name: 'web_design_get_active_context', arguments: {} });
    assert.equal(context.structuredContent.scope.projectId, 'host-project-through-123');
    assert.equal(context.structuredContent.active.documentId, documentId);
    assert.equal(context.structuredContent.nextAction.tool, 'web_design_plan_site');

    const documents = await client.callTool({ name: 'web_design_list_documents', arguments: {} });
    assert.deepEqual(documents.structuredContent.documents.map((item) => item.documentId), [documentId]);
    const requests = await client.callTool({ name: 'web_design_list_requests', arguments: { documentId } });
    assert.deepEqual(requests.structuredContent.requests, []);
  } finally {
    await client.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
  }
});
