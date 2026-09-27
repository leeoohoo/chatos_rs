import type {
  SceneAlignment,
  SceneDistribution,
  SceneLayerPlacement,
  SceneResizeHandle
} from './scene-editor-transaction.js';
import type {
  SceneDocument,
  SceneNode,
  SceneResponsiveNodeOverride,
  SceneVariableCollection
} from './scene-schema.js';
import type { SceneTransactionSummary } from './scene-transaction.js';

export type ScenePadding = number | { top: number; right: number; bottom: number; left: number };

export interface SceneEditorNodePatch {
  path: string[];
  value: unknown;
}

export type SceneEditorCommand =
  | { type: 'create-page'; pageId: string; name: string; rootNodeId: string; width: number; height: number }
  | { type: 'duplicate-page'; pageId: string; newPageId: string; name: string }
  | { type: 'delete-page'; pageId: string }
  | { type: 'rename-page'; pageId: string; name: string }
  | { type: 'insert-node'; parentId: string; index: number; slot?: string; node: SceneNode }
  | { type: 'set-variable-collections'; collections: SceneVariableCollection[] }
  | { type: 'move'; nodeIds: string[]; deltaX: number; deltaY: number }
  | { type: 'align'; nodeIds: string[]; alignment: SceneAlignment }
  | { type: 'distribute'; nodeIds: string[]; axis: SceneDistribution }
  | { type: 'reorder'; nodeIds: string[]; placement: SceneLayerPlacement }
  | {
      type: 'resize';
      nodeId: string;
      handle: SceneResizeHandle;
      deltaX: number;
      deltaY: number;
      minimumWidth?: number;
      minimumHeight?: number;
    }
  | { type: 'group'; nodeIds: string[]; wrapperId: string; name: string }
  | { type: 'frame'; nodeIds: string[]; wrapperId: string; name: string; padding?: ScenePadding }
  | {
      type: 'auto-layout-frame';
      nodeIds: string[];
      wrapperId: string;
      name: string;
      direction: 'horizontal' | 'vertical';
      padding?: ScenePadding;
      gap?: number;
      alignItems?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
      justifyContent?: 'start' | 'center' | 'end' | 'between' | 'around' | 'evenly';
      sizingX?: 'fixed' | 'hug';
      sizingY?: 'fixed' | 'hug';
    }
  | { type: 'ungroup'; wrapperId: string }
  | { type: 'delete-nodes'; nodeIds: string[] }
  | {
      type: 'set-responsive-override';
      ruleId: string;
      ruleName: string;
      minWidth?: number;
      maxWidth?: number;
      nodeId: string;
      override: Omit<SceneResponsiveNodeOverride, 'nodeId'>;
    }
  | { type: 'clear-responsive-override'; ruleId: string; nodeId: string }
  | { type: 'add-annotation'; nodeId: string; annotationId: string; body: string }
  | { type: 'resolve-annotation'; nodeId: string; annotationId: string }
  | { type: 'reopen-annotation'; nodeId: string; annotationId: string }
  | { type: 'update-node'; nodeId: string; patches: SceneEditorNodePatch[] };

export interface SceneEditorCommandRequest {
  transactionId: string;
  expectedRevision: number;
  reason?: string;
  command: SceneEditorCommand;
}

export interface SceneEditorCommandResult {
  document: SceneDocument;
  summary: SceneTransactionSummary;
  commandType: SceneEditorCommand['type'];
  recovered: boolean;
}
