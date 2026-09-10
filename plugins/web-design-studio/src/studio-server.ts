import express from 'express';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { WebDesignDocumentStore, RevisionConflictError } from './document-store.js';
import { runtimeScopeFingerprint } from './runtime-scope.js';
import { assertWebDesignDocument, type WebDesignDocument } from './schema.js';
import { executeSceneEditorCommand } from './v2/scene-editor-command.js';
import { SceneDocumentStore, SceneRevisionConflictError } from './v2/scene-store.js';
import { AnnotationAiService } from './v2/annotation-ai-service.js';
import { GenerationVisualArtifactStore } from './v2/generation-visual-artifact-store.js';
import { GenerationVisualService } from './v2/generation-visual-service.js';
import { GenerationCandidateStore } from './v2/generation-candidate-store.js';
import { GenerationPlanRevisionConflictError, GenerationPlanStore } from './v2/generation-plan-store.js';
import { GenerationSoftProtectionStore } from './v2/generation-soft-protection-store.js';
import { ChromiumSceneImageRenderer } from './v2/headless-scene-renderer.js';
import { ProgressiveGenerationService } from './v2/progressive-generation-service.js';
import { parseWorkspaceArtboards, WorkspacePlacementStore } from './v2/workspace-placement-store.js';
import { parseWorkspaceCamera } from './v2/workspace-camera.js';

const port = Number.parseInt(process.env.CHATOS_PLUGIN_APP_PORT ?? process.env.WEB_DESIGN_STUDIO_PORT ?? '4188', 10);
const host = process.env.CHATOS_PLUGIN_APP_HOST ?? process.env.WEB_DESIGN_STUDIO_HOST ?? '127.0.0.1';
const store = new WebDesignDocumentStore();
await store.initialize();

const contextKind = process.env.CHATOS_CONTEXT_SCOPE ?? 'device';
const runtimeContext = {
  kind: contextKind,
  isolated: true,
  hasProjectContext: contextKind === 'project',
  ...(process.env.CHATOS_PROJECT_NAME ? { projectName: process.env.CHATOS_PROJECT_NAME } : {})
};
const scopeKey = runtimeScopeFingerprint(store.rootDirectory);
const defaultProject = await store.ensureScopedProject(
  scopeKey,
  contextKind === 'project' && process.env.CHATOS_PROJECT_ID
    ? process.env.CHATOS_PROJECT_NAME?.trim() || 'ChatOS 网站项目'
    : '公共网站设计'
);
const defaultProjectId = defaultProject.projectId;
const workspacePlacements = new WorkspacePlacementStore(store.rootDirectory);
const scenes = new SceneDocumentStore(store.rootDirectory);
const visualArtifacts = new GenerationVisualArtifactStore(store.rootDirectory);
const generationProjectId = process.env.CHATOS_PROJECT_ID ?? defaultProjectId;
const generationPlans = new GenerationPlanStore(store.rootDirectory);
const generationCandidates = new GenerationCandidateStore(store.rootDirectory);
const generationProtections = new GenerationSoftProtectionStore(store.rootDirectory);
const progressiveGeneration = new ProgressiveGenerationService({
  projectId: generationProjectId,
  repositories: {
    plans: generationPlans,
    scenes,
    candidates: generationCandidates,
    protections: generationProtections
  },
  assertDocumentInScope: async (documentId) => ({ name: (await store.readInScope(documentId, scopeKey)).title })
});
const visualService = new GenerationVisualService({
  projectId: generationProjectId,
  scenes,
  artifacts: visualArtifacts,
  renderer: new ChromiumSceneImageRenderer(),
  assertDocumentInScope: async (documentId) => { await store.readInScope(documentId, scopeKey); }
});
const annotationAiService = new AnnotationAiService({
  projectId: generationProjectId,
  scenes,
  visuals: visualService,
  assertDocumentInScope: async (documentId) => { await store.readInScope(documentId, scopeKey); }
});

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '50mb' }));

app.get('/api/health', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ok: true, service: 'web-design-studio', dataDirectory: store.rootDirectory });
});

app.get('/api/context', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.json({ ...runtimeContext, ...(defaultProjectId ? { defaultProjectId } : {}) });
});

app.get('/api/documents', async (_request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json({ items: await store.listInProject(defaultProjectId, scopeKey) });
  } catch (error) {
    next(error);
  }
});

app.get('/api/projects', async (_request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json({ items: await store.listProjects(scopeKey) });
  } catch (error) {
    next(error);
  }
});

app.get('/api/projects/:projectId', async (request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json(await store.readProjectInScope(request.params.projectId, scopeKey));
  } catch (error) {
    next(error);
  }
});

app.post('/api/documents', async (request, response, next) => {
  try {
    const title = typeof request.body?.title === 'string' ? request.body.title : undefined;
    response.status(201).json(await store.createInProject(defaultProjectId, title, request.body?.blank === true));
  } catch (error) {
    next(error);
  }
});

app.get('/api/documents/:documentId', async (request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json(await store.readInScope(request.params.documentId, scopeKey));
  } catch (error) {
    next(error);
  }
});

app.get('/api/scenes/:documentId', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    response.setHeader('Cache-Control', 'no-store');
    response.json(await scenes.read(request.params.documentId));
  } catch (error) {
    next(error);
  }
});

app.post('/api/scenes/:documentId/commands', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    response.json(await executeSceneEditorCommand(scenes, request.params.documentId, request.body, 'human'));
  } catch (error) {
    next(error);
  }
});

app.post('/api/scenes/:documentId/annotation-ai-context', async (request, response, next) => {
  try {
    response.json(await annotationAiService.prepare({
      documentId: request.params.documentId,
      nodeId: request.body?.nodeId,
      annotationId: request.body?.annotationId,
      viewportWidth: request.body?.viewportWidth,
      dependencyNodeIds: request.body?.dependencyNodeIds,
      padding: request.body?.padding
    }));
  } catch (error) {
    next(error);
  }
});

app.get('/api/scenes/:documentId/history', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    response.setHeader('Cache-Control', 'no-store');
    response.json(await scenes.history(request.params.documentId));
  } catch (error) {
    next(error);
  }
});

app.post('/api/scenes/:documentId/undo', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    if (!Number.isSafeInteger(request.body?.expectedRevision)) throw new Error('expectedRevision is required.');
    response.json(await scenes.undo(request.params.documentId, request.body.expectedRevision, 'human'));
  } catch (error) {
    next(error);
  }
});

app.post('/api/scenes/:documentId/redo', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    if (!Number.isSafeInteger(request.body?.expectedRevision)) throw new Error('expectedRevision is required.');
    response.json(await scenes.redo(request.params.documentId, request.body.expectedRevision, 'human'));
  } catch (error) {
    next(error);
  }
});

app.get('/api/generation/:documentId/plan', async (request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json(await progressiveGeneration.getPlan(request.params.documentId));
  } catch (error) {
    next(error);
  }
});

app.get('/api/generation/:documentId/steps/:stepId', async (request, response, next) => {
  try {
    response.setHeader('Cache-Control', 'no-store');
    response.json(await progressiveGeneration.inspectStep(
      request.params.documentId,
      request.params.stepId,
      typeof request.query.attemptId === 'string' ? request.query.attemptId : undefined
    ));
  } catch (error) {
    next(error);
  }
});

app.post('/api/generation/:documentId/steps/:stepId/accept', async (request, response, next) => {
  try {
    response.json(await progressiveGeneration.acceptStep(
      request.params.documentId,
      request.body?.expectedPlanRevision,
      request.params.stepId,
      request.body?.attemptId,
      request.body?.approveSoftProtectionConflicts === true
    ));
  } catch (error) {
    next(error);
  }
});

app.post('/api/generation/:documentId/steps/:stepId/reject', async (request, response, next) => {
  try {
    response.json(await progressiveGeneration.rejectStep(
      request.params.documentId,
      request.body?.expectedPlanRevision,
      request.params.stepId,
      request.body?.attemptId,
      request.body?.reason
    ));
  } catch (error) {
    next(error);
  }
});

app.post('/api/generation/:documentId/steps/:stepId/rollback', async (request, response, next) => {
  try {
    response.json(await progressiveGeneration.rollbackStep(
      request.params.documentId,
      request.body?.expectedPlanRevision,
      request.params.stepId
    ));
  } catch (error) {
    next(error);
  }
});

app.post('/api/generation/:documentId/pause', async (request, response, next) => {
  try {
    response.json(await progressiveGeneration.pause(request.params.documentId, request.body?.expectedPlanRevision));
  } catch (error) {
    next(error);
  }
});

app.post('/api/generation/:documentId/resume', async (request, response, next) => {
  try {
    response.json(await progressiveGeneration.resume(request.params.documentId, request.body?.expectedPlanRevision));
  } catch (error) {
    next(error);
  }
});

app.get('/api/generation/:documentId/artifacts/:artifactId/image', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    const image = await visualArtifacts.readImage(
      { projectId: generationProjectId, documentId: request.params.documentId },
      request.params.artifactId
    );
    response.setHeader('Cache-Control', 'private, no-store');
    response.type(image.mimeType).send(image.data);
  } catch (error) {
    next(error);
  }
});

app.get('/api/workspace/:documentId', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    response.setHeader('Cache-Control', 'no-store');
    response.json(await workspacePlacements.readOrCreate(scopeKey, request.params.documentId));
  } catch (error) {
    next(error);
  }
});

app.put('/api/workspace/:documentId/camera', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    const camera = parseWorkspaceCamera(request.body?.camera);
    if (!camera) throw new Error('Workspace camera is invalid.');
    response.json(await workspacePlacements.updateCamera(scopeKey, request.params.documentId, camera));
  } catch (error) {
    next(error);
  }
});

app.put('/api/workspace/:documentId/artboards', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    const artboards = parseWorkspaceArtboards(request.body?.artboards);
    if (!artboards) throw new Error('Workspace artboards are invalid.');
    response.json(await workspacePlacements.updateArtboards(scopeKey, request.params.documentId, artboards));
  } catch (error) {
    next(error);
  }
});

app.put('/api/documents/:documentId', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    const expectedRevision = request.body?.expectedRevision;
    const submitted: unknown = request.body?.document;
    if (!Number.isSafeInteger(expectedRevision)) throw new Error('expectedRevision is required.');
    assertWebDesignDocument(submitted);
    const document = submitted as WebDesignDocument;
    if (document.documentId !== request.params.documentId) throw new Error('Document identity mismatch.');
    response.json(await store.replace(document, expectedRevision));
  } catch (error) {
    next(error);
  }
});

app.delete('/api/documents/:documentId', async (request, response, next) => {
  try {
    await store.readInScope(request.params.documentId, scopeKey);
    await scenes.remove(request.params.documentId).catch((error) => {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    });
    await generationPlans.remove({ projectId: generationProjectId, documentId: request.params.documentId }).catch((error) => {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    });
    await store.remove(request.params.documentId);
    response.status(204).end();
  } catch (error) {
    next(error);
  }
});

const currentDirectory = path.dirname(fileURLToPath(import.meta.url));
const uiDirectory = path.resolve(currentDirectory, '../ui');
app.use(express.static(uiDirectory, {
  etag: false,
  lastModified: false,
  fallthrough: true,
  setHeaders(response) {
    response.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
    response.setHeader('Pragma', 'no-cache');
    response.setHeader('Expires', '0');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('Referrer-Policy', 'no-referrer');
  }
}));
app.get('*path', (_request, response) => {
  response.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
  response.setHeader('Pragma', 'no-cache');
  response.setHeader('Expires', '0');
  response.sendFile(path.join(uiDirectory, 'index.html'));
});

app.use((error: unknown, _request: express.Request, response: express.Response, _next: express.NextFunction) => {
  const revisionConflict = error instanceof RevisionConflictError || error instanceof SceneRevisionConflictError || error instanceof GenerationPlanRevisionConflictError;
  const status = revisionConflict ? 409 : (error as NodeJS.ErrnoException).code === 'ENOENT' ? 404 : 400;
  response.status(status).json({
    error: error instanceof Error ? error.message : String(error),
    ...(revisionConflict ? { actualRevision: error.actualRevision } : {})
  });
});

app.listen(port, host, () => {
  process.stdout.write(`Web Design Studio is available at http://${host}:${port}\n`);
});
