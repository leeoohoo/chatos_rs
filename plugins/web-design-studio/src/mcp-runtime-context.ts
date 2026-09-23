import { WebDesignDocumentStore } from './document-store.js';
import { runtimeScopeFingerprint } from './runtime-scope.js';
import { GenerationCandidateStore } from './v2/generation-candidate-store.js';
import { GenerationPlanStore } from './v2/generation-plan-store.js';
import { GenerationSoftProtectionStore } from './v2/generation-soft-protection-store.js';
import { GenerationVisualArtifactStore } from './v2/generation-visual-artifact-store.js';
import { GenerationVisualService, type ToolImagePayload } from './v2/generation-visual-service.js';
import { ChromiumSceneImageRenderer } from './v2/headless-scene-renderer.js';
import { AnnotationAiService } from './v2/annotation-ai-service.js';
import { ProgressiveGenerationService } from './v2/progressive-generation-service.js';
import { indexSceneDocument } from './v2/scene-schema.js';
import { SceneDocumentStore } from './v2/scene-store.js';
import { isMissingFileError } from './mcp-tool-helpers.js';
import { TOOL_DEFINITIONS_BASE } from './mcp-tool-definitions.js';
import type { GenerationArtifact } from './v2/generation-plan-schema.js';

export const store = new WebDesignDocumentStore();
await store.initialize();
export const scopeKey = runtimeScopeFingerprint(store.rootDirectory);
export const defaultProject = await store.ensureScopedProject(
  scopeKey,
  process.env.CHATOS_CONTEXT_SCOPE === 'project' && process.env.CHATOS_PROJECT_ID
    ? process.env.CHATOS_PROJECT_NAME?.trim() || 'ChatOS 网站项目'
    : '公共网站设计',
  { consolidateDefaultProjects: process.env.CHATOS_CONTEXT_SCOPE === 'project' }
);
export const generationRepositories = {
  plans: new GenerationPlanStore(store.rootDirectory),
  scenes: new SceneDocumentStore(store.rootDirectory),
  candidates: new GenerationCandidateStore(store.rootDirectory),
  protections: new GenerationSoftProtectionStore(store.rootDirectory),
  visualArtifacts: new GenerationVisualArtifactStore(store.rootDirectory)
};

export async function assertGenerationDocumentInScope(documentId: string): Promise<{ name: string }> {
  const document = await store.readInScope(documentId, scopeKey);
  return { name: document.title };
}

export function progressiveGenerationService(): ProgressiveGenerationService {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Progressive website generation requires a ChatOS project context with a host-injected projectId.');
  const visuals = generationVisualService();
  return new ProgressiveGenerationService({
    projectId,
    repositories: generationRepositories,
    assertDocumentInScope: assertGenerationDocumentInScope,
    verifyCandidate: (input) => visuals.verifyCandidate(input),
    captureVisualInputs: async ({ documentId, pageId, viewportWidths }) => {
      const captures = await Promise.all(viewportWidths.map((viewportWidth) => visuals.capturePage(documentId, pageId, viewportWidth)));
      return captures.flatMap((capture) => (capture.artifacts as GenerationArtifact[])
        .filter((artifact) => artifact.kind === 'page-snapshot' || artifact.kind === 'visual-grounding'));
    },
    loadArtifactImages: (scope, artifacts) => visuals.loadArtifactImages(scope.documentId, artifacts)
  });
}

export function generationVisualService(): GenerationVisualService {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Visual website inspection requires a ChatOS project context with a host-injected projectId.');
  return new GenerationVisualService({
    projectId,
    scenes: generationRepositories.scenes,
    artifacts: generationRepositories.visualArtifacts,
    renderer: new ChromiumSceneImageRenderer(),
    assertDocumentInScope: async (documentId) => { await assertGenerationDocumentInScope(documentId); }
  });
}

export function annotationAiService(): AnnotationAiService {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Annotation AI tasks require a ChatOS project context with a host-injected projectId.');
  return new AnnotationAiService({
    projectId,
    scenes: generationRepositories.scenes,
    visuals: generationVisualService(),
    assertDocumentInScope: async (documentId) => { await assertGenerationDocumentInScope(documentId); }
  });
}

export function webDesignToolSkills(name: string): string[] {
  if (name === 'web_design_get_active_context' || name === 'web_design_plan_site' || name === 'web_design_plan_page') return ['web-design-planning'];
  if (name === 'web_design_execute_step' || name === 'web_design_query_scene' || name === 'web_design_edit_scene') return ['web-design-scene-building'];
  if (name === 'web_design_control_plan' || name === 'web_design_capture_page' || name === 'web_design_capture_region'
    || name === 'web_design_compare_snapshots' || name === 'web_design_inspect_at_point'
    || name === 'web_design_prepare_annotation_task') return ['web-design-candidate-review'];
  if (name === 'web_design_replace_document'
    || name === 'web_design_insert_section' || name === 'web_design_apply_page_template') {
    return ['web-design-components', 'web-design-responsive-layout', 'web-design-visual-system'];
  }
  if (name === 'web_design_apply_patch' || name === 'web_design_apply_node_batch') return ['web-design-components', 'web-design-responsive-layout'];
  if (name === 'web_design_get_node') return ['web-design-components'];
  if (name.includes('auto_layout')) return ['web-design-responsive-layout'];
  if (name.includes('catalog') || name.includes('component') || name.includes('symbol')) return ['web-design-components'];
  if (name.includes('export') || name.includes('validate')) return ['web-design-validation-export'];
  return ['web-design-documents'];
}

export const SCENE_V3_TOOL_NAMES = new Set([
  'web_design_get_active_context',
  'web_design_plan_site', 'web_design_plan_page',
  'web_design_capture_page', 'web_design_capture_region', 'web_design_prepare_annotation_task',
  'web_design_compare_snapshots', 'web_design_inspect_at_point',
  'web_design_query_scene', 'web_design_edit_scene',
  'web_design_execute_step', 'web_design_control_plan',
  'web_design_list_documents', 'web_design_create_document',
  'web_design_get_catalog', 'web_design_search_catalog', 'web_design_get_component_contract',
  'web_design_list_requests'
]);

export const TOOL_DEFINITIONS = TOOL_DEFINITIONS_BASE.filter((tool) => SCENE_V3_TOOL_NAMES.has(tool.name)).map((tool) => ({
  ...tool,
  _meta: {
    ...tool._meta,
    'chatos/skillGate': {
      allOf: ['web-design-studio', ...webDesignToolSkills(tool.name)]
    }
  }
}));

export async function requestEntries(documentId: string, includeResolved: boolean) {
  const document = await store.readInScope(documentId, scopeKey);
  let sceneEntries: Array<Record<string, unknown>> = [];
  try {
    const scene = await generationRepositories.scenes.read(documentId);
    const index = indexSceneDocument(scene);
    sceneEntries = [...index.values()].flatMap((entry) => entry.node.annotations
      .filter((annotation) => includeResolved || annotation.status === 'open')
      .map((annotation) => ({
        kind: 'scene-annotation',
        documentId,
        documentTitle: document.title,
        revision: scene.revision,
        pageId: entry.pageId,
        request: {
          id: annotation.id,
          nodeId: entry.node.id,
          instruction: annotation.body,
          status: annotation.status === 'open' ? 'pending' : 'resolved',
          author: annotation.author,
          createdAt: annotation.createdAt,
          ...(annotation.resolvedAt ? { resolvedAt: annotation.resolvedAt } : {})
        },
        target: {
          id: entry.node.id,
          name: entry.node.name,
          type: entry.node.type,
          role: entry.node.role,
          frame: entry.node.frame
        },
        prepareWith: {
          tool: 'web_design_prepare_annotation_task',
          arguments: { documentId, nodeId: entry.node.id, annotationId: annotation.id, viewportWidth: 1440 }
        }
      })));
  } catch (error) {
    if (!isMissingFileError(error)) throw error;
  }
  return sceneEntries;
}

export function activeSelectionIds(): string[] {
  const source = process.env.CHATOS_ACTIVE_SELECTION?.trim();
  if (!source) return [];
  try {
    const parsed: unknown = JSON.parse(source);
    if (Array.isArray(parsed) && parsed.every((item) => typeof item === 'string')) return [...new Set(parsed)];
  } catch {
    // Comma-separated IDs are accepted as a small host interoperability fallback.
  }
  return [...new Set(source.split(',').map((item) => item.trim()).filter(Boolean))];
}

export async function activeProgressiveContext(): Promise<Record<string, unknown>> {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Web Design Studio is not running in a ChatOS project context.');
  const documents = await store.listInProject(defaultProject.projectId, scopeKey);
  const injectedDocumentId = process.env.CHATOS_ACTIVE_DOCUMENT_ID?.trim();
  const documentId = injectedDocumentId || (documents.length === 1 ? documents[0].documentId : undefined);
  if (documentId && !documents.some((document) => document.documentId === documentId)) {
    throw new Error('The host-injected active document is outside the current ChatOS scope.');
  }
  const pageId = process.env.CHATOS_ACTIVE_PAGE_ID?.trim() || undefined;
  let pendingRequests: Awaited<ReturnType<typeof requestEntries>> = [];
  let plan: Record<string, unknown> | undefined;
  let artboardDirectory: Array<Record<string, unknown>> = [];
  let resumeReview: Record<string, unknown> | undefined;
  let resumeImages: ToolImagePayload[] = [];
  if (documentId) {
    pendingRequests = await requestEntries(documentId, false);
    try {
      const context = await progressiveGenerationService().getActiveContext(documentId);
      plan = context.plan as Record<string, unknown>;
      resumeReview = context.resumeReview as Record<string, unknown> | undefined;
      resumeImages = Array.isArray(context.__images) ? context.__images as ToolImagePayload[] : [];
    }
    catch (error) { if (!isMissingFileError(error)) throw error; }
    try {
      const scene = await generationRepositories.scenes.read(documentId);
      artboardDirectory = scene.pages.map((page, index) => ({
        artboardId: page.id,
        name: page.name,
        order: index,
        rootNodeIds: page.children.map((node) => node.id)
      }));
    } catch (error) { if (!isMissingFileError(error)) throw error; }
  }
  const plannedActivePageId = plan?.activePage && typeof plan.activePage === 'object'
    ? String((plan.activePage as Record<string, unknown>).pageId ?? '') || undefined
    : undefined;
  const activePageId = pageId ?? plannedActivePageId;
  const requiredNextAction = plan
    ? plan.nextAction
    : documentId
      ? { type: 'plan-site', tool: 'web_design_plan_site', documentId }
      : { type: 'select-or-create-document', tool: 'web_design_list_documents' };
  return {
    scope: { projectId, kind: process.env.CHATOS_CONTEXT_SCOPE ?? 'project' },
    active: { documentId, pageId: activePageId, selectionNodeIds: activeSelectionIds() },
    documents,
    artboardDirectory,
    pendingRequests,
    ...(plan ? { plan } : {}),
    ...(resumeReview ? { resumeReview } : {}),
    nextAction: requiredNextAction,
    deliveryGate: plan?.deliveryGate ?? {
      status: 'blocked',
      code: documentId ? 'NO_SITE_PLAN' : 'NO_DOCUMENT',
      message: documentId
        ? 'The document has no generation plan and no accepted visible Scene work. Plan the site and continue its required next action before editing product UI code or reporting a task outcome.'
        : 'No design document is active. Select or create one and continue until a visible Scene Candidate is accepted before editing product UI code or reporting a task outcome.',
      visibleSceneReady: false,
      projectImplementationAllowed: false,
      taskCompletionAllowed: false,
      acceptedVisibleStepCount: 0,
      completedArtboardCount: 0,
      plannedArtboardCount: 0,
      requiredNextAction
    },
    ...(resumeImages.length > 0 ? { __images: resumeImages } : {})
  };
}
