import { createHash } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { executeSceneEditorCommand, type SceneEditorCommand } from './v2/scene-editor-command.js';
import { SceneQueryIndex, type SceneQuery } from './v2/scene-query.js';
import { renderResponsiveSceneHandoff } from './v2/scene-handoff-exporter.js';
import type { CreateGenerationStepInput, GenerationDesignIntent } from './v2/generation-plan-schema.js';
import {
  generationRepositories,
  progressiveGenerationService,
  generationVisualService,
  annotationAiService,
  activeProgressiveContext,
  assertGenerationDocumentInScope
} from './mcp-runtime-context.js';
import {
  decodeStructuredJson,
  normalizeGenerationOperations,
  assertSceneCommandInArtboard
} from './mcp-tool-helpers.js';

interface HandoffArtifactFile {
  producerArtifactId: string;
  relativePath: string;
  displayName: string;
  mimeType: string;
  size: number;
  sha256: string;
}

function safeArtifactBaseName(value: unknown, fallback: string): string {
  const source = typeof value === 'string' ? value.trim().normalize('NFKC') : '';
  const normalized = source.replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, '');
  return (normalized || fallback).slice(0, 120);
}

async function writeHandoffArtifact(fileName: string, data: Buffer, mimeType: string): Promise<HandoffArtifactFile> {
  const configured = process.env.CHATOS_PLUGIN_ARTIFACT_DIR?.trim();
  if (!configured) throw new Error('CHATOS_PLUGIN_ARTIFACT_DIR is required for Web Design handoff export.');
  const directory = path.resolve(configured);
  await fs.mkdir(directory, { recursive: true });
  await fs.writeFile(path.join(directory, fileName), data);
  const sha256 = createHash('sha256').update(data).digest('hex');
  return {
    producerArtifactId: `web_design_${sha256}`,
    relativePath: fileName,
    displayName: fileName,
    mimeType,
    size: data.length,
    sha256
  };
}

export async function callProgressiveTool(
  name: string,
  argumentsValue: Record<string, unknown>
): Promise<Record<string, unknown> | undefined> {
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
    case 'web_design_export_handoff': {
      const documentId = String(argumentsValue.documentId);
      const pageId = String(argumentsValue.pageId);
      const planResult = await progressiveGenerationService().getPlan(documentId);
      const plan = planResult.plan as { pages?: Array<{ pageId: string; status: string }> };
      if (!plan.pages?.some((page) => page.pageId === pageId && page.status === 'completed')) {
        throw new Error(`Scene page ${pageId} must complete its accepted handoff before export.`);
      }
      const desktopWidth = typeof argumentsValue.desktopWidth === 'number' ? argumentsValue.desktopWidth : 1440;
      const mobileWidth = typeof argumentsValue.mobileWidth === 'number' ? argumentsValue.mobileWidth : 390;
      const scene = await generationRepositories.scenes.read(documentId);
      const rendered = renderResponsiveSceneHandoff(scene, pageId, desktopWidth, mobileWidth);
      const desktopCapture = await generationVisualService().capturePage(documentId, pageId, desktopWidth);
      const mobileCapture = await generationVisualService().capturePage(documentId, pageId, mobileWidth);
      const desktopImage = desktopCapture.__images[0];
      const mobileImage = mobileCapture.__images[0];
      if (!desktopImage || !mobileImage) throw new Error('Web Design handoff preview capture did not return PNG data.');
      const baseName = safeArtifactBaseName(argumentsValue.fileBaseName, pageId);
      const files = [
        await writeHandoffArtifact(`${baseName}.html`, Buffer.from(rendered.html, 'utf8'), 'text/html'),
        await writeHandoffArtifact(`${baseName}-desktop.png`, Buffer.from(desktopImage.data, 'base64'), 'image/png'),
        await writeHandoffArtifact(`${baseName}-mobile.png`, Buffer.from(mobileImage.data, 'base64'), 'image/png')
      ];
      return {
        documentId,
        pageId,
        entryFile: files[0].displayName,
        files,
        verification: {
          desktop: { viewportWidth: desktopWidth, height: rendered.desktopHeight },
          mobile: { viewportWidth: mobileWidth, height: rendered.mobileHeight },
          sourceRevision: scene.revision
        }
      };
    }
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
      if (argumentsValue.operations !== undefined && argumentsValue.operationsJson !== undefined) {
        throw new Error('Supply operations only; operationsJson is retained only for compatibility.');
      }
      const decodedOperations = Array.isArray(argumentsValue.operations)
        ? argumentsValue.operations
        : decodeStructuredJson(argumentsValue.operationsJson, 'operationsJson');
      if (!Array.isArray(decodedOperations)) throw new Error('operations must be an operation array.');
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
    default:
      return undefined;
  }
}
