import {
  createAnnotationAiTask,
  type AnnotationAiTask
} from './annotation-ai-protocol.js';
import type { GenerationVisualService, VisualToolResult } from './generation-visual-service.js';
import type { SceneDocumentStore } from './scene-store.js';

export interface PrepareAnnotationAiTaskInput {
  documentId: string;
  nodeId: string;
  annotationId: string;
  viewportWidth: number;
  dependencyNodeIds?: string[];
  padding?: number;
}

export interface PreparedAnnotationAiTask extends Record<string, unknown> {
  task: AnnotationAiTask;
  visualContext: Omit<VisualToolResult, '__images'>;
  nextAction: {
    tool: 'web_design_edit_scene';
    documentId: string;
    pageId: string;
    targetNodeId: string;
    expectedRevision: number;
    afterEdit: ['web_design_capture_region', 'web_design_compare_snapshots'];
  };
  __images: VisualToolResult['__images'];
}

export interface AnnotationAiServiceOptions {
  projectId: string;
  scenes: SceneDocumentStore;
  visuals: GenerationVisualService;
  assertDocumentInScope(documentId: string): Promise<void>;
}

function identifier(value: string, label: string): string {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value)) throw new Error(`${label} is invalid.`);
  return value;
}

export class AnnotationAiService {
  constructor(private readonly options: AnnotationAiServiceOptions) {
    identifier(options.projectId, 'projectId');
  }

  async prepare(input: PrepareAnnotationAiTaskInput): Promise<PreparedAnnotationAiTask> {
    const documentId = identifier(input.documentId, 'documentId');
    const nodeId = identifier(input.nodeId, 'nodeId');
    const annotationId = identifier(input.annotationId, 'annotationId');
    await this.options.assertDocumentInScope(documentId);
    const document = await this.options.scenes.read(documentId);
    const task = createAnnotationAiTask(document, {
      scope: { projectId: this.options.projectId, documentId },
      targetNodeId: nodeId,
      annotationId,
      dependencyNodeIds: input.dependencyNodeIds,
      requiredViewportWidths: [input.viewportWidth]
    });
    const capture = await this.options.visuals.captureRegion({
      documentId,
      pageId: task.pageId,
      viewportWidth: input.viewportWidth,
      nodeId,
      padding: input.padding ?? 24
    });
    const captureArtifact = (capture.capture as { artifact?: { revision?: number } } | undefined)?.artifact;
    if (captureArtifact?.revision !== task.baseRevision) {
      throw new Error('Scene changed while preparing the annotation visual context. Retry with the latest revision.');
    }
    const { __images, ...visualContext } = capture;
    return {
      task,
      visualContext,
      nextAction: {
        tool: 'web_design_edit_scene',
        documentId,
        pageId: task.pageId,
        targetNodeId: task.targetNodeId,
        expectedRevision: task.baseRevision,
        afterEdit: ['web_design_capture_region', 'web_design_compare_snapshots']
      },
      __images
    };
  }
}
