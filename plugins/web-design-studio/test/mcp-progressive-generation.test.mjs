import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { resolveHeadlessBrowserExecutable } from '../dist/v2-headless-scene-renderer.test.mjs';
import { createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { executeSceneEditorCommand } from '../dist/v2-scene-editor-command.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';

const progressiveTools = [
  'web_design_get_active_context', 'web_design_plan_site', 'web_design_plan_page', 'web_design_get_plan',
  'web_design_start_page', 'web_design_run_next_step', 'web_design_retry_step', 'web_design_inspect_step',
  'web_design_repair_step', 'web_design_accept_step', 'web_design_reject_step', 'web_design_skip_step',
  'web_design_rollback_step', 'web_design_complete_page', 'web_design_pause_plan', 'web_design_resume_plan',
  'web_design_capture_page', 'web_design_capture_region', 'web_design_get_visual_grounding',
  'web_design_compare_snapshots', 'web_design_inspect_at_point', 'web_design_query_scene', 'web_design_edit_scene',
  'web_design_prepare_annotation_task'
];

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

function artifacts(prefix, revision, kinds) {
  return kinds.map((kind, index) => ({
    artifactId: `${prefix}:${index}`,
    kind,
    revision,
    viewportWidth: 1440,
    uri: `artifact://${prefix}/${kind}`,
    createdAt: '2026-09-08T16:00:00.000Z'
  }));
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
    for (const name of progressiveTools) {
      const tool = listed.tools.find((candidate) => candidate.name === name);
      assert.ok(tool, `missing ${name}`);
      assert.deepEqual(tool._meta['chatos/skillGate'].allOf, ['web-design-studio', 'web-design-progressive-generation']);
      assert.equal(JSON.stringify(tool.inputSchema).includes('projectId'), false);
    }

    const created = await client.callTool({ name: 'web_design_create_document', arguments: { title: 'AI Progressive Website', blank: true } });
    const documentId = created.structuredContent.document.documentId;
    const context = await client.callTool({ name: 'web_design_get_active_context', arguments: {} });
    assert.equal(context.structuredContent.scope.projectId, 'host-project-progressive');
    assert.equal(context.structuredContent.active.documentId, documentId);
    assert.equal(context.structuredContent.nextAction.tool, 'web_design_plan_site');

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
    assert.equal(page.structuredContent.plan.revision, 3);
    assert.equal(page.structuredContent.plan.nextAction.tool, 'web_design_start_page');

    const started = await client.callTool({
      name: 'web_design_start_page',
      arguments: { documentId, expectedPlanRevision: 3, pageId: 'home', viewportWidth: 1440 }
    });
    assert.equal(started.isError, false);
    assert.equal(started.structuredContent.scene.revision, 2);
    assert.equal(started.structuredContent.plan.nextAction.stepId, 'home-structure');

    if (browserAvailable) {
      const captured = await client.callTool({
        name: 'web_design_capture_page',
        arguments: { documentId, pageId: 'home', viewportWidth: 800 }
      });
      assert.equal(captured.isError, false);
      assert.equal(captured.structuredContent.capture.artifact.kind, 'page-snapshot');
      assert.equal(Object.hasOwn(captured.structuredContent, '__images'), false);
      assert.equal(captured.content.some((item) => item.type === 'image' && item.mimeType === 'image/png' && item.data.length > 100), true);
    }

    const section = {
      ...createSceneNodeBase('section', 'Hero section', { x: 0, y: 0, width: 1440, height: 720 }, 'ai'),
      type: 'section', id: 'section:hero', role: 'hero', children: []
    };
    const executed = await client.callTool({
      name: 'web_design_run_next_step',
      arguments: {
        documentId,
        expectedPlanRevision: 4,
        idempotencyKey: 'mcp-home-structure-scene-2',
        transactionId: 'transaction:mcp-home-structure',
        operations: [{ op: 'insert-node', parentId: 'root:home', index: 0, node: section }],
        visualInputs: artifacts('before', 2, ['page-snapshot', 'visual-grounding']),
        verification: {
          passed: true,
          qualitySummary: 'The first bounded section establishes a clear composition.',
          issueIds: [],
          artifacts: artifacts('candidate', 3, ['layout', 'page-snapshot', 'visual-grounding', 'visual-diff', 'calibration', 'quality-report'])
        }
      }
    });
    assert.equal(executed.isError, false);
    assert.equal(executed.structuredContent.status, 'committed');
    assert.equal(executed.structuredContent.scene.revision, 3);
    assert.equal(executed.structuredContent.plan.pages.find((item) => item.pageId === 'home').stepCounts.accepted, 1);
    assert.equal(executed.structuredContent.plan.pages.find((item) => item.pageId === 'pricing').status, 'unplanned');

    const sceneQuery = await client.callTool({
      name: 'web_design_query_scene',
      arguments: { documentId, query: { ids: ['root:home', 'section:hero'] } }
    });
    assert.equal(sceneQuery.isError, false);
    assert.equal(sceneQuery.structuredContent.scene.revision, 3);
    assert.deepEqual(sceneQuery.structuredContent.results.map((entry) => entry.nodeId), ['root:home', 'section:hero']);
    assert.equal(sceneQuery.structuredContent.results.find((entry) => entry.nodeId === 'section:hero').parentId, 'root:home');

    const edited = await client.callTool({
      name: 'web_design_edit_scene',
      arguments: {
        documentId,
        transactionId: 'transaction:mcp-resize-root',
        expectedRevision: 3,
        reason: 'Give the desktop composition more horizontal room.',
        command: { type: 'resize', nodeId: 'root:home', handle: 'east', deltaX: 80, deltaY: 0 }
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
        transactionId: 'transaction:mcp-resize-root',
        expectedRevision: 3,
        reason: 'Give the desktop composition more horizontal room.',
        command: { type: 'resize', nodeId: 'root:home', handle: 'east', deltaX: 80, deltaY: 0 }
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
  } finally {
    await client.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
  }
});
