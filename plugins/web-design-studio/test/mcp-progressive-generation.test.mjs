import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { resolveHeadlessBrowserExecutable } from '../dist/v2-headless-scene-renderer.test.mjs';
import { executeSceneEditorCommand } from '../dist/v2-scene-editor-command.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';

const progressiveTools = [
  'web_design_get_active_context', 'web_design_plan_site', 'web_design_plan_page',
  'web_design_execute_step', 'web_design_control_plan',
  'web_design_capture_page', 'web_design_capture_region',
  'web_design_compare_snapshots', 'web_design_inspect_at_point', 'web_design_query_scene', 'web_design_edit_scene',
  'web_design_prepare_annotation_task', 'web_design_list_documents', 'web_design_create_document',
  'web_design_get_catalog', 'web_design_search_catalog', 'web_design_get_component_contract', 'web_design_list_requests'
];

const skillForTool = (name) => name === 'web_design_get_active_context' || name === 'web_design_plan_site' || name === 'web_design_plan_page'
  ? 'web-design-planning'
  : name === 'web_design_execute_step' || name === 'web_design_query_scene' || name === 'web_design_edit_scene'
    ? 'web-design-scene-building'
    : name === 'web_design_control_plan' || name === 'web_design_capture_page' || name === 'web_design_capture_region'
      || name === 'web_design_compare_snapshots' || name === 'web_design_inspect_at_point' || name === 'web_design_prepare_annotation_task'
      ? 'web-design-candidate-review'
      : name.includes('catalog') || name.includes('component')
        ? 'web-design-components'
        : 'web-design-documents';

let browserAvailable = false;
try { resolveHeadlessBrowserExecutable(); browserAvailable = true; } catch { /* Visual schemas remain testable without Chromium. */ }

function designIntent() {
  return {
    artDirection: 'Editorial product design with quiet material contrast',
    compositionIntent: 'One dominant focal point followed by varied supporting sections',
    typographyIntent: 'Large display type and compact readable supporting copy',
    imageStrategy: 'Use purposeful product imagery with a consistent crop language',
    contentHierarchy: ['Promise', 'Evidence', 'Action'],
    designAcceptanceCriteria: ['The primary focus is immediate', 'The page does not resemble an admin dashboard'],
    interactionIntents: []
  };
}

test('MCP exposes an AI-first single-step generation workflow without a model-supplied projectId', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-progressive-mcp-'));
  const client = new Client({ name: 'progressive-generation-test', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp'],
    env: {
      ...process.env,
      WEB_DESIGN_STUDIO_DATA_DIR: root,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'host-project-progressive',
      CHATOS_PROJECT_NAME: 'AI Design Project'
    }
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    assert.equal(listed.tools.length, 18);
    for (const name of progressiveTools) {
      const tool = listed.tools.find((candidate) => candidate.name === name);
      assert.ok(tool, `missing ${name}`);
      assert.deepEqual(tool._meta['chatos/skillGate'].allOf, ['web-design-studio', skillForTool(name)]);
      assert.equal(JSON.stringify(tool.inputSchema).includes('projectId'), false);
    }
    const runTool = listed.tools.find((candidate) => candidate.name === 'web_design_execute_step');
    assert.match(JSON.stringify(runTool.inputSchema), /operationsJson/);
    assert.equal(JSON.stringify(runTool.inputSchema).includes('insert-simple-tree'), true);
    assert.equal(JSON.stringify(runTool.inputSchema).includes('visualInputs'), false);
    const queryTool = listed.tools.find((candidate) => candidate.name === 'web_design_query_scene');
    assert.equal(JSON.stringify(queryTool.inputSchema).includes('pageIds'), false);

    const created = await client.callTool({ name: 'web_design_create_document', arguments: { title: 'AI Progressive Website', blank: true } });
    const documentId = created.structuredContent.document.documentId;
    const context = await client.callTool({ name: 'web_design_get_active_context', arguments: {} });
    assert.equal(context.structuredContent.scope.projectId, 'host-project-progressive');
    assert.equal(context.structuredContent.active.documentId, documentId);
    assert.equal(context.structuredContent.nextAction.tool, 'web_design_plan_site');
    assert.equal(context.structuredContent.deliveryGate.code, 'NO_SITE_PLAN');
    assert.equal(context.structuredContent.deliveryGate.projectImplementationAllowed, false);

    const site = await client.callTool({
      name: 'web_design_plan_site',
      arguments: {
        documentId,
        planId: 'plan:mcp-progressive',
        mode: 'auto-current-page',
        objective: 'Design a visually distinctive AI product website',
        audience: ['Product design reviewers'],
        pages: [
          { pageId: 'home', name: '首页', purpose: '建立视觉品牌与产品价值' },
          { pageId: 'pricing', name: '价格', purpose: '解释方案并支持购买决策' }
        ]
      }
    });
    assert.equal(site.isError, false);
    assert.equal(site.structuredContent.plan.revision, 1);
    assert.equal(site.structuredContent.plan.nextAction.tool, 'web_design_plan_page');
    assert.equal(site.structuredContent.plan.deliveryGate.code, 'NO_ACCEPTED_VISUAL_STEP');
    assert.deepEqual(site.structuredContent.plan.deliveryGate.requiredNextAction, site.structuredContent.plan.nextAction);

    const page = await client.callTool({
      name: 'web_design_plan_page',
      arguments: {
        documentId,
        expectedPlanRevision: 1,
        pageId: 'home',
        design: designIntent(),
        steps: [
          { stepId: 'home-structure', title: '建立首页骨架', kind: 'structure', target: { viewportWidths: [390, 1440] } },
          { stepId: 'home-gate', title: '首页视觉验收', kind: 'design-gate', dependsOn: ['home-structure'], target: { viewportWidths: [390, 1440] } },
          { stepId: 'home-handoff', title: '首页最终验收', kind: 'handoff', dependsOn: ['home-gate'], target: { viewportWidths: [390, 1440] } }
        ]
      }
    });
    assert.equal(page.isError, false);
    assert.equal(page.structuredContent.plan.revision, 4);
    assert.equal(page.structuredContent.scene.revision, 2);
    assert.equal(page.structuredContent.plan.nextAction.tool, 'web_design_execute_step');
    assert.equal(page.structuredContent.plan.nextAction.stepId, 'home-structure');
    assert.deepEqual(page.structuredContent.plan.nextAction.target.viewportWidths, [390, 1440]);
    assert.equal(page.structuredContent.plan.deliveryGate.visibleSceneReady, false);
    const plannedContext = await client.callTool({ name: 'web_design_get_active_context', arguments: {} });
    assert.deepEqual(plannedContext.structuredContent.artboardDirectory.map((item) => item.artboardId), ['home', 'pricing']);
    assert.equal(plannedContext.structuredContent.active.pageId, 'home');

    if (!browserAvailable) return;

    const executed = await client.callTool({
      name: 'web_design_execute_step',
      arguments: {
        documentId,
        expectedPlanRevision: 4,
        requestId: 'mcp-home-structure-scene-2',
        operationsJson: JSON.stringify([
          {
            op: 'insert-simple-tree', parentId: 'root:home', index: 0,
            tree: { node: {
                id: 'section:hero', type: 'frame', name: 'Hero section', role: 'hero',
                frame: { x: 0, y: 0, width: 1440, height: 720 },
                layout: { mode: 'auto', direction: 'vertical', padding: 40, gap: 24, sizingX: 'fill' },
                style: { fill: '#14213d' }
              }, children: [{ node: {
                id: 'hero:title', type: 'text', name: 'Hero title', role: 'heading',
                frame: { x: 40, y: 40, width: 760, height: 120 }, content: 'Design with clarity',
                layout: { sizingX: 'fill' },
                style: { fill: '#ffffff', fontSize: 32, fontWeight: 800 }
              } }] }
          }
        ])
      }
    });
    assert.equal(executed.isError, false);
    assert.equal(executed.structuredContent.status, 'awaiting-review', JSON.stringify(executed.structuredContent));
    assert.equal(executed.content.some((item) => item.type === 'image' && item.mimeType === 'image/png'), true);
    const accepted = await client.callTool({
      name: 'web_design_control_plan',
      arguments: {
        documentId,
        expectedPlanRevision: executed.structuredContent.plan.revision,
        action: 'accept',
        stepId: 'home-structure',
        attemptId: executed.structuredContent.candidate.attemptId
      }
    });
    assert.equal(accepted.isError, false);
    assert.equal(accepted.structuredContent.status, 'committed');
    assert.equal(accepted.structuredContent.scene.revision, 3);
    assert.equal(accepted.structuredContent.plan.pages.find((item) => item.pageId === 'home').stepCounts.accepted, 1);
    assert.equal(accepted.structuredContent.plan.pages.find((item) => item.pageId === 'pricing').status, 'unplanned');
    assert.equal(accepted.structuredContent.plan.deliveryGate.visibleSceneReady, true);
    assert.equal(accepted.structuredContent.plan.deliveryGate.projectImplementationAllowed, false);

    const sceneQuery = await client.callTool({
      name: 'web_design_query_scene',
      arguments: { documentId, artboardId: 'home', query: { ids: ['root:home', 'section:hero', 'hero:title'] } }
    });
    assert.equal(sceneQuery.isError, false);
    assert.equal(sceneQuery.structuredContent.scene.revision, 3);
    assert.equal(sceneQuery.structuredContent.scene.activeArtboard.artboardId, 'home');
    assert.equal(Object.hasOwn(sceneQuery.structuredContent.scene, 'pages'), false);
    assert.deepEqual(sceneQuery.structuredContent.results.map((entry) => entry.nodeId), ['root:home', 'section:hero', 'hero:title']);
    assert.equal(sceneQuery.structuredContent.results.find((entry) => entry.nodeId === 'section:hero').parentId, 'root:home');
    assert.equal(sceneQuery.structuredContent.results.find((entry) => entry.nodeId === 'hero:title').parentId, 'section:hero');

    const rejectedCrossArtboardEdit = await client.callTool({
      name: 'web_design_edit_scene',
      arguments: {
        documentId,
        artboardId: 'pricing',
        transactionId: 'transaction:cross-artboard-edit',
        expectedRevision: 3,
        reason: 'This must be rejected before mutation.',
        commandJson: JSON.stringify({ type: 'resize', nodeId: 'root:home', handle: 'east', deltaX: 80, deltaY: 0 })
      }
    });
    assert.equal(rejectedCrossArtboardEdit.isError, true);

    const edited = await client.callTool({
      name: 'web_design_edit_scene',
      arguments: {
        documentId,
        artboardId: 'home',
        transactionId: 'transaction:mcp-resize-root',
        expectedRevision: 3,
        reason: 'Give the desktop composition more horizontal room.',
        commandJson: JSON.stringify({ type: 'resize', nodeId: 'root:home', handle: 'east', deltaX: 80, deltaY: 0 })
      }
    });
    assert.equal(edited.isError, false);
    assert.equal(edited.structuredContent.scene.revision, 4);
    assert.equal(edited.structuredContent.commandType, 'resize');
    assert.equal(edited.structuredContent.recovered, false);
    assert.deepEqual(edited.structuredContent.affectedPageIds, ['home']);
    assert.equal(edited.structuredContent.nextRecommendedActions.some((action) => action.tool === 'web_design_capture_page'), true);

    const retriedEdit = await client.callTool({
      name: 'web_design_edit_scene',
      arguments: {
        documentId,
        artboardId: 'home',
        transactionId: 'transaction:mcp-resize-root',
        expectedRevision: 3,
        reason: 'Give the desktop composition more horizontal room.',
        commandJson: JSON.stringify({ type: 'resize', nodeId: 'root:home', handle: 'east', deltaX: 80, deltaY: 0 })
      }
    });
    assert.equal(retriedEdit.isError, false);
    assert.equal(retriedEdit.structuredContent.scene.revision, 4);
    assert.equal(retriedEdit.structuredContent.recovered, true);

    const directScenes = new SceneDocumentStore(root);
    const annotated = await executeSceneEditorCommand(directScenes, documentId, {
      transactionId: 'transaction:human-annotation', expectedRevision: 4,
      command: {
        type: 'add-annotation', nodeId: 'section:hero', annotationId: 'annotation:hero-hierarchy',
        body: 'Strengthen the visual hierarchy of this hero without changing the approved page structure.'
      }
    }, 'human');
    assert.equal(annotated.document.revision, 5);
    const listedRequests = await client.callTool({
      name: 'web_design_list_requests', arguments: { documentId }
    });
    const sceneRequest = listedRequests.structuredContent.requests.find((entry) => entry.kind === 'scene-annotation');
    assert.equal(sceneRequest.pageId, 'home');
    assert.equal(sceneRequest.revision, 5);
    assert.equal(sceneRequest.request.nodeId, 'section:hero');
    assert.equal(sceneRequest.prepareWith.tool, 'web_design_prepare_annotation_task');

    if (browserAvailable) {
      const prepared = await client.callTool({
        name: 'web_design_prepare_annotation_task',
        arguments: { documentId, nodeId: 'section:hero', annotationId: 'annotation:hero-hierarchy', viewportWidth: 800 }
      });
      assert.equal(prepared.isError, false);
      assert.equal(prepared.structuredContent.task.scope.projectId, 'host-project-progressive');
      assert.equal(prepared.structuredContent.task.pageId, 'home');
      assert.equal(prepared.structuredContent.task.baseRevision, 5);
      assert.equal(prepared.structuredContent.nextAction.targetNodeId, 'section:hero');
      assert.equal(prepared.content.some((item) => item.type === 'image' && item.mimeType === 'image/png'), true);
    }

    const renamedThroughPortablePath = await client.callTool({
      name: 'web_design_edit_scene',
      arguments: {
        documentId,
        artboardId: 'home',
        transactionId: 'transaction:mcp-rename-hero',
        expectedRevision: 5,
        reason: 'Use the approved semantic label for the hero region.',
        commandJson: JSON.stringify({
          type: 'update-node',
          nodeId: 'section:hero',
          patches: [{ path: ['name'], value: 'Primary Hero' }]
        })
      }
    });
    assert.equal(renamedThroughPortablePath.isError, false);
    assert.equal(renamedThroughPortablePath.structuredContent.scene.revision, 6);
    assert.equal(renamedThroughPortablePath.structuredContent.commandType, 'update-node');
  } finally {
    await client.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
  }
});
