import type { WebDesignComponent, WebDesignDocument } from '../../src/schema';
import type { WorkspaceArtboardPlacement } from '../../src/v2/workspace-placement-store';
import type { EditorSelectableNode } from './selection-model';

/**
 * Actions invoked by an earlier action group but implemented by a later group.
 * Keeping this boundary explicit avoids circular ReturnType dependencies while
 * preserving the staged action construction order.
 */
export interface WebDesignDeferredActions {
  editComponentSlot(
    component: WebDesignComponent,
    slotId: string,
    options?: { compoundOnly?: boolean }
  ): Promise<boolean>;
  activateWorkspaceArtboard(artboard: WorkspaceArtboardPlacement): void;
  withGeneratedResponsiveLayouts(
    active: WebDesignDocument,
    targetPageId: string
  ): WebDesignDocument;
  selectComponent(componentId: string, additive?: boolean): void;
  selectableNodesForCurrentEditor(current: WebDesignDocument): EditorSelectableNode[];
  copySceneSelection(): void;
  duplicateSceneSelection(): void;
  pasteSceneClipboard(): void;
}

/** Actions that state-level effects and keyboard handlers may invoke. */
export interface WebDesignStateActions {
  openDocument(document: WebDesignDocument): void;
  showToast(message: string): void;
  changeLiveWithCanvasGrowth(updater: (document: WebDesignDocument) => WebDesignDocument): void;
  toggleInteractionMode(): void;
  save(): Promise<void>;
  undo(): void;
  redo(): void;
  ungroupSelected(): void;
  groupSelected(): void;
  duplicateSelected(): void;
  copySelected(): void;
  pasteClipboard(): void;
  selectSelectionChild(): void;
  selectSelectionParent(): void;
  activateWorkspaceTool(tool: import('./workspace-shell-model').WorkspaceTool): void;
  deleteSceneSelection(): Promise<void>;
  deleteSelected(): void;
  nudgeSceneSelection(deltaX: number, deltaY: number): Promise<void>;
  nudgeSelected(deltaX: number, deltaY: number): void;
}
