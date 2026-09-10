import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { applyMagicUiComponentVariant, createMagicUiComponent, variantsForMagicUiComponent } from '../dist/magicui-library.test.mjs';
import { applySpellComponentVariant, createSpellComponent, variantsForSpellComponent } from '../dist/spell-library.test.mjs';
import { applyInspiraComponentVariant, createInspiraComponent } from '../dist/inspira-library.test.mjs';
import { applyDaisyUiComponentVariant, createDaisyUiComponent } from '../dist/daisyui-library.test.mjs';
import { createBlankSceneDocument, createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { SceneDocumentStore } from '../dist/v2-scene-store.test.mjs';
import { createGenerationSitePlan } from '../dist/v2-generation-plan-schema.test.mjs';
import { GenerationPlanStore } from '../dist/v2-generation-plan-store.test.mjs';

test('studio serves the packaged workbench and persists a design', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-studio-server-test-'));
  const port = await availablePort();
  const child = spawn(process.execPath, ['bin/chatos-web-design-studio', 'studio'], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      WEB_DESIGN_STUDIO_HOST: '127.0.0.1',
      WEB_DESIGN_STUDIO_PORT: String(port),
      WEB_DESIGN_STUDIO_DATA_DIR: root,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'host-project-through-123',
      CHATOS_PROJECT_NAME: '宿主产品项目',
      CHATOS_WORKSPACE_ID: 'workspace-through-456'
    },
    stdio: ['ignore', 'pipe', 'pipe']
  });
  try {
    await waitForReady(child, port);
    const base = `http://127.0.0.1:${port}`;
    const page = await fetch(base);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Web Design Studio/);

    const context = await fetch(`${base}/api/context`).then((response) => response.json());
    assert.equal(context.kind, 'project');
    assert.equal(context.isolated, true);
    assert.equal(context.hasProjectContext, true);
    assert.equal(context.projectName, '宿主产品项目');
    assert.equal(Object.hasOwn(context, 'chatosProjectId'), false);
    assert.equal(Object.hasOwn(context, 'workspaceId'), false);
    assert.ok(context.defaultProjectId);

    const projects = await fetch(`${base}/api/projects`).then((response) => response.json());
    assert.equal(projects.items.length, 1);
    assert.equal(projects.items[0].projectId, context.defaultProjectId);
    assert.equal(projects.items[0].name, '宿主产品项目');

    const projectMutation = await fetch(`${base}/api/projects`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: '不应创建的内部项目' })
    });
    assert.equal(projectMutation.status, 404);

    const projectDesign = await fetch(`${base}/api/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: '项目内空白网站', blank: true })
    }).then((response) => response.json());
    assert.equal(projectDesign.components.length, 0);
    const projectAfterCreate = await fetch(`${base}/api/projects/${context.defaultProjectId}`).then((response) => response.json());
    assert.deepEqual(projectAfterCreate.designIds, [projectDesign.documentId]);

    const created = await fetch(`${base}/api/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: '本地网站设计' })
    }).then((response) => response.json());
    assert.equal(created.revision, 1);
    const stored = JSON.parse(await readFile(path.join(root, `${created.documentId}.web-design.json`), 'utf8'));
    assert.equal(stored.title, '本地网站设计');

    const missingPlan = await fetch(`${base}/api/generation/${projectDesign.documentId}/plan`);
    assert.equal(missingPlan.status, 404);
    const generationPlanStore = new GenerationPlanStore(root);
    await generationPlanStore.create(createGenerationSitePlan({
      planId: 'plan:studio-review',
      scope: { projectId: 'host-project-through-123', documentId: created.documentId },
      mode: 'guided',
      objective: 'Create a visually distinctive product website one reviewed step at a time',
      audience: ['Design reviewers'],
      pages: [{ pageId: created.pages[0].id, name: '首页', purpose: '建立品牌视觉与核心价值' }]
    }));
    const generationPlanResponse = await fetch(`${base}/api/generation/${created.documentId}/plan`);
    assert.equal(generationPlanResponse.status, 200);
    const generationPlan = await generationPlanResponse.json();
    assert.equal(generationPlan.plan.planId, 'plan:studio-review');
    assert.equal(generationPlan.plan.objective, 'Create a visually distinctive product website one reviewed step at a time');
    assert.equal(generationPlan.plan.pages.length, 1);

    const sceneStore = new SceneDocumentStore(root);
    const sceneSource = createBlankSceneDocument('本地网站设计');
    sceneSource.documentId = created.documentId;
    sceneSource.pages[0].id = created.pages[0].id;
    sceneSource.pages[0].children = [{
      ...createSceneNodeBase('frame', 'Page root', { x: 0, y: 0, width: 1440, height: 900 }),
      id: 'frame:studio-root',
      children: [
        { ...createSceneNodeBase('shape', 'First', { x: 80, y: 100, width: 160, height: 100 }), id: 'shape:studio-first', shape: 'rectangle' },
        { ...createSceneNodeBase('shape', 'Second', { x: 300, y: 100, width: 160, height: 100 }), id: 'shape:studio-second', shape: 'rectangle' }
      ]
    }];
    const createdScene = await sceneStore.create(sceneSource);
    const sceneReadResponse = await fetch(`${base}/api/scenes/${created.documentId}`);
    assert.equal(sceneReadResponse.status, 200);
    const sceneRead = await sceneReadResponse.json();
    assert.equal(sceneRead.revision, createdScene.revision);
    assert.equal(sceneRead.pages[0].children[0].id, 'frame:studio-root');
    const groupedResponse = await fetch(`${base}/api/scenes/${created.documentId}/commands`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        transactionId: 'studio:group-pair', expectedRevision: createdScene.revision,
        command: {
          type: 'group', nodeIds: ['shape:studio-first', 'shape:studio-second'],
          wrapperId: 'group:studio-pair', name: 'Studio pair'
        }
      })
    });
    assert.equal(groupedResponse.status, 200);
    const groupedScene = await groupedResponse.json();
    assert.equal(groupedScene.document.revision, 2);
    assert.equal(groupedScene.commandType, 'group');
    assert.equal(groupedScene.recovered, false);
    assert.deepEqual(groupedScene.summary.insertedNodeIds, ['group:studio-pair']);
    const staleSceneEdit = await fetch(`${base}/api/scenes/${created.documentId}/commands`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        transactionId: 'studio:stale-move', expectedRevision: 1,
        command: { type: 'move', nodeIds: ['group:studio-pair'], deltaX: 10, deltaY: 0 }
      })
    });
    assert.equal(staleSceneEdit.status, 409);
    assert.equal((await staleSceneEdit.json()).actualRevision, 2);
    const sceneHistory = await fetch(`${base}/api/scenes/${created.documentId}/history`).then((response) => response.json());
    assert.equal(sceneHistory.undoCount, 1);
    assert.equal(sceneHistory.redoCount, 0);
    const undoneScene = await fetch(`${base}/api/scenes/${created.documentId}/undo`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ expectedRevision: 2 })
    }).then((response) => response.json());
    assert.equal(undoneScene.revision, 3);
    assert.deepEqual(undoneScene.pages[0].children[0].children.map((node) => node.id), ['shape:studio-first', 'shape:studio-second']);
    const redoneScene = await fetch(`${base}/api/scenes/${created.documentId}/redo`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ expectedRevision: 3 })
    }).then((response) => response.json());
    assert.equal(redoneScene.revision, 4);
    assert.equal(redoneScene.pages[0].children[0].children[0].id, 'group:studio-pair');
    const annotatedSceneResponse = await fetch(`${base}/api/scenes/${created.documentId}/commands`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        transactionId: 'studio:add-visual-note', expectedRevision: redoneScene.revision,
        command: {
          type: 'add-annotation', nodeId: 'group:studio-pair', annotationId: 'annotation:studio-contrast',
          body: 'Increase the visual contrast of this pair.'
        }
      })
    });
    assert.equal(annotatedSceneResponse.status, 200);
    const annotatedScene = await annotatedSceneResponse.json();
    assert.equal(annotatedScene.document.revision, 5);
    assert.equal(annotatedScene.document.pages[0].children[0].children[0].annotations[0].author, 'human');
    const annotationContextResponse = await fetch(`${base}/api/scenes/${created.documentId}/annotation-ai-context`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        nodeId: 'group:studio-pair', annotationId: 'annotation:studio-contrast', viewportWidth: 1440
      })
    });
    assert.equal(annotationContextResponse.status, 200);
    const annotationContext = await annotationContextResponse.json();
    assert.equal(annotationContext.task.scope.projectId, 'host-project-through-123');
    assert.equal(annotationContext.task.pageId, created.pages[0].id);
    assert.equal(annotationContext.task.targetNodeId, 'group:studio-pair');
    assert.equal(annotationContext.task.baseRevision, annotatedScene.document.revision);
    assert.equal(annotationContext.visualContext.capture.artifact.revision, annotatedScene.document.revision);
    assert.equal(annotationContext.__images[0].mimeType, 'image/png');
    assert.ok(annotationContext.__images[0].data.length > 100);

    const initialWorkspace = await fetch(`${base}/api/workspace/${created.documentId}`).then((response) => response.json());
    assert.deepEqual(initialWorkspace.camera, { x: 0, y: 0, zoom: 1 });
    const movedWorkspace = await fetch(`${base}/api/workspace/${created.documentId}/camera`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ camera: { x: -3840, y: 5120, zoom: 3.25 } })
    }).then((response) => response.json());
    assert.deepEqual(movedWorkspace.camera, { x: -3840, y: 5120, zoom: 3.25 });
    assert.deepEqual(
      (await fetch(`${base}/api/workspace/${created.documentId}`).then((response) => response.json())).camera,
      movedWorkspace.camera
    );
    assert.equal(JSON.parse(await readFile(path.join(root, `${created.documentId}.web-design.json`), 'utf8')).revision, created.revision);
    const artboardWorkspace = await fetch(`${base}/api/workspace/${created.documentId}/artboards`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ artboards: [
        { artboardId: 'home-page', pageId: 'home', surfaceKind: 'page', viewportWidth: 1440, viewportHeight: 900, x: 0, y: 0 },
        { artboardId: 'sign-in-modal', pageId: 'sign-in', surfaceKind: 'modal', viewportWidth: 720, viewportHeight: 720, x: 1600, y: 0 }
      ] })
    }).then((response) => response.json());
    assert.equal(artboardWorkspace.artboards.length, 2);
    assert.deepEqual(artboardWorkspace.camera, movedWorkspace.camera);
    assert.equal(JSON.parse(await readFile(path.join(root, `${created.documentId}.web-design.json`), 'utf8')).revision, created.revision);

    const exactDesign = structuredClone(created);
    exactDesign.viewport.width = 2560;
    exactDesign.viewport.height = 2300;
    exactDesign.breakpoints.desktop = { width: 2560, height: 2300, preview: { presetId: 'desktop-qhd', orientation: 'default', viewportHeight: 1440 } };
    const form = exactDesign.components[0];
    form.x = 928.25;
    form.y = 1478.75;
    form.width = 480.5;
    form.height = 720.25;
    form.style = { ...form.style, background: 'rgba(255,255,255,.83)', borderRadius: 17.5 };
    form.responsive = { mobile: { x: 21.5, y: 812.25, width: 347.5, height: 508.75 } };
    const magicCard = applyMagicUiComponentVariant(createMagicUiComponent('MagicCard', 1480.25, 620.75), variantsForMagicUiComponent('MagicCard')[0].id);
    magicCard.pageId = exactDesign.pages[0].id;
    magicCard.width = 512.5;
    magicCard.height = 286.25;
    magicCard.responsive = { mobile: { x: 18.25, y: 1420.5, width: 354.75, height: 236.5 } };
    exactDesign.components.push(magicCard);
    const spellChart = applySpellComponentVariant(createSpellComponent('Chart', 720.5, 1860.25), variantsForSpellComponent('Chart')[0].id);
    spellChart.pageId = exactDesign.pages[0].id;
    spellChart.library.props.values = [21, 35, 29, 64, 73, 91];
    exactDesign.components.push(spellChart);
    const inspiraUpload = applyInspiraComponentVariant(createInspiraComponent('FileUpload', 1320.75, 1940.5), 'inspira-file-upload-basic');
    inspiraUpload.pageId = exactDesign.pages[0].id;
    inspiraUpload.width = 518.25;
    inspiraUpload.height = 346.75;
    inspiraUpload.library.props.files = ['brand-system.fig', 'launch-visual.pdf'];
    inspiraUpload.responsive = { mobile: { x: 18.5, y: 1710.25, width: 352.75, height: 318.5 } };
    exactDesign.components.push(inspiraUpload);
    const daisyCard = applyDaisyUiComponentVariant(createDaisyUiComponent('Card', 1900.25, 2010.75), 'card-14');
    daisyCard.pageId = exactDesign.pages[0].id;
    daisyCard.width = 548.5;
    daisyCard.height = 312.25;
    daisyCard.library.props.items = ['Design system', 'Responsive states', 'Publish'];
    daisyCard.responsive = { mobile: { x: 17.25, y: 2058.5, width: 355.25, height: 302.75 } };
    exactDesign.components.push(daisyCard);
    const saved = await fetch(`${base}/api/documents/${created.documentId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ document: exactDesign, expectedRevision: created.revision })
    }).then((response) => response.json());
    const reopened = await fetch(`${base}/api/documents/${created.documentId}`).then((response) => response.json());
    assert.deepEqual(reopened.components, exactDesign.components);
    assert.deepEqual(reopened.viewport, exactDesign.viewport);
    assert.deepEqual(reopened.breakpoints, exactDesign.breakpoints);
    assert.deepEqual(saved.components, exactDesign.components);
    assert.deepEqual(reopened.components.find((component) => component.id === magicCard.id), magicCard);
    assert.deepEqual(reopened.components.find((component) => component.id === spellChart.id), spellChart);
    assert.deepEqual(reopened.components.find((component) => component.id === inspiraUpload.id), inspiraUpload);
    assert.deepEqual(reopened.components.find((component) => component.id === daisyCard.id), daisyCard);
  } finally {
    if (child.exitCode === null) {
      child.kill('SIGTERM');
      await new Promise((resolve) => child.once('exit', resolve));
    }
    await rm(root, { recursive: true, force: true });
  }
});

async function availablePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function waitForReady(child, port) {
  const deadline = Date.now() + 8000;
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Studio exited before startup: ${stderr}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/api/health`);
      if (response.ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out waiting for Web Design Studio: ${stderr}`);
}
