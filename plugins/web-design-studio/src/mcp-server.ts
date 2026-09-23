import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { RevisionConflictError, WebDesignDocumentStore } from './document-store.js';
import { exportDocumentHtmlFiles } from './html-exporter.js';
import { exportReactComponent } from './react-exporter.js';
import { exportVueComponent } from './vue-exporter.js';
import { editableSlotsForUiComponent } from './library-slots.js';
import { createComponentFromUiLibrary, UI_LIBRARIES } from './ui-libraries.js';
import { WEB_DESIGN_THEME_PRESETS } from './design-themes.js';
import { WEB_DESIGN_BLOCK_PRESETS, WEB_DESIGN_PAGE_TEMPLATES } from './component-library.js';
import { runtimeScopeFingerprint } from './runtime-scope.js';
import { GenerationCandidateStore } from './v2/generation-candidate-store.js';
import { GenerationPlanRevisionConflictError, GenerationPlanStore } from './v2/generation-plan-store.js';
import { GenerationSoftProtectionStore } from './v2/generation-soft-protection-store.js';
import { GenerationVisualArtifactStore } from './v2/generation-visual-artifact-store.js';
import { GenerationVisualService, type ToolImagePayload } from './v2/generation-visual-service.js';
import { ChromiumSceneImageRenderer } from './v2/headless-scene-renderer.js';
import { AnnotationAiService } from './v2/annotation-ai-service.js';
import { ProgressiveGenerationService } from './v2/progressive-generation-service.js';
import { executeSceneEditorCommand, type SceneEditorCommand } from './v2/scene-editor-command.js';
import { jsonEncodedValueSchema, jsonScalarValueSchema, stringLiteralSchema } from './json-schema.js';
import { SceneQueryIndex, type SceneQuery } from './v2/scene-query.js';
import { createSceneNodeBase, indexSceneDocument, type SceneDocument, type SceneNode, type SceneNodeType } from './v2/scene-schema.js';
import { SceneDocumentStore, SceneRevisionConflictError } from './v2/scene-store.js';
import type { CreateGenerationStepInput, GenerationArtifact, GenerationDesignIntent } from './v2/generation-plan-schema.js';
import type { SceneTransactionOperation } from './v2/scene-transaction.js';
import {
  assertHandoffQuality,
  componentPageId,
  pageOutline,
  validateWebDesignDocument
} from './design-quality.js';
import {
  assertWebDesignDocument,
  designSummary,
  pageIdForComponent,
  pagesForDocument,
  type WebDesignDocument,
  type WebDesignComponent,
  type WebDesignPatchOperation
} from './schema.js';
import { TOOL_DEFINITIONS_BASE } from './mcp-tool-definitions.js';
import {
  store,
  scopeKey,
  defaultProject,
  generationRepositories,
  progressiveGenerationService,
  generationVisualService,
  annotationAiService,
  requestEntries,
  activeProgressiveContext
  ,assertGenerationDocumentInScope
  ,TOOL_DEFINITIONS
} from './mcp-runtime-context.js';
import {
  objectArguments,
  decodeStructuredJson,
  simpleSceneNode,
  simpleSceneTree,
  normalizeGenerationOperations,
  changedComponentIds,
  compactMutationResult,
  assertFocusedOperations,
  changedIdsBetween,
  isMissingFileError,
  assertSceneCommandInArtboard
} from './mcp-tool-helpers.js';

const SERVER_NAME = 'chatos-web-design-studio';
const SERVER_VERSION = '3.0.24';
async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  switch (name) {
    case 'web_design_get_active_context':
      return activeProgressiveContext();
    case 'web_design_plan_site':
      return progressiveGenerationService().planSite({
        documentId: String(argumentsValue.documentId),
        expectedPlanRevision: typeof argumentsValue.expectedPlanRevision === 'number' ? argumentsValue.expectedPlanRevision : undefined,
        planId: typeof argumentsValue.planId === 'string' ? argumentsValue.planId : undefined,
        mode: typeof argumentsValue.mode === 'string' ? argumentsValue.mode as 'guided' | 'auto-current-page' | 'review-sensitive' : undefined,
        objective: String(argumentsValue.objective),
        audience: argumentsValue.audience as string[],
        pages: argumentsValue.pages as Array<{ pageId: string; name: string; purpose: string }>
      });
    case 'web_design_plan_page':
      return progressiveGenerationService().planPage({
        documentId: String(argumentsValue.documentId),
        expectedPlanRevision: Number(argumentsValue.expectedPlanRevision),
        pageId: String(argumentsValue.pageId),
        design: argumentsValue.design as GenerationDesignIntent,
        steps: argumentsValue.steps as CreateGenerationStepInput[]
      });
    case 'web_design_get_plan':
      return progressiveGenerationService().getPlan(String(argumentsValue.documentId));
    case 'web_design_start_page':
      return progressiveGenerationService().startPage(
        String(argumentsValue.documentId),
        Number(argumentsValue.expectedPlanRevision),
        String(argumentsValue.pageId),
        typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440
      );
    case 'web_design_capture_page':
      return generationVisualService().capturePage(
        String(argumentsValue.documentId), String(argumentsValue.pageId),
        typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440
      );
    case 'web_design_capture_region':
      return generationVisualService().captureRegion({
        documentId: String(argumentsValue.documentId),
        pageId: String(argumentsValue.pageId),
        viewportWidth: typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440,
        ...(typeof argumentsValue.nodeId === 'string' ? { nodeId: argumentsValue.nodeId } : {}),
        ...(argumentsValue.rect && typeof argumentsValue.rect === 'object' ? { rect: argumentsValue.rect as { x: number; y: number; width: number; height: number } } : {}),
        ...(typeof argumentsValue.padding === 'number' ? { padding: argumentsValue.padding } : {})
      });
    case 'web_design_prepare_annotation_task':
      return annotationAiService().prepare({
        documentId: String(argumentsValue.documentId),
        nodeId: String(argumentsValue.nodeId),
        annotationId: String(argumentsValue.annotationId),
        viewportWidth: typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440,
        ...(Array.isArray(argumentsValue.dependencyNodeIds) ? { dependencyNodeIds: argumentsValue.dependencyNodeIds as string[] } : {}),
        ...(typeof argumentsValue.padding === 'number' ? { padding: argumentsValue.padding } : {})
      });
    case 'web_design_get_visual_grounding':
      return generationVisualService().getVisualGrounding(String(argumentsValue.documentId), String(argumentsValue.artifactId));
    case 'web_design_compare_snapshots':
      return generationVisualService().compareSnapshots(
        String(argumentsValue.documentId), String(argumentsValue.beforeArtifactId), String(argumentsValue.afterArtifactId)
      );
    case 'web_design_inspect_at_point':
      return generationVisualService().inspectAtPoint(
        String(argumentsValue.documentId), String(argumentsValue.artifactId), Number(argumentsValue.x), Number(argumentsValue.y),
        typeof argumentsValue.limit === 'number' ? argumentsValue.limit : 12
      );
    case 'web_design_query_scene': {
      const documentId = String(argumentsValue.documentId);
      const artboardId = String(argumentsValue.artboardId);
      await assertGenerationDocumentInScope(documentId);
      const scene = await generationRepositories.scenes.read(documentId);
      const query = argumentsValue.query && typeof argumentsValue.query === 'object'
        ? structuredClone(argumentsValue.query) as SceneQuery
        : { limit: 100 };
      if (!scene.pages.some((page) => page.id === artboardId)) throw new Error(`Artboard not found: ${artboardId}`);
      query.pageIds = [artboardId];
      if (query.limit === undefined) query.limit = 100;
      const results = new SceneQueryIndex(scene).query(query);
      return {
        scene: {
          documentId: scene.documentId,
          name: scene.name,
          revision: scene.revision,
          activeArtboard: {
            artboardId,
            name: scene.pages.find((page) => page.id === artboardId)?.name,
            rootNodeIds: scene.pages.find((page) => page.id === artboardId)?.children.map((node) => node.id) ?? []
          }
        },
        resultCount: results.length,
        results
      };
    }
    case 'web_design_edit_scene': {
      const documentId = String(argumentsValue.documentId);
      const artboardId = String(argumentsValue.artboardId);
      await assertGenerationDocumentInScope(documentId);
      const currentScene = await generationRepositories.scenes.read(documentId);
      const decodedCommand = decodeStructuredJson(argumentsValue.commandJson, 'commandJson');
      if (Array.isArray(decodedCommand)) throw new Error('commandJson must encode one command object.');
      const request = {
        transactionId: String(argumentsValue.transactionId),
        expectedRevision: Number(argumentsValue.expectedRevision),
        ...(typeof argumentsValue.reason === 'string' ? { reason: argumentsValue.reason } : {}),
        command: decodedCommand
      };
      assertSceneCommandInArtboard(currentScene, decodedCommand as SceneEditorCommand, artboardId);
      const edited = await executeSceneEditorCommand(generationRepositories.scenes, documentId, request, 'ai');
      const changedNodeIds = [...new Set([
        ...edited.summary.insertedNodeIds,
        ...edited.summary.updatedNodeIds,
        ...edited.summary.movedNodeIds,
        ...edited.summary.removedNodeIds
      ])];
      const remaining = changedNodeIds.length > 0
        ? new SceneQueryIndex(edited.document).query({ ids: changedNodeIds, limit: 256 })
        : [];
      const affectedPageIds = [...new Set(remaining.map((entry) => entry.pageId))];
      return {
        scene: { documentId, revision: edited.document.revision },
        commandType: edited.commandType,
        transaction: edited.summary,
        recovered: edited.recovered,
        affectedNodeIds: changedNodeIds,
        affectedPageIds,
        nextRecommendedActions: [
          ...(changedNodeIds.length > 0 ? [{ tool: 'web_design_query_scene', arguments: { documentId, query: { ids: changedNodeIds } } }] : []),
          ...affectedPageIds.slice(0, 4).map((pageId) => ({ tool: 'web_design_capture_page', arguments: { documentId, pageId, viewportWidth: 1440 } }))
        ]
      };
    }
    case 'web_design_execute_step':
      {
      const decodedOperations = decodeStructuredJson(argumentsValue.operationsJson, 'operationsJson');
      if (!Array.isArray(decodedOperations)) throw new Error('operationsJson must encode an operation array.');
      return progressiveGenerationService().executeStep({
        documentId: String(argumentsValue.documentId),
        expectedPlanRevision: Number(argumentsValue.expectedPlanRevision),
        stepId: typeof argumentsValue.stepId === 'string' ? argumentsValue.stepId : undefined,
        requestId: typeof argumentsValue.requestId === 'string' ? argumentsValue.requestId : undefined,
        operations: normalizeGenerationOperations(decodedOperations)
      });
      }
    case 'web_design_control_plan': {
      const service = progressiveGenerationService();
      const documentId = String(argumentsValue.documentId);
      const revision = Number(argumentsValue.expectedPlanRevision);
      const action = String(argumentsValue.action);
      if (action === 'accept') return service.acceptStep(
        documentId, revision, String(argumentsValue.stepId), String(argumentsValue.attemptId),
        argumentsValue.approveSoftProtectionConflicts === true
      );
      if (action === 'reject') return service.rejectStep(
        documentId, revision, String(argumentsValue.stepId), String(argumentsValue.attemptId), String(argumentsValue.reason)
      );
      if (action === 'skip') return service.skipStep(documentId, revision, String(argumentsValue.stepId));
      if (action === 'rollback') return service.rollbackStep(documentId, revision, String(argumentsValue.stepId));
      if (action === 'pause') return service.pause(documentId, revision);
      if (action === 'resume') return service.resume(documentId, revision);
      if (action === 'start-page') return service.startPage(
        documentId, revision, String(argumentsValue.pageId),
        typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440
      );
      throw new Error(`Unsupported plan control action: ${action}`);
    }
    case 'web_design_inspect_step':
      return progressiveGenerationService().inspectStep(
        String(argumentsValue.documentId), String(argumentsValue.stepId),
        typeof argumentsValue.attemptId === 'string' ? argumentsValue.attemptId : undefined
      );
    case 'web_design_accept_step':
      return progressiveGenerationService().acceptStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId),
        String(argumentsValue.attemptId), argumentsValue.approveSoftProtectionConflicts === true
      );
    case 'web_design_reject_step':
      return progressiveGenerationService().rejectStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId),
        String(argumentsValue.attemptId), String(argumentsValue.reason)
      );
    case 'web_design_skip_step':
      return progressiveGenerationService().skipStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId)
      );
    case 'web_design_rollback_step':
      return progressiveGenerationService().rollbackStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId)
      );
    case 'web_design_complete_page':
      return progressiveGenerationService().completePage(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.pageId)
      );
    case 'web_design_pause_plan':
      return progressiveGenerationService().pause(String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision));
    case 'web_design_resume_plan':
      return progressiveGenerationService().resume(String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision));
    case 'web_design_list_documents':
      return { documents: await store.listInProject(defaultProject.projectId, scopeKey) };
    case 'web_design_create_document': {
      const title = typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined;
      const document = await store.createInProject(defaultProject.projectId, title, true);
      return { document: designSummary(document) };
    }
    case 'web_design_get_document':
      return { document: await store.readInScope(String(argumentsValue.documentId), scopeKey) };
    case 'web_design_get_document_outline': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      return {
        document: designSummary(document),
        pages: pagesForDocument(document).map((page) => pageOutline(document, page.id)),
        symbolCount: document.symbols?.length ?? 0,
        assetCount: document.assets?.length ?? 0
      };
    }
    case 'web_design_get_page': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      const page = pagesForDocument(document).find((candidate) => candidate.id === pageId);
      if (!page) throw new Error(`Page not found: ${pageId}`);
      const components = document.components
        .filter((component) => pageIdForComponent(document, component) === pageId)
        .sort((left, right) => left.zIndex - right.zIndex);
      const componentIds = new Set(components.map((component) => component.id));
      return {
        document: designSummary(document),
        page,
        viewport: document.viewport,
        breakpoints: document.breakpoints,
        tokens: document.tokens,
        rootIds: components.filter((component) => !component.parentId).map((component) => component.id),
        components,
        requests: document.requests.filter((request) => !request.componentId || componentIds.has(request.componentId)),
        quality: pageOutline(document, pageId).quality
      };
    }
    case 'web_design_get_node': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const componentId = String(argumentsValue.componentId);
      const root = document.components.find((component) => component.id === componentId);
      if (!root) throw new Error(`Component not found: ${componentId}`);
      const depth = typeof argumentsValue.depth === 'number' ? Math.max(0, Math.min(8, Math.trunc(argumentsValue.depth))) : 4;
      const byParent = new Map<string, WebDesignComponent[]>();
      for (const component of document.components) {
        if (!component.parentId) continue;
        const children = byParent.get(component.parentId) ?? [];
        children.push(component);
        byParent.set(component.parentId, children);
      }
      const descendants: Array<{ component: WebDesignComponent; depth: number }> = [];
      let frontier = [{ component: root, depth: 0 }];
      while (frontier.length > 0) {
        const current = frontier.shift()!;
        descendants.push(current);
        if (current.depth >= depth) continue;
        frontier.push(...(byParent.get(current.component.id) ?? [])
          .sort((left, right) => left.zIndex - right.zIndex)
          .map((component) => ({ component, depth: current.depth + 1 })));
      }
      const byId = new Map(document.components.map((component) => [component.id, component]));
      const ancestors: Array<{ id: string; name: string; type: string }> = [];
      let parentId = root.parentId;
      while (parentId) {
        const parent = byId.get(parentId);
        if (!parent) break;
        ancestors.unshift({ id: parent.id, name: parent.name, type: parent.type });
        parentId = parent.parentId;
      }
      return {
        document: designSummary(document),
        pageId: pageIdForComponent(document, root),
        ancestorPath: ancestors,
        rootId: root.id,
        requestedDepth: depth,
        truncated: descendants.some(({ component, depth: componentDepth }) => componentDepth === depth && (byParent.get(component.id)?.length ?? 0) > 0),
        nodes: descendants
      };
    }
    case 'web_design_get_catalog': {
      const kind = typeof argumentsValue.kind === 'string' ? argumentsValue.kind : 'summary';
      const libraries = UI_LIBRARIES.map((library) => ({
          id: library.id,
          name: library.displayName,
          version: library.version,
          categories: library.categories,
          componentCount: library.components.length,
          variantCount: library.components.reduce((total, component) => total + (library.variants[component.id]?.length ?? 1), 0)
        }));
      if (kind === 'libraries') return { kind, libraries };
      if (kind === 'sections') return { kind, sections: WEB_DESIGN_BLOCK_PRESETS };
      if (kind === 'templates') return { kind, templates: WEB_DESIGN_PAGE_TEMPLATES };
      if (kind === 'themes') return {
        kind,
        themes: WEB_DESIGN_THEME_PRESETS.map(({ tokens: _tokens, ...theme }) => theme)
      };
      return {
        kind: 'summary',
        libraries,
        assetKinds: {
          components: UI_LIBRARIES.reduce((count, library) => count + library.components.length, 0),
          sections: WEB_DESIGN_BLOCK_PRESETS.length,
          templates: WEB_DESIGN_PAGE_TEMPLATES.length,
          themes: WEB_DESIGN_THEME_PRESETS.length
        },
        nextAction: { tool: 'web_design_search_catalog', detail: 'Search only the kind needed by the active design Step.' }
      };
    }
    case 'web_design_search_catalog': {
      const kind = typeof argumentsValue.kind === 'string' ? argumentsValue.kind : 'components';
      const query = typeof argumentsValue.query === 'string' ? argumentsValue.query.trim().toLocaleLowerCase() : '';
      const queryTokens = [...new Set(query.split(/[^\p{L}\p{N}]+/u).filter(Boolean))];
      const category = typeof argumentsValue.category === 'string' ? argumentsValue.category.trim().toLocaleLowerCase() : '';
      const limit = typeof argumentsValue.limit === 'number' ? Math.max(1, Math.min(50, Math.trunc(argumentsValue.limit))) : 20;
      const matches = (values: string[]): number => {
        if (!query) return 1;
        const searchable = values.map((value) => value.toLocaleLowerCase());
        if (searchable.some((value) => value.includes(query))) return 100;
        return queryTokens.filter((token) => searchable.some((value) => value.includes(token))).length;
      };
      if (kind === 'sections') {
        const candidates = WEB_DESIGN_BLOCK_PRESETS
          .filter((item) => !category || item.category.toLocaleLowerCase() === category)
          .map((item) => ({ item, score: matches([item.id, item.name, item.category, item.description, ...item.keywords]) }))
          .filter(({ score }) => score > 0).sort((left, right) => right.score - left.score)
          .slice(0, limit).map(({ item }) => item);
        return { kind, query, count: candidates.length, candidates };
      }
      if (kind === 'templates') {
        const candidates = WEB_DESIGN_PAGE_TEMPLATES
          .filter((item) => !category || item.category.toLocaleLowerCase() === category)
          .map((item) => ({ item, score: matches([item.id, item.name, item.category, item.description, ...item.blocks]) }))
          .filter(({ score }) => score > 0).sort((left, right) => right.score - left.score)
          .slice(0, limit).map(({ item }) => item);
        return { kind, query, count: candidates.length, candidates };
      }
      if (kind === 'themes') {
        const candidates = WEB_DESIGN_THEME_PRESETS
          .map((item) => ({ item, score: matches([item.id, item.name, item.description]) }))
          .filter(({ score }) => score > 0).sort((left, right) => right.score - left.score)
          .slice(0, limit).map(({ item }) => ({
            id: item.id, name: item.name, description: item.description,
            canvasBackground: item.canvasBackground, preview: item.preview
          }));
        return { kind, query, count: candidates.length, candidates };
      }
      const libraryId = typeof argumentsValue.libraryId === 'string' ? argumentsValue.libraryId : undefined;
      const includeDeprecated = argumentsValue.includeDeprecated === true;
      const candidates = UI_LIBRARIES
        .filter((library) => !libraryId || library.id === libraryId)
        .flatMap((library) => library.components.map((component) => ({ library, component })))
        .filter(({ component }) => includeDeprecated || component.status !== 'deprecated')
        .filter(({ component }) => !category || component.category.toLocaleLowerCase() === category)
        .map(({ library, component }) => ({
          library, component,
          score: matches([component.id, component.label, component.category, component.baseType, ...component.keywords])
        }))
        .filter(({ score }) => score > 0)
        .sort((left, right) => right.score - left.score || left.component.id.localeCompare(right.component.id))
        .slice(0, limit)
        .map(({ library, component }) => ({
          libraryId: library.id,
          libraryName: library.displayName,
          libraryVersion: library.version,
          componentId: component.id,
          label: component.label,
          category: component.category,
          baseType: component.baseType,
          defaultSize: { width: component.width, height: component.height },
          variantCount: library.variants[component.id]?.length ?? 1,
          keywords: component.keywords,
          status: component.status ?? 'stable'
        }));
      return { kind: 'components', query, count: candidates.length, candidates };
    }
    case 'web_design_search_components': {
      const query = typeof argumentsValue.query === 'string' ? argumentsValue.query.trim().toLocaleLowerCase() : '';
      const queryTokens = [...new Set(query.split(/[^\p{L}\p{N}]+/u).filter(Boolean))];
      const libraryId = typeof argumentsValue.libraryId === 'string' ? argumentsValue.libraryId : undefined;
      const category = typeof argumentsValue.category === 'string' ? argumentsValue.category.trim().toLocaleLowerCase() : '';
      const includeDeprecated = argumentsValue.includeDeprecated === true;
      const limit = typeof argumentsValue.limit === 'number' ? Math.max(1, Math.min(50, Math.trunc(argumentsValue.limit))) : 20;
      const candidates = UI_LIBRARIES
        .filter((library) => !libraryId || library.id === libraryId)
        .flatMap((library) => library.components.map((component) => ({ library, component })))
        .filter(({ component }) => includeDeprecated || component.status !== 'deprecated')
        .filter(({ component }) => !category || component.category.toLocaleLowerCase() === category)
        .map(({ library, component }) => {
          const searchable = [component.id, component.label, component.category, component.baseType, ...component.keywords]
            .map((value) => value.toLocaleLowerCase());
          const exact = query ? searchable.some((value) => value.includes(query)) : true;
          const tokenMatches = queryTokens.filter((token) => searchable.some((value) => value.includes(token))).length;
          return { library, component, exact, tokenMatches };
        })
        .filter(({ exact, tokenMatches }) => !query || exact || tokenMatches > 0)
        .sort((left, right) => Number(right.exact) - Number(left.exact)
          || right.tokenMatches - left.tokenMatches
          || left.component.id.localeCompare(right.component.id))
        .slice(0, limit)
        .map(({ library, component }) => ({
          libraryId: library.id,
          libraryName: library.displayName,
          libraryVersion: library.version,
          componentId: component.id,
          label: component.label,
          category: component.category,
          baseType: component.baseType,
          defaultSize: { width: component.width, height: component.height },
          variantCount: library.variants[component.id]?.length ?? 1,
          keywords: component.keywords,
          status: component.status ?? 'stable',
          docsUrl: component.docsUrl
        }));
      return { query, count: candidates.length, candidates };
    }
    case 'web_design_get_component_contract': {
      const library = UI_LIBRARIES.find((candidate) => candidate.id === argumentsValue.libraryId);
      if (!library) throw new Error(`UI library not found: ${String(argumentsValue.libraryId)}`);
      const component = library.components.find((candidate) => candidate.id === argumentsValue.componentId);
      if (!component) throw new Error(`${library.displayName} component not found: ${String(argumentsValue.componentId)}`);
      const variants = library.variants[component.id] ?? [{ id: 'default', label: '默认款式', props: {} }];
      const defaultVariant = variants[0];
      const instance = createComponentFromUiLibrary(library.id, component.id, 0, 0);
      const editableSlots = editableSlotsForUiComponent(instance);
      const componentSlug = String(component.props?.componentSlug
        ?? component.id.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase());
      return {
        library: {
          id: library.id,
          name: library.displayName,
          version: library.version,
          license: library.license,
          sourceUrl: library.sourceUrl,
          licenseUrl: library.licenseUrl
        },
        component: {
          ...component,
          variants,
          bindingTemplate: {
            name: library.id,
            version: library.version,
            component: component.id,
            variant: defaultVariant.id,
            props: { ...(component.props ?? {}), ...defaultVariant.props }
          },
          sceneBindingTemplate: {
            type: 'library-instance',
            library: library.id,
            component: component.id,
            variant: defaultVariant.id,
            properties: { ...(component.props ?? {}), ...defaultVariant.props, componentSlug },
            content: defaultVariant.content ?? component.content,
            frame: { width: defaultVariant.width ?? component.width, height: defaultVariant.height ?? component.height },
            layout: { position: 'absolute' },
            slots: Object.fromEntries(editableSlots.map((slot) => [slot.id, []]))
          },
          editableSlots
        }
      };
    }
    case 'web_design_insert_section': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      const document = await store.insertSection(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          pageId,
          String(argumentsValue.sectionId) as (typeof WEB_DESIGN_BLOCK_PRESETS)[number]['id']
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document, pageId) });
    }
    case 'web_design_apply_page_template': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      const document = await store.applyPageTemplate(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          pageId,
          String(argumentsValue.templateId) as (typeof WEB_DESIGN_PAGE_TEMPLATES)[number]['id']
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document, pageId) });
    }
    case 'web_design_replace_document': {
      const document: unknown = argumentsValue.document;
      assertWebDesignDocument(document);
      await store.readInScope((document as WebDesignDocument).documentId, scopeKey);
      const saved = await store.replace(document as WebDesignDocument, Number(argumentsValue.expectedRevision));
      return compactMutationResult(saved, { changedIds: saved.components.map((component) => component.id) });
    }
    case 'web_design_apply_patch': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const operations = argumentsValue.operations as WebDesignPatchOperation[];
      const pageId = assertFocusedOperations(current, operations);
      const document = await store.patch(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          operations
        );
      return compactMutationResult(document, { pageId, changedIds: changedComponentIds(operations) });
    }
    case 'web_design_apply_node_batch': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      if (!pagesForDocument(current).some((page) => page.id === pageId)) throw new Error(`Page not found: ${pageId}`);
      const components = argumentsValue.components as WebDesignComponent[];
      if (components.some((component) => component.pageId !== pageId)) {
        throw new Error('Every component in a node batch must use the requested pageId.');
      }
      const serializedBytes = Buffer.byteLength(JSON.stringify(components), 'utf8');
      if (serializedBytes > 65_536) throw new Error(`Node batch is ${serializedBytes} bytes. Split the logical region into smaller batches.`);
      const operations: WebDesignPatchOperation[] = components.map((component) => ({ op: 'upsert_component', component }));
      const document = await store.patch(String(argumentsValue.documentId), Number(argumentsValue.expectedRevision), operations);
      return compactMutationResult(document, {
        pageId,
        regionName: String(argumentsValue.regionName),
        changedIds: components.map((component) => component.id)
      });
    }
    case 'web_design_auto_layout': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const componentId = String(argumentsValue.containerId);
      const pageId = componentPageId(current, componentId);
      const document = await store.autoLayout(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          componentId,
          String(argumentsValue.device) as 'desktop' | 'tablet' | 'mobile'
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document, pageId) });
    }
    case 'web_design_sync_symbol_instances': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const document = await store.syncSymbolInstances(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.symbolId)
        );
      return compactMutationResult(document, { changedIds: changedIdsBetween(current, document) });
    }
    case 'web_design_update_symbol_from_instance': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const componentId = String(argumentsValue.componentId);
      const pageId = componentPageId(current, componentId);
      const document = await store.updateSymbolFromInstance(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          componentId
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document) });
    }
    case 'web_design_export_html': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      if (typeof argumentsValue.pageId === 'string') {
        const pageId = argumentsValue.pageId;
        assertHandoffQuality(document, pageId);
        const file = exportDocumentHtmlFiles(document, device).find((candidate) => candidate.pageId === pageId);
        if (!file) throw new Error(`Page not found: ${pageId}`);
        return { files: [file] };
      }
      assertHandoffQuality(document);
      return { files: exportDocumentHtmlFiles(document, device) };
    }
    case 'web_design_export_react': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      assertHandoffQuality(document);
      return { files: [exportReactComponent(document, device)] };
    }
    case 'web_design_export_vue': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      assertHandoffQuality(document);
      return { files: [exportVueComponent(document, device)] };
    }
    case 'web_design_list_requests': {
      const includeResolved = argumentsValue.includeResolved === true;
      if (typeof argumentsValue.documentId === 'string') {
        return { requests: await requestEntries(argumentsValue.documentId, includeResolved) };
      }
      const summaries = await store.listInProject(defaultProject.projectId, scopeKey);
      const requests = (await Promise.all(summaries.map((item) => requestEntries(item.documentId, includeResolved)))).flat();
      return { requests };
    }
    case 'web_design_resolve_request': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const requestId = String(argumentsValue.requestId);
      const request = current.requests.find((candidate) => candidate.id === requestId);
      const pageId = request?.componentId ? componentPageId(current, request.componentId) : undefined;
      const document = await store.patch(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          [{
            op: 'resolve_request',
            requestId,
            resolution: typeof argumentsValue.resolution === 'string' ? argumentsValue.resolution : undefined
          }]
        );
      return compactMutationResult(document, { pageId, changedIds: request?.componentId ? [request.componentId] : [] });
    }
    case 'web_design_validate': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      return validateWebDesignDocument(document, {
        pageId: typeof argumentsValue.pageId === 'string' ? argumentsValue.pageId : undefined,
        mode: argumentsValue.mode === 'draft' ? 'draft' : 'handoff'
      });
    }
    default:
      throw new Error(`Unknown Web Design Studio tool: ${name}`);
  }
}

function result(value: Record<string, unknown>, isError = false) {
  const sourceImages = Array.isArray(value.__images) ? value.__images as ToolImagePayload[] : [];
  const { __images: _discardedImages, ...structuredContent } = value;
  return {
    content: [
      { type: 'text' as const, text: JSON.stringify(structuredContent) },
      ...sourceImages.map((image) => ({ type: 'image' as const, data: image.data, mimeType: image.mimeType }))
    ],
    structuredContent,
    isError
  };
}

async function runMcp(): Promise<void> {
  await store.initialize();
  await store.ensureLegacyProject();
  const server = new Server({ name: SERVER_NAME, version: SERVER_VERSION }, { capabilities: { tools: {} } });
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [...TOOL_DEFINITIONS] }));
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try {
      return result(await callTool(request.params.name, request.params.arguments ?? {}));
    } catch (error) {
      return result({
        error: error instanceof Error ? error.message : String(error),
        ...(error instanceof RevisionConflictError || error instanceof GenerationPlanRevisionConflictError || error instanceof SceneRevisionConflictError
          ? { actualRevision: error.actualRevision }
          : {})
      }, true);
    }
  });
  await server.connect(new StdioServerTransport());
}

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === '--version' || command === '-v') {
    process.stdout.write(`${SERVER_VERSION}\n`);
    return;
  }
  if (command === 'mcp') {
    await runMcp();
    return;
  }
  process.stderr.write('Usage: chatos-web-design-studio mcp\n');
  process.exitCode = 2;
}

await main().catch((error) => {
  process.stderr.write(`Web Design Studio failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
