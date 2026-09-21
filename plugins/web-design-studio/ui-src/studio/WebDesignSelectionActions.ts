import { useEffect, useMemo, useReducer, useRef, useState, type CSSProperties, type DragEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import {
  autoLayoutContainer,
  breakpointFor,
  cloneComponentSubtrees,
  constrainComponentFrame,
  componentsForPage,
  createSymbolFromSelection,
  deriveResponsivePageFromDevice,
  detachSymbolInstance,
  descendantIds,
  flattenComponentTree,
  fitContentCanvasToComponents,
  growPageToFitContent,
  moveComponentsWithDescendants,
  reflowPageForViewport,
  instantiateSymbol,
  resolveComponent,
  selectedRootIds,
  setSymbolOverride,
  snapComponentFrame,
  syncSymbolInstances,
  updateComponentFrame,
  updateComponentStyle,
  updateSymbolFromInstance,
  type ResolvedWebDesignComponent,
  type SnapGuides
} from '../../src/editor-model';
import { exportPageHtml } from '../../src/html-exporter';
import { exportReactComponent } from '../../src/react-exporter';
import { exportVueComponent } from '../../src/vue-exporter';
import {
  componentsInSlot,
  editableSlotsForUiComponent,
  growUiContentContainersToFit,
  isOverlayUiContentContainer,
  isUiContentContainer,
  slotIdForDescendant,
  visibleComponentsInSlot
} from '../../src/library-slots';
import { applyUiLibraryVariant, createComponentFromUiLibrary, uiLibraryByName, UI_LIBRARIES, variantsForBoundComponent } from '../../src/ui-libraries';
import type { UiComponentDefinition, UiComponentVariant, UiEditableSlot } from '../../src/ui-library';
import { officialRuntimePresentation } from '../library-runtime/registry';
import { WEB_DESIGN_THEME_PRESETS, type WebDesignThemePreset } from '../../src/design-themes';
import { componentDefaults } from '../../src/templates';
import {
  matchArtboardSizePreset,
  matchViewportPreset,
  viewportDimensions,
  viewportPresetsForDevice,
  WEB_DESIGN_ARTBOARD_SIZE_PRESETS,
  type WebDesignViewportOrientation
} from '../../src/viewport-presets';
import {
  pagesForDocument,
  tokensForDocument,
  type WebComponentStyle,
  type WebComponentConstraints,
  type WebComponentVisualState,
  type WebComponentType,
  type WebDesignAsset,
  type WebDesignComponent,
  type WebDesignDevice,
  type WebDesignDocument,
  type WebDesignJsonValue,
  type WebDesignLibraryName,
  type WebDesignProject,
  type WebHorizontalConstraint,
  type WebDesignSymbol,
  type WebDesignTokens,
  type WebSymbolOverride
} from '../../src/schema';
import {
  createRepository,
  type DesignRepository,
  type DesignSummary,
  type GenerationPlanSummary,
  type GenerationStepReview,
  type SceneAnnotationAiContext
} from './repository';
import { LibraryCanvasComponent } from './LibraryCanvasComponent';
import { componentEffectStyleToCss, componentStyleToCss, mergeComponentStyles } from './component-style';
import { CanvasComponent as WorkspaceCanvasComponent, CanvasComponentContent as WorkspaceCanvasComponentContent } from './CanvasComponent';
import { libraryPreviewSelection, type LibraryPreviewPointerEvent, type LibraryPreviewSelection } from '../library-runtime/element-selection';
import { WorkspaceBottomToolbar, WorkspaceNavigationBar, WorkspacePanelResizeHandle } from './WorkspaceShellChrome';
import { DEFAULT_WORKSPACE_SHELL, parseWorkspaceShellState, workspaceShellGridStyle, workspaceShellReducer, workspaceShellShortcut, type WorkspaceArea, type WorkspaceTool } from './workspace-shell-model';
import { initialWorkspaceArtboards, reconcileWorkspaceArtboards, updateWorkspaceArtboardById, workspaceViewportHeight } from './workspace-artboard-model';
import {
  fitWorkspaceRect,
  fitWorkspaceWidth,
  panWorkspaceCamera,
  unionWorkspaceRects,
  workspaceArtboardRenderTier,
  workspaceViewportReady,
  workspaceZoomFromWheel,
  zoomWorkspaceCameraAt,
  type WorkspaceCamera
} from '../../src/v2/workspace-camera';
import type { WorkspaceArtboardPlacement, WorkspacePlacementDocument, WorkspaceSurfaceKind } from '../../src/v2/workspace-placement-store';
import { indexSceneDocument, isSceneContainer, isSceneSlotContainer, type SceneDocument, type SceneNode, type ScenePrototypeLink, type SceneResponsiveNodeOverride, type SceneVariableCollection } from '../../src/v2/scene-schema';
import type { SceneEditorCommand } from '../../src/v2/scene-editor-command';
import type { SceneHistoryStatus } from '../../src/v2/scene-store';
import { inspectorCapabilities as resolveInspectorCapabilities } from './inspector-model';
import { SelectionOverlay, type SelectionOverlayItem } from './SelectionOverlay';
import { SceneArtboardCanvas, sceneArtboardContentHeight, sceneArtboardSelectionBounds } from './SceneArtboardCanvas';
import { createSceneBasicShape, createSceneLibraryInstance } from './scene-node-factory';
import { editableSlotsForSceneLibraryNode, resolveSceneInsertionTarget, type SceneInsertionFocus, type SceneInsertionTarget } from './scene-insertion-target';
import { createSceneSnippet, instantiateSceneSnippet, parseSceneSnippets, type SceneSnippet } from './scene-snippet-library';
import {
  deepestSelectionChild,
  normalizedSelectionRect,
  selectionCandidatesAtPoint,
  selectionNodesInRect,
  type EditorSelectableNode,
  type EditorSelectionCandidate,
  type EditorSelectionRect
} from './selection-model';

import {
  BasicShapeId,
  SCENE_RESPONSIVE_EDITOR_RULES,
  SLOT_EDITOR_HEADER_HEIGHT,
  SLOT_EDITOR_CANVAS_INSETS,
  slotEditorFrameBounds,
  palette,
  fillPresets,
  shadowPresets,
  basicShapeDefaults,
  contentContainerAncestor,
  overlayContentContainerAncestor,
  growCanvasForDevice,
  growAllCanvases,
  createSlotStarterComponents,
  materializeExistingSlotContent,
  visibleCssColor,
  cssPixels,
  svgDataUrl,
  horizontalConstraintOptions,
  ViewportSelection,
  STUDIO_PROJECT_QUERY,
  STUDIO_DESIGN_QUERY,
  studioLocationSelection,
  replaceStudioLocation,
  DEFAULT_VIEWPORT_SELECTIONS,
  viewportSelectionsForDocument,
  editableDocumentPayload,
  Interaction,
  CanvasPan,
  CanvasMarquee,
  LayerAction,
  AlignAction,
  LibraryTab,
  VariantPickerTarget,
  VariantPickerPointerDrag,
  EditingSlot,
  SceneContentFocus,
  SelectionCandidatePopover,
  InspectorVisualState,
  InspectorTab,
  PERSONAL_SYMBOLS_STORAGE_KEY,
  SCENE_SNIPPETS_STORAGE_KEY,
  WORKSPACE_ARTBOARD_HEADER_HEIGHT,
  WORKSPACE_SURFACE_LABELS,
  WORKSPACE_SURFACE_SIZES,
  deviceForWorkspaceArtboard,
  workspaceArtboardContentBounds,
  workspaceArtboardSignature,
  loadPersonalSymbols,
  VARIANT_PROP_LABELS,
  INTERNAL_LIBRARY_PROPS,
  inspectableLibraryProps,
  bindLibraryPreviewElement,
  variantDifferenceLabels,
  INTERACTIVE_COMPONENT_PREVIEWS,
  OPEN_OVERLAY_PREVIEWS,
  WIDE_VARIANT_PREVIEWS,
  variantIsInteractive,
  LazyVariantPreview,
  SelectableVariantCard
} from './WebDesignStudioSupport';
import { createWebDesignCoreActions } from './WebDesignCoreActions';
import { createWebDesignInsertActions } from './WebDesignInsertActions';
import { createWebDesignCanvasActions } from './WebDesignCanvasActions';
import { createWebDesignViewportActions } from './WebDesignViewportActions';

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions> &
  ReturnType<typeof import('./WebDesignViewportActions').createWebDesignViewportActions>;

export function createWebDesignSelectionActions(context: Record<string, any>) {
  const {
    actionsRef, repository, setRepository, documents, setDocuments, activeProject,
    setActiveProject, document, setDocument, sceneDocument, setSceneDocument, sceneHistory,
    setSceneHistory, sceneLoadState, setSceneLoadState, sceneReloadToken, setSceneReloadToken, ready,
    setReady, screen, setScreen, persistedRevision, setPersistedRevision, selectedId,
    setSelectedId, selectedIds, setSelectedIds, selectionCandidatePopover, setSelectionCandidatePopover, marqueeRect,
    setMarqueeRect, pageId, setPageId, clipboard, setClipboard, sceneClipboard, setSceneClipboard,
    snapGuides, setSnapGuides, dirty, setDirty, saving, setSaving,
    previewOverlayPageId, setPreviewOverlayPageId, interactionMode, setInteractionMode, device, setDevice,
    viewportSelections, setViewportSelections, workspaceCamera, setWorkspaceCamera, workspacePlacement, setWorkspacePlacement,
    activeArtboardId, setActiveArtboardId, scenePreviewHeights, setScenePreviewHeights, newSurfaceKind, setNewSurfaceKind,
    past, setPast, future, setFuture, toast, setToast,
    annotationText, setAnnotationText, aiInstruction, setAiInstruction, sceneAiContext, setSceneAiContext,
    sceneAnnotationPreparingId, setSceneAnnotationPreparingId, generationPlan, setGenerationPlan, generationReview, setGenerationReview,
    generationLoading, setGenerationLoading, generationAction, setGenerationAction, generationRejectionReason, setGenerationRejectionReason,
    paletteQuery, setPaletteQuery, libraryTab, setLibraryTab, personalSymbols, setPersonalSymbols,
    sceneSnippets, setSceneSnippets, sceneVariablesDraft, setSceneVariablesDraft, variantPickerTarget, setVariantPickerTarget,
    sceneContentFocus, setSceneContentFocus, variantPickerDrag, setVariantPickerDrag, themePickerOpen, setThemePickerOpen,
    projectLibraryOpen, setProjectLibraryOpen, newDesignOpen, setNewDesignOpen, newDesignName, setNewDesignName,
    deleteDesignTarget, setDeleteDesignTarget, deletingDesign, setDeletingDesign, editingSlot, setEditingSlot,
    inspectorVisualState, setInspectorVisualState, inspectorTab, setInspectorTab, workspaceShell, dispatchWorkspaceShell,
    interaction, canvasPan, canvasMarquee, spacePressed, workspaceCameraContext, workspaceCameraBeforeSlot,
    slotCameraContext, persistedWorkspaceArtboards, variantPickerDragRef, documentRef, sceneDocumentRef, sceneCommandQueue,
    sceneHistoryRequestId, assetInput, canvasStage, canvasScroll, zoom, interactionZoom,
    canvasPanning, setCanvasPanning, canvasPanReady, setCanvasPanReady, selected, selectedSceneEntry,
    selectedSceneNode, sceneResponsiveRuleSpec, selectedSceneResponsiveOverride, selectedScenePositionEditable, activeScenePage, sceneEditingActive,
    selectedFrame, selectedIdSet, activeWorkspaceArtboard, breakpoint, viewportPresets, configuredViewportSelection,
    activeViewportWidth, sceneViewportMatch, viewportSelection, viewportPreset, previewViewportHeight, renderedCanvasHeight,
    pages, previewOverlayPage, previewOverlayArtboard, selectedPrototypeTarget, activeProjectDocuments, tokens,
    currentPage, pageComponents, editingContainer, editingSlotDefinition, editingSlotComponents, editingVisibleComponents,
    editingSlotCanvasSize, inspectedFrame, inspectedStyle, showToast, updateScenePreviewHeight, chooseLibraryTab,
    activateWorkspaceArea, activateWorkspaceTool, setCurrent, openDocument, commit, commitWithCanvasGrowth,
    changeLive, changeLiveWithCanvasGrowth, historyDocument, applySceneDocument, refreshSceneHistory, refreshGenerationState,
    runGenerationReviewAction, commitSceneCommand, undoScene, redoScene, undo, redo,
    updateComponent, save, refresh, createNew, refreshCatalog, createDesignFromSheet,
    openProjectDocument, goToActiveProject, confirmDeleteProjectDocument, onPaletteDrag, addUiLibraryComponent, scenePageRoot,
    insertSceneLibraryComponent, insertSceneBasicShape, onSceneCanvasDrop, insertUiLibraryComponent, chooseUiLibraryPreviewElement, beginUiLibraryPreviewPointerDrag,
    moveUiLibraryPreviewPointerDragAt, finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent, dropUiLibraryPreviewElement, enterSlotEditor, resetSlotEditorCamera,
    editComponentSlot, exitSlotEditor, insertSlotTemplate, onCanvasDrop, beginInteraction, beginCanvasPan,
    beginCanvasMarquee, updateSelected, updateSelectedFrame, updateInspectedFrame, updateSelectedStyle, clearSelectedVisualState,
    updateSelectedCustomCss, updateSelectedHorizontalConstraint, updateSelectedSizeConstraints, deleteSelected, duplicateSelected, copySelected,
    pasteClipboard, reorderSelected, alignSelected, nudgeSelected, toggleHidden, toggleLocked,
    activateWorkspaceArtboard, updateActiveWorkspaceViewport, addWorkspaceSurface, fitWorkspaceArtboard, fitActiveWorkspaceArtboard, fitSlotEditorContent,
    fitWorkspaceSelection, focusWorkspaceArtboard, focusWorkspaceArtboardByPageId, updateBreakpoint, selectViewportPreset, updateCustomViewportWidth,
    updateCustomViewportHeight, withGeneratedResponsiveLayouts, generateResponsiveLayouts, fitCanvasToWidth, fitCanvasWidth, setCanvasZoom,
    toggleInteractionMode, groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout, saveSelectionAsSymbol,
    insertSymbol, renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet, renameSceneSnippet,
    removeSceneSnippet, applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol, updateSelectedSymbolDefinition,
    updateSelectedLibraryProp, applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme, updateTokenColor,
    applyColorToken, applyRadiusToken, switchPage, addPage, duplicateScenePage, duplicatePage,
    deleteCurrentPage, updateCurrentPage, useAsset, importAssets, downloadTextFile, exportCurrentPage,
    exportReact, exportVue, activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation, prepareSceneAnnotation,
    addSceneAnnotation, changeSceneAnnotationStatus, submitSceneAiInstruction, addAiRequest, renderGenerationReviewPanel, renderWorkspaceArtboard,
    renderPreviewSurfaceOverlay
  } = context as WebDesignActionContext & Record<string, any>;

  function selectComponent(componentId: string, additive = false) {
    const current = documentRef.current;
    const component = current?.components.find((candidate) => candidate.id === componentId);
    const container = current && component ? contentContainerAncestor(current, component) : undefined;
    const slotId = current && component && container ? slotIdForDescendant(current, component, container.id) : undefined;
    if (container && slotId && (editingSlot?.componentId !== container.id || editingSlot.slotId !== slotId)) {
      enterSlotEditor({ componentId: container.id, slotId });
    }
    if (!additive) {
      setSelectedId(componentId);
      setSelectedIds([componentId]);
      return;
    }
    const next = selectedIds.includes(componentId) ? selectedIds.filter((id) => id !== componentId) : [...selectedIds, componentId];
    setSelectedIds(next);
    setSelectedId(next.includes(componentId) ? componentId : next[0]);
  }

  function selectableNodesForCurrentEditor(current: WebDesignDocument): EditorSelectableNode[] {
    if (editingSlot) {
      const container = current.components.find((component) => component.id === editingSlot.componentId);
      if (!container) return [];
      const containerFrame = resolveComponent(container, device);
      return visibleComponentsInSlot(current, editingSlot.componentId, editingSlot.slotId).map((component) => {
        const frame = resolveComponent(component, device);
        return {
          id: component.id,
          name: component.name,
          type: component.library?.component ?? component.type,
          parentId: component.parentId === container.id ? undefined : component.parentId,
          zIndex: component.zIndex,
          locked: component.locked,
          visible: !frame.hidden,
          rect: { x: frame.x - containerFrame.x, y: frame.y - containerFrame.y, width: frame.width, height: frame.height }
        };
      });
    }
    return componentsForPage(current, pageId)
      .filter((component) => !contentContainerAncestor(current, component))
      .map((component) => {
        const frame = resolveComponent(component, device);
        return {
          id: component.id,
          name: component.name,
          type: component.library?.component ?? component.type,
          parentId: component.parentId,
          zIndex: component.zIndex,
          locked: component.locked,
          visible: !frame.hidden,
          rect: { x: frame.x, y: frame.y, width: frame.width, height: frame.height }
        };
      });
  }

  function selectionOverlayItemsFor(
    components: readonly WebDesignComponent[],
    targetDevice: WebDesignDevice,
    origin: { x: number; y: number } = { x: 0, y: 0 }
  ): SelectionOverlayItem[] {
    const byId = new Map(components.map((component) => [component.id, component]));
    return selectedIds.flatMap((id) => {
      const component = byId.get(id);
      if (!component) return [];
      const frame = resolveComponent(component, targetDevice);
      if (frame.hidden) return [];
      return [{
        id: component.id,
        name: component.name,
        locked: Boolean(component.locked),
        primary: component.id === selectedId,
        rect: { x: frame.x - origin.x, y: frame.y - origin.y, width: frame.width, height: frame.height }
      }];
    });
  }

  function selectSelectionChild() {
    if (sceneEditingActive && selectedSceneNode) {
      const children = isSceneContainer(selectedSceneNode)
        ? selectedSceneNode.children
        : isSceneSlotContainer(selectedSceneNode)
          ? Object.values(selectedSceneNode.slots).flat()
          : [];
      const child = children.at(-1);
      if (child) {
        setSelectedId(child.id);
        setSelectedIds([child.id]);
      } else {
        showToast('当前 Scene 图层没有可进入的子层');
      }
      return;
    }
    const current = documentRef.current;
    if (!current || !selectedId) return;
    const selectedComponent = current.components.find((component) => component.id === selectedId);
    const editableSlot = selectedComponent ? editableSlotsForUiComponent(selectedComponent)[0] : undefined;
    if (selectedComponent && editableSlot) {
      void editComponentSlot(selectedComponent, editableSlot.id);
      return;
    }
    const child = deepestSelectionChild(selectableNodesForCurrentEditor(current), selectedId);
    if (child) selectComponent(child.id);
    else showToast('当前图层没有可进入的子层');
  }

  function selectSelectionParent() {
    if (sceneEditingActive && selectedId && sceneDocument) {
      const entry = indexSceneDocument(sceneDocument).get(selectedId);
      if (!entry || sceneDocument.pages.some((page) => page.id === entry.parentId)) {
        showToast('当前已经是画板最外层');
        return;
      }
      setSelectedId(entry.parentId);
      setSelectedIds([entry.parentId]);
      return;
    }
    const current = documentRef.current;
    if (!current || !selectedId) return;
    const component = current.components.find((candidate) => candidate.id === selectedId);
    if (!component?.parentId) {
      showToast('当前已经是最外层');
      return;
    }
    if (editingSlot && component.parentId === editingSlot.componentId) {
      exitSlotEditor();
      return;
    }
    selectComponent(component.parentId);
  }

  function sceneSelectionRootIds(): string[] {
    const scene = sceneDocumentRef.current;
    if (!scene) return [];
    const index = indexSceneDocument(scene);
    const selectedSet = new Set(selectedIds);
    return selectedIds.filter((id) => {
      let parentId = index.get(id)?.parentId;
      while (parentId && index.has(parentId)) {
        if (selectedSet.has(parentId)) return false;
        parentId = index.get(parentId)?.parentId;
      }
      return index.get(id)?.pageId === pageId;
    });
  }

  function cloneSceneSubtree(source: SceneNode, offsetX = 20, offsetY = 20): SceneNode {
    const clone = structuredClone(source);
    const idMap = new Map<string, string>();
    const collect = (node: SceneNode) => {
      idMap.set(node.id, `${node.type}:${crypto.randomUUID()}`);
      if (isSceneContainer(node)) node.children.forEach(collect);
      if (isSceneSlotContainer(node)) Object.values(node.slots).flat().forEach(collect);
    };
    const rewrite = (node: SceneNode, root: boolean) => {
      node.id = idMap.get(node.id)!;
      node.name = root ? `${node.name} 副本` : node.name;
      node.frame = { ...node.frame, ...(root ? { x: node.frame.x + offsetX, y: node.frame.y + offsetY } : {}) };
      node.annotations = [];
      node.createdBy = 'human';
      node.updatedBy = 'human';
      if (node.type === 'component-instance' && idMap.has(node.mainComponentId)) node.mainComponentId = idMap.get(node.mainComponentId)!;
      if (isSceneContainer(node)) node.children.forEach((child) => rewrite(child, false));
      if (isSceneSlotContainer(node)) Object.values(node.slots).flat().forEach((child) => rewrite(child, false));
    };
    collect(clone);
    rewrite(clone, true);
    return clone;
  }

  async function insertSceneCopies(nodes: readonly SceneNode[], targetPageId = pageId, preserveParent = false) {
    let scene = sceneDocumentRef.current;
    if (!scene || nodes.length === 0) return;
    const insertedRootIds: string[] = [];
    for (const source of nodes) {
      scene = sceneDocumentRef.current;
      if (!scene) return;
      const sourceEntry = indexSceneDocument(scene).get(source.id);
      const sourceParent = sourceEntry ? indexSceneDocument(scene).get(sourceEntry.parentId)?.node : undefined;
      const sourceSlot = sourceParent && isSceneSlotContainer(sourceParent)
        ? Object.entries(sourceParent.slots).find(([, children]) => children.some((child) => child.id === source.id))?.[0]
        : undefined;
      const target = preserveParent && sourceEntry
        ? { parentId: sourceEntry.parentId, index: Number.MAX_SAFE_INTEGER, slot: sourceSlot }
        : (() => {
          const { root } = scenePageRoot(scene!, targetPageId);
          return { parentId: root.id, index: root.children.length, slot: undefined };
        })();
      const parentEntry = indexSceneDocument(scene).get(target.parentId)?.node;
      const parentPage = scene.pages.find((candidate) => candidate.id === target.parentId);
      const childCount = parentPage?.children.length
        ?? (parentEntry && isSceneContainer(parentEntry)
          ? parentEntry.children.length
          : parentEntry && isSceneSlotContainer(parentEntry) && target.slot
            ? parentEntry.slots[target.slot]?.length ?? 0
            : 0);
      const copy = cloneSceneSubtree(source);
      await commitSceneCommand({ type: 'insert-node', parentId: target.parentId, index: Math.min(target.index, childCount), slot: target.slot, node: copy }, '用户复制 Scene 图层。');
      insertedRootIds.push(copy.id);
    }
    setSelectedId(insertedRootIds[0]);
    setSelectedIds(insertedRootIds);
    showToast(`已复制 ${insertedRootIds.length} 个 Scene 图层`);
  }

  function copySceneSelection() {
    const scene = sceneDocumentRef.current;
    if (!scene) return;
    const index = indexSceneDocument(scene);
    const nodes = sceneSelectionRootIds().flatMap((id) => {
      const node = index.get(id)?.node;
      return node ? [structuredClone(node)] : [];
    });
    setSceneClipboard(nodes);
    showToast(`已复制 ${nodes.length} 个 Scene 图层`);
  }

  function duplicateSceneSelection() {
    const scene = sceneDocumentRef.current;
    if (!scene) return;
    const index = indexSceneDocument(scene);
    const nodes = sceneSelectionRootIds().flatMap((id) => {
      const node = index.get(id)?.node;
      return node ? [structuredClone(node)] : [];
    });
    void insertSceneCopies(nodes, pageId, true).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
  }

  function pasteSceneClipboard() {
    void insertSceneCopies(sceneClipboard, pageId, false).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
  }

  async function wrapSceneSelection(kind: 'group' | 'frame' | 'auto-horizontal' | 'auto-vertical') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length < 2) {
      showToast('请选择同一容器中的至少两个图层');
      return;
    }
    const wrapperId = `${kind.startsWith('auto') ? 'frame' : kind}:${crypto.randomUUID()}`;
    const command: SceneEditorCommand = kind === 'group'
      ? { type: 'group', nodeIds, wrapperId, name: `分组 · ${nodeIds.length} 项` }
      : kind === 'frame'
        ? { type: 'frame', nodeIds, wrapperId, name: `Frame · ${nodeIds.length} 项`, padding: 16 }
        : {
          type: 'auto-layout-frame',
          nodeIds,
          wrapperId,
          name: kind === 'auto-horizontal' ? '横向 Auto Layout' : '纵向 Auto Layout',
          direction: kind === 'auto-horizontal' ? 'horizontal' : 'vertical',
          padding: 16,
          gap: 16,
          sizingX: 'hug',
          sizingY: 'hug'
        };
    try {
      await commitSceneCommand(command, `在画板中创建 ${command.type}。`);
      setSelectedId(wrapperId);
      setSelectedIds([wrapperId]);
      showToast(command.type === 'group' ? '已创建 Group' : command.type === 'frame' ? '已创建 Frame' : '已创建 Auto Layout');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function ungroupSceneSelection() {
    if (!selectedSceneNode || (selectedSceneNode.type !== 'group' && selectedSceneNode.type !== 'frame') || selectedSceneNode.layout.mode !== 'free') return;
    const childIds = selectedSceneNode.children.map((child) => child.id);
    try {
      await commitSceneCommand({ type: 'ungroup', wrapperId: selectedSceneNode.id }, '用户取消 Scene 分组。');
      setSelectedId(childIds[0]);
      setSelectedIds(childIds);
      showToast('已取消分组');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function nudgeSceneSelection(deltaX: number, deltaY: number) {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length === 0) return;
    try {
      await commitSceneCommand({ type: 'move', nodeIds, deltaX, deltaY }, '用户使用键盘微调 Scene 图层。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function alignSceneSelection(alignment: 'left' | 'horizontal-center' | 'right' | 'top' | 'vertical-center' | 'bottom') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length < 2) return;
    try {
      await commitSceneCommand({ type: 'align', nodeIds, alignment }, '用户对齐 Scene 图层。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function distributeSceneSelection(axis: 'horizontal' | 'vertical') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length < 3) return;
    try {
      await commitSceneCommand({ type: 'distribute', nodeIds, axis }, '用户等间距分布 Scene 图层。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function reorderSceneSelection(placement: 'front' | 'forward' | 'backward' | 'back') {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length === 0) return;
    try {
      await commitSceneCommand({ type: 'reorder', nodeIds, placement }, '用户调整 Scene 图层顺序。');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function deleteSceneSelection() {
    const nodeIds = sceneSelectionRootIds();
    if (nodeIds.length === 0) return;
    try {
      await commitSceneCommand({ type: 'delete-nodes', nodeIds }, '用户从画板删除 Scene 图层。');
      setSelectedId(undefined);
      setSelectedIds([]);
      showToast(`已删除 ${nodeIds.length} 个图层`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function updateSceneNodeById(nodeId: string, patches: Array<{ path: string[]; value: unknown }>, reason = '用户在属性栏调整 Scene 图层。') {
    try {
      await commitSceneCommand({ type: 'update-node', nodeId, patches }, reason);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function updateSceneNode(patches: Array<{ path: string[]; value: unknown }>, reason = '用户在属性栏调整 Scene 图层。') {
    if (!selectedSceneNode) return;
    await updateSceneNodeById(selectedSceneNode.id, patches, reason);
  }

  async function applySelectedSceneLibraryVariant(variantId: string) {
    const selectedSceneLibrary = context.selectedSceneLibrary as ReturnType<typeof uiLibraryByName>;
    const selectedSceneLibraryDefinition = context.selectedSceneLibraryDefinition as UiComponentDefinition | undefined;
    const selectedSceneLibraryVariants = context.selectedSceneLibraryVariants as UiComponentVariant[];
    if (selectedSceneNode?.type !== 'library-instance' || !selectedSceneLibrary || !selectedSceneLibraryDefinition) return;
    const variant = selectedSceneLibraryVariants.find((candidate) => candidate.id === variantId);
    if (!variant) return;
    const variantKeys = new Set(selectedSceneLibraryVariants.flatMap((candidate) => Object.keys(candidate.props)));
    const customProperties = Object.fromEntries(Object.entries(selectedSceneNode.properties).filter(([key]) => !variantKeys.has(key)));
    const patches: Array<{ path: string[]; value: unknown }> = [
      { path: ['variant'], value: variant.id },
      { path: ['properties'], value: { ...(selectedSceneLibraryDefinition.props ?? {}), ...customProperties, ...variant.props } }
    ];
    if (variant.content !== undefined) patches.push({ path: ['content'], value: variant.content });
    if (variant.width !== undefined) patches.push({ path: ['frame', 'width'], value: variant.width });
    if (variant.height !== undefined) patches.push({ path: ['frame', 'height'], value: variant.height });
    await updateSceneNode(patches, `用户切换 ${selectedSceneLibrary.displayName} ${selectedSceneLibraryDefinition.label} 的官方款式。`);
  }

  function focusSceneContent(nodeId: string, slot?: string) {
    if (!selectedSceneEntry) return;
    setSceneContentFocus({ pageId: selectedSceneEntry.pageId, nodeId, ...(slot ? { slot } : {}) });
    showToast(slot ? '已进入内容区；现在拖入的组件会直接放到这里' : '已进入容器；现在拖入的组件会直接放到这里');
  }

  async function updateSelectedSceneResponsiveOverride(changes: Omit<SceneResponsiveNodeOverride, 'nodeId'>) {
    if (!selectedSceneNode || !sceneResponsiveRuleSpec) return;
    const current = selectedSceneResponsiveOverride ?? {};
    const next = { ...structuredClone(current), ...structuredClone(changes) };
    await commitSceneCommand({
      type: 'set-responsive-override',
      ...sceneResponsiveRuleSpec,
      nodeId: selectedSceneNode.id,
      override: next
    }, `用户调整 ${device} Scene 响应式布局。`);
  }

  async function replaceSelectedSceneResponsiveOverride(next: Omit<SceneResponsiveNodeOverride, 'nodeId'>) {
    if (!selectedSceneNode || !sceneResponsiveRuleSpec) return;
    if (next.visible === undefined && next.layout === undefined && next.childOrder === undefined) {
      await clearSelectedSceneResponsiveOverride();
      return;
    }
    await commitSceneCommand({
      type: 'set-responsive-override',
      ...sceneResponsiveRuleSpec,
      nodeId: selectedSceneNode.id,
      override: next
    }, `用户调整 ${device} Scene 响应式布局。`);
  }

  async function clearSelectedSceneResponsiveOverride() {
    if (!selectedSceneNode || !sceneResponsiveRuleSpec || !selectedSceneResponsiveOverride) return;
    await commitSceneCommand({
      type: 'clear-responsive-override', ruleId: sceneResponsiveRuleSpec.ruleId, nodeId: selectedSceneNode.id
    }, `用户恢复 ${device} Scene 响应式继承。`);
  }


  return {
    selectComponent,
    selectableNodesForCurrentEditor,
    selectionOverlayItemsFor,
    selectSelectionChild,
    selectSelectionParent,
    sceneSelectionRootIds,
    cloneSceneSubtree,
    insertSceneCopies,
    copySceneSelection,
    duplicateSceneSelection,
    pasteSceneClipboard,
    wrapSceneSelection,
    ungroupSceneSelection,
    nudgeSceneSelection,
    alignSceneSelection,
    distributeSceneSelection,
    reorderSceneSelection,
    deleteSceneSelection,
    updateSceneNodeById,
    updateSceneNode,
    applySelectedSceneLibraryVariant,
    focusSceneContent,
    updateSelectedSceneResponsiveOverride,
    replaceSelectedSceneResponsiveOverride,
    clearSelectedSceneResponsiveOverride
  };
}
