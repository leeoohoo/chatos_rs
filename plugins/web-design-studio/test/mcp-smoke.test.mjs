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
  'web_design_capture_page',
  'web_design_export_handoff',
  'web_design_capture_region',
  'web_design_prepare_annotation_task',
  'web_design_compare_snapshots',
  'web_design_inspect_at_point',
  'web_design_query_scene',
  'web_design_edit_scene',
  'web_design_execute_step',
  'web_design_control_plan',
  'web_design_list_documents',
  'web_design_create_document',
  'web_design_get_catalog',
  'web_design_search_catalog',
  'web_design_get_component_contract',
  'web_design_list_requests'
];
const packageRoot = process.env.CHATOS_PLUGIN_TEST_PACKAGE_ROOT || path.resolve(import.meta.dirname, '..');

test('MCP exposes only the AI-first Scene 3.0.1 surface and preserves host project scope', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-studio-test-'));
  const client = new Client({ name: 'web-design-studio-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.join(packageRoot, 'bin/chatos-web-design-studio'), 'mcp'],
    env: {
      ...process.env,
      CHATOS_PLUGIN_ROOT: packageRoot,
      WEB_DESIGN_STUDIO_DATA_DIR: root,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'host-project-through-123',
      CHATOS_PROJECT_NAME: '宿主产品项目',
      CHATOS_WORKSPACE_ID: 'workspace-through-456',
      CHATOS_PLUGIN_ARTIFACT_DIR: path.join(root, 'artifacts')
    }
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    assert.deepEqual(listed.tools.map((tool) => tool.name), sceneV3Tools);
    assert.equal(listed.tools.some((tool) => tool.name === 'web_design_export_handoff'), true);
    assert.equal(listed.tools.some((tool) => ['web_design_export_html', 'web_design_export_react', 'web_design_export_vue'].includes(tool.name)), false);
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
    assert.equal(catalog.content.find((item) => item.type === 'text').text.includes('\n'), false);
    assert.deepEqual(catalog.structuredContent.libraries.map((item) => item.id), ['antd', 'chakra', 'shadcn', 'magicui', 'spell', 'inspira', 'daisyui']);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'antd').componentCount, 72);
    assert.equal(catalog.structuredContent.libraries.find((item) => item.id === 'chakra').componentCount, 113);

    const search = await client.callTool({
      name: 'web_design_search_catalog',
      arguments: { kind: 'components', libraryId: 'antd', query: 'input', limit: 5 }
    });
    assert.equal(search.structuredContent.candidates.every((item) => item.libraryId === 'antd'), true);
    assert.equal(search.structuredContent.candidates.some((item) => item.componentId === 'Input'), true);

    const intentSearch = await client.callTool({
      name: 'web_design_search_catalog',
      arguments: { kind: 'components', libraryId: 'shadcn', category: '按钮', query: 'primary CTA button', limit: 5 }
    });
    assert.equal(intentSearch.structuredContent.candidates[0].componentId, 'Button');

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

    // Exercise the installed server's actual generation/rendering path, not source stores.
    const site = await call(client, 'web_design_plan_site', {
      documentId, planId: 'plan:installed-smoke', mode: 'auto-current-page',
      objective: 'Create a fictional coffee landing page for plugin acceptance',
      audience: ['Test reviewers'],
      pages: [{ pageId: 'home', name: '首页', purpose: '介绍虚构咖啡品牌' }]
    });
    const page = await call(client, 'web_design_plan_page', {
      documentId, expectedPlanRevision: site.plan.revision, pageId: 'home',
      design: {
        artDirection: 'Quiet editorial coffee brand with warm contrast',
        compositionIntent: 'One clear hero followed by supporting content',
        typographyIntent: 'Large heading and readable supporting copy',
        imageStrategy: 'No external imagery needed for this isolated fixture',
        contentHierarchy: ['Brand', 'Introduction', 'Action'],
        designAcceptanceCriteria: ['The heading is readable', 'The content fits mobile and desktop'],
        interactionIntents: []
      },
      steps: [
        { stepId: 'home-structure', title: '建立首页骨架', kind: 'structure', target: { viewportWidths: [390, 1440] } },
        { stepId: 'home-gate', title: '视觉验收', kind: 'design-gate', dependsOn: ['home-structure'], target: { viewportWidths: [390, 1440] } },
        { stepId: 'home-handoff', title: '最终验收', kind: 'handoff', dependsOn: ['home-gate'], target: { viewportWidths: [390, 1440] } }
      ]
    });
    const executedResponse = await client.callTool({ name: 'web_design_execute_step', arguments: {
      documentId, expectedPlanRevision: page.plan.revision, requestId: 'installed-smoke-hero',
      operations: [{ op: 'insert-simple-tree', parentId: 'root:home', index: 0,
        tree: { node: {
          id: 'section:hero', type: 'frame', name: 'Hero', role: 'hero',
          frame: { x: 0, y: 0, width: 1440, height: 720 },
          layout: { mode: 'auto', direction: 'vertical', padding: 40, gap: 24, sizingX: 'fill' },
          style: { fill: '#14213d' }
        }, children: [{ node: {
          id: 'hero:title', type: 'text', name: 'Brand', role: 'heading', content: 'North Shore Coffee',
          frame: { x: 40, y: 40, width: 760, height: 120 }, layout: { sizingX: 'fill' },
          style: { fill: '#ffffff', fontSize: 32, fontWeight: 800 }
        } }] }
      }]
    } });
    assert.equal(executedResponse.isError, false, executedResponse.content?.[0]?.text);
    const executed = executedResponse.structuredContent;
    assert.equal(executed.status, 'awaiting-review');
    assert.ok(executedResponse.content.some(item => item.type === 'image' && item.mimeType === 'image/png'));
    const rejected = await call(client, 'web_design_control_plan', {
      documentId, expectedPlanRevision: executed.plan.revision, action: 'reject',
      stepId: 'home-structure', attemptId: executed.candidate.attemptId,
      reason: 'Add an explicit readable supporting statement before delivery'
    });
    assert.equal(rejected.recovery.recoverable, true);
    assert.equal(rejected.plan.nextAction.type, 'retry-step');
    assert.equal(rejected.plan.deliveryGate.taskCompletionAllowed, false);
    const discarded = await call(client, 'web_design_query_scene', {
      documentId, artboardId: 'home', query: { ids: ['section:hero', 'hero:title'] }
    });
    assert.equal(discarded.results.length, 0, 'Reject must not commit Candidate nodes');
    // Exercise the styled, editable authoring example shipped in the Skill.
    const { readFile } = await import('node:fs/promises');
    const skill = await readFile(path.join(packageRoot, 'skills/web-design-scene-building/SKILL.md'), 'utf8');
    const operations = JSON.parse(skill.match(/```json\s*([\s\S]*?)```/)[1]);
    operations[0].parentId = 'root:home';
    const repairedResponse = await client.callTool({ name: 'web_design_execute_step', arguments: {
      documentId, expectedPlanRevision: rejected.plan.revision, stepId: 'home-structure',
      requestId: 'installed-smoke-hero-repaired', operations
    } });
    assert.equal(repairedResponse.isError, false, repairedResponse.content?.[0]?.text);
    assert.ok(repairedResponse.content.some(item => item.type === 'image'));
    const repaired = repairedResponse.structuredContent;
    const accepted = await call(client, 'web_design_control_plan', {
      documentId, expectedPlanRevision: repaired.plan.revision, action: 'accept',
      stepId: 'home-structure', attemptId: repaired.candidate.attemptId
    });
    assert.equal(accepted.status, 'committed');
    assert.equal(accepted.plan.deliveryGate.visibleSceneReady, true);
    assert.equal(accepted.plan.deliveryGate.projectImplementationAllowed, false, 'One accepted structure is not final delivery');
    const query = await call(client, 'web_design_query_scene', {
      documentId, artboardId: 'home', query: { ids: ['hero', 'hero-title', 'hero-cta-label'] }
    });
    assert.equal(query.scene.revision, accepted.scene.revision);
    assert.deepEqual(query.results.map(entry => entry.nodeId), ['hero', 'hero-title', 'hero-cta-label']);
    assert.equal(query.results[1].parentId, 'hero');
    const capture = await client.callTool({ name: 'web_design_capture_page', arguments: { documentId, pageId: 'home', viewportWidth: 390 } });
    assert.equal(capture.isError, false, capture.content?.[0]?.text);
    const png = capture.content.find(item => item.type === 'image' && item.mimeType === 'image/png');
    assert.ok(png, 'A real mobile PNG must be returned, not only snapshot metadata');
    assert.deepEqual([...Buffer.from(png.data, 'base64').subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    let planRevision = accepted.plan.revision;
    for (const stepId of ['home-gate', 'home-handoff']) {
      const review = await call(client, 'web_design_execute_step', {
        documentId, expectedPlanRevision: planRevision, stepId,
        requestId: `installed-smoke-${stepId}`,
        operations: [{ op: 'update-node', nodeId: 'hero', patches: [{ path: ['name'], value: `Reviewed ${stepId}` }] }]
      });
      const committed = await call(client, 'web_design_control_plan', {
        documentId, expectedPlanRevision: review.plan.revision, action: 'accept', stepId,
        attemptId: review.candidate.attemptId
      });
      planRevision = committed.plan.revision;
      assert.equal(committed.plan.deliveryGate.taskCompletionAllowed, stepId === 'home-handoff');
    }
    const delivery = await client.callTool({ name: 'web_design_export_handoff', arguments: {
      documentId, pageId: 'home', fileBaseName: 'north-shore-coffee'
    } });
    assert.equal(delivery.isError, false, delivery.content?.[0]?.text);
    assert.deepEqual(delivery.structuredContent.files.map((file) => file.displayName), [
      'north-shore-coffee.html',
      'north-shore-coffee-desktop.png',
      'north-shore-coffee-mobile.png'
    ]);
    assert.equal(delivery._meta['chatos/artifacts'].length, 3);
    assert.equal(delivery.structuredContent.files.every((file) => /^[0-9a-f]{64}$/.test(file.sha256)), true);
    const { readFile: readExportedFile } = await import('node:fs/promises');
    const exportedHtml = await readExportedFile(path.join(root, 'artifacts', 'north-shore-coffee.html'), 'utf8');
    assert.match(exportedHtml, /data-chatos-responsive-handoff/);
    assert.match(exportedHtml, /A real headline/);
  } finally {
    await client.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
  }
});

async function call(client, name, args) {
  const response = await client.callTool({ name, arguments: args });
  assert.equal(response.isError, false, `${name}: ${response.content?.[0]?.text}`);
  return response.structuredContent;
}
