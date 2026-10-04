import type { WebDesignComponent, WebDesignDocument } from '../../src/schema';
import type {
  UiComponentDefinition,
  UiComponentVariant,
  UiLibraryCatalog,
} from '../../src/ui-library';
import type { WorkspaceArtboardPlacement } from '../../src/v2/workspace-placement-store';
import type { EditorSelectableNode } from './selection-model';

export type WebDesignStudioState =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState>;

export type WebDesignCoreActionContext = WebDesignStudioState;

export type WebDesignInsertActionContext =
  WebDesignStudioState &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  Pick<WebDesignDeferredActions,
    'editComponentSlot' | 'activateWorkspaceArtboard' | 'withGeneratedResponsiveLayouts'>;

export type WebDesignCanvasActionContext =
  WebDesignStudioState &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  Pick<WebDesignDeferredActions,
    | 'withGeneratedResponsiveLayouts'
    | 'selectComponent'
    | 'selectableNodesForCurrentEditor'
    | 'copySceneSelection'
    | 'duplicateSceneSelection'
    | 'pasteSceneClipboard'>;

export type WebDesignViewportActionContext =
  WebDesignStudioState &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions>;

export type WebDesignSelectionActionContext =
  WebDesignViewportActionContext & {
    selectedSceneLibrary: UiLibraryCatalog | undefined;
    selectedSceneLibraryDefinition: UiComponentDefinition | undefined;
    selectedSceneLibraryVariants: readonly UiComponentVariant[];
  };

export type WebDesignAssetActionContext =
  WebDesignSelectionActionContext &
  ReturnType<typeof import('./WebDesignSelectionActions').createWebDesignSelectionActions>;

export type WebDesignDocumentActionContext =
  WebDesignAssetActionContext &
  ReturnType<typeof import('./WebDesignAssetActions').createWebDesignAssetActions>;

export type WebDesignRenderContext =
  WebDesignDocumentActionContext &
  ReturnType<typeof import('./WebDesignDocumentActions').createWebDesignDocumentActions>;

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
