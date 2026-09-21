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
import type { UiComponentVariant, UiEditableSlot } from '../../src/ui-library';
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

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions>;

export function createWebDesignViewportActions(context: Record<string, any>) {
  const {
    actionsRef, repository, setRepository, documents, setDocuments, activeProject,
    setActiveProject, document, setDocument, sceneDocument, setSceneDocument, sceneHistory,
    setSceneHistory, sceneLoadState, setSceneLoadState, sceneReloadToken, setSceneReloadToken, ready,
    setReady, screen, setScreen, persistedRevision, setPersistedRevision, selectedId,
    setSelectedId, selectedIds, setSelectedIds, selectionCandidatePopover, setSelectionCandidatePopover, marqueeRect,
    setMarqueeRect, pageId, setPageId, clipboard, setClipboard, sceneClipboard,
    setSceneClipboard, snapGuides, setSnapGuides, dirty, setDirty, saving,
    setSaving, previewOverlayPageId, setPreviewOverlayPageId, interactionMode, setInteractionMode, device,
    setDevice, viewportSelections, setViewportSelections, workspaceCamera, setWorkspaceCamera, workspacePlacement,
    setWorkspacePlacement, activeArtboardId, setActiveArtboardId, scenePreviewHeights, setScenePreviewHeights, newSurfaceKind,
    setNewSurfaceKind, past, setPast, future, setFuture, toast,
    setToast, annotationText, setAnnotationText, aiInstruction, setAiInstruction, sceneAiContext,
    setSceneAiContext, sceneAnnotationPreparingId, setSceneAnnotationPreparingId, generationPlan, setGenerationPlan, generationReview,
    setGenerationReview, generationLoading, setGenerationLoading, generationAction, setGenerationAction, generationRejectionReason,
    setGenerationRejectionReason, paletteQuery, setPaletteQuery, libraryTab, setLibraryTab, personalSymbols,
    setPersonalSymbols, sceneSnippets, setSceneSnippets, sceneVariablesDraft, setSceneVariablesDraft, variantPickerTarget,
    setVariantPickerTarget, sceneContentFocus, setSceneContentFocus, variantPickerDrag, setVariantPickerDrag, themePickerOpen,
    setThemePickerOpen, projectLibraryOpen, setProjectLibraryOpen, newDesignOpen, setNewDesignOpen, newDesignName,
    setNewDesignName, deleteDesignTarget, setDeleteDesignTarget, deletingDesign, setDeletingDesign, editingSlot,
    setEditingSlot, inspectorVisualState, setInspectorVisualState, inspectorTab, setInspectorTab, workspaceShell,
    dispatchWorkspaceShell, interaction, canvasPan, canvasMarquee, spacePressed, workspaceCameraContext,
    workspaceCameraBeforeSlot, slotCameraContext, persistedWorkspaceArtboards, variantPickerDragRef, documentRef, sceneDocumentRef,
    sceneCommandQueue, sceneHistoryRequestId, assetInput, canvasStage, canvasScroll, zoom,
    interactionZoom, canvasPanning, setCanvasPanning, canvasPanReady, setCanvasPanReady, selected,
    selectedSceneEntry, selectedSceneNode, sceneResponsiveRuleSpec, selectedSceneResponsiveOverride, selectedScenePositionEditable, activeScenePage,
    sceneEditingActive, selectedFrame, selectedIdSet, activeWorkspaceArtboard, breakpoint, viewportPresets,
    configuredViewportSelection, activeViewportWidth, sceneViewportMatch, viewportSelection, viewportPreset, previewViewportHeight,
    renderedCanvasHeight, pages, previewOverlayPage, previewOverlayArtboard, selectedPrototypeTarget, activeProjectDocuments,
    tokens, currentPage, pageComponents, editingContainer, editingSlotDefinition, editingSlotComponents,
    editingVisibleComponents, editingSlotCanvasSize, inspectedFrame, inspectedStyle, showToast, updateScenePreviewHeight,
    chooseLibraryTab, activateWorkspaceArea, activateWorkspaceTool, setCurrent, openDocument, commit,
    commitWithCanvasGrowth, changeLive, changeLiveWithCanvasGrowth, historyDocument, applySceneDocument, refreshSceneHistory,
    refreshGenerationState, runGenerationReviewAction, commitSceneCommand, undoScene, redoScene, undo,
    redo, updateComponent, save, refresh, createNew, refreshCatalog,
    createDesignFromSheet, openProjectDocument, goToActiveProject, confirmDeleteProjectDocument, onPaletteDrag, addUiLibraryComponent,
    scenePageRoot, insertSceneLibraryComponent, insertSceneBasicShape, onSceneCanvasDrop, insertUiLibraryComponent, chooseUiLibraryPreviewElement,
    beginUiLibraryPreviewPointerDrag, moveUiLibraryPreviewPointerDragAt, finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent, dropUiLibraryPreviewElement, enterSlotEditor,
    resetSlotEditorCamera, editComponentSlot, exitSlotEditor, insertSlotTemplate, onCanvasDrop, beginInteraction,
    beginCanvasPan, beginCanvasMarquee, updateSelected, updateSelectedFrame, updateInspectedFrame, updateSelectedStyle,
    clearSelectedVisualState, updateSelectedCustomCss, updateSelectedHorizontalConstraint, updateSelectedSizeConstraints, deleteSelected, duplicateSelected,
    copySelected, pasteClipboard, reorderSelected, alignSelected, nudgeSelected, toggleHidden,
    toggleLocked, nodeIds, wrapperId, deltaX, nodeId, patches,
    selectComponent, selectableNodesForCurrentEditor, selectionOverlayItemsFor, selectSelectionChild, selectSelectionParent, sceneSelectionRootIds,
    cloneSceneSubtree, insertSceneCopies, copySceneSelection, duplicateSceneSelection, pasteSceneClipboard, wrapSceneSelection,
    ungroupSceneSelection, nudgeSceneSelection, alignSceneSelection, distributeSceneSelection, reorderSceneSelection, deleteSceneSelection,
    updateSceneNodeById, updateSceneNode, applySelectedSceneLibraryVariant, focusSceneContent, updateSelectedSceneResponsiveOverride, replaceSelectedSceneResponsiveOverride,
    clearSelectedSceneResponsiveOverride, groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout, saveSelectionAsSymbol,
    insertSymbol, renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet, renameSceneSnippet,
    removeSceneSnippet, applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol, updateSelectedSymbolDefinition,
    updateSelectedLibraryProp, applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme, updateTokenColor,
    applyColorToken, applyRadiusToken, switchPage, addPage, duplicateScenePage, duplicatePage,
    deleteCurrentPage, updateCurrentPage, useAsset, importAssets, downloadTextFile, exportCurrentPage,
    exportReact, exportVue, activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation, prepareSceneAnnotation,
    addSceneAnnotation, changeSceneAnnotationStatus, submitSceneAiInstruction, addAiRequest, renderGenerationReviewPanel, renderWorkspaceArtboard,
    renderPreviewSurfaceOverlay
  } = context as WebDesignActionContext & Record<string, any>;

  function activateWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    const current = documentRef.current;
    if (!current) return;
    setActiveArtboardId(artboard.artboardId);
    setDevice(deviceForWorkspaceArtboard(current, artboard));
    setPageId(artboard.pageId);
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
    fitWorkspaceArtboard(artboard);
  }

  async function updateActiveWorkspaceViewport(width: number, height: number) {
    const targetArtboardId = activeArtboardId;
    const target = workspacePlacement?.artboards.find((artboard) => artboard.artboardId === targetArtboardId);
    if (!targetArtboardId || !target) return;
    const safeWidth = Math.min(10000, Math.max(240, Math.round(width)));
    const safeHeight = Math.min(50000, Math.max(240, Math.round(height)));
    setWorkspacePlacement((current) => current ? {
      ...current,
      artboards: updateWorkspaceArtboardById(current.artboards, targetArtboardId, {
        viewportWidth: safeWidth,
        viewportHeight: safeHeight
      })
    } : current);
    const currentDocument = documentRef.current;
    if (currentDocument) setDevice(deviceForWorkspaceArtboard(currentDocument, {
      ...target,
      viewportWidth: safeWidth,
      viewportHeight: safeHeight
    }));

    const scene = sceneDocumentRef.current;
    const root = scene?.pages.find((page) => page.id === target.pageId)?.children[0];
    if (!root || root.frame.width === safeWidth && root.layout.sizingX === 'fill') return;
    try {
      await commitSceneCommand({
        type: 'update-node',
        nodeId: root.id,
        patches: [
          { path: ['frame', 'width'], value: safeWidth },
          { path: ['layout', 'sizingX'], value: 'fill' }
        ]
      }, `用户把当前画板“${scene?.pages.find((page) => page.id === target.pageId)?.name ?? target.pageId}”宽度调整为 ${safeWidth}px。`);
    } catch (error) {
      setWorkspacePlacement((current) => current ? {
        ...current,
        artboards: updateWorkspaceArtboardById(current.artboards, targetArtboardId, {
          viewportWidth: target.viewportWidth,
          viewportHeight: target.viewportHeight
        })
      } : current);
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function addWorkspaceSurface(surfaceKind: WorkspaceSurfaceKind = newSurfaceKind) {
    const current = documentRef.current;
    const scene = sceneDocumentRef.current;
    if (!current || !workspacePlacement) return;
    if (!scene) {
      showToast('请先让 AI 创建 Scene 设计，再添加新的独立画板');
      return;
    }
    const pageIndex = scene.pages.length + 1;
    const surfaceLabel = WORKSPACE_SURFACE_LABELS[surfaceKind];
    const nextPageId = `${surfaceKind}:${crypto.randomUUID()}`;
    const nextPageName = surfaceKind === 'page' ? `页面 ${pageIndex}` : `${surfaceLabel} ${pageIndex}`;
    const desktop = breakpointFor(current, 'desktop');
    const fixedSize = surfaceKind === 'page' || surfaceKind === 'state' ? undefined : WORKSPACE_SURFACE_SIZES[surfaceKind];
    const width = fixedSize?.width ?? desktop.width;
    const height = fixedSize?.height ?? workspaceViewportHeight(current, 'desktop');
    const artboard: WorkspaceArtboardPlacement = {
      artboardId: `artboard-${crypto.randomUUID().slice(0, 8)}`,
      pageId: nextPageId,
      surfaceKind,
      viewportWidth: width,
      viewportHeight: height,
      x: 0,
      y: 0
    };
    try {
      await commitSceneCommand({
        type: 'create-page',
        pageId: nextPageId,
        name: nextPageName,
        rootNodeId: `root:${crypto.randomUUID()}`,
        width,
        height
      }, `用户创建独立${surfaceLabel}画板。`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
      return;
    }
    setWorkspacePlacement({ ...workspacePlacement, artboards: [...workspacePlacement.artboards, artboard] });
    setActiveArtboardId(artboard.artboardId);
    setPageId(nextPageId);
    setDevice(deviceForWorkspaceArtboard(current, artboard));
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
    showToast(`已创建独立${surfaceLabel}画板，可以分多次让 AI 继续设计`);
  }

  function fitWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    const current = documentRef.current;
    const viewport = canvasScroll.current;
    if (!current || !viewport) return;
    setWorkspaceCamera(fitWorkspaceRect(
      workspaceArtboardContentBounds(current, { ...artboard, x: 0, y: 0 }, sceneDocumentRef.current),
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 92, right: 64, bottom: 92, left: 64 }
    ));
  }

  function fitActiveWorkspaceArtboard() {
    const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId);
    if (artboard) fitWorkspaceArtboard(artboard);
  }

  function fitSlotEditorContent() {
    const viewport = canvasScroll.current;
    if (!viewport || !editingSlotCanvasSize) return;
    setWorkspaceCamera(fitWorkspaceRect(
      slotEditorFrameBounds(editingSlotCanvasSize),
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 112, right: 72, bottom: 88, left: 72 },
      2.5
    ));
  }

  function fitWorkspaceSelection() {
    const current = documentRef.current;
    const viewport = canvasScroll.current;
    if (!current || !viewport || selectedIds.length === 0) return;
    const selected = new Set(selectedIds);
    if (editingSlot && editingContainer) {
      const containerFrame = resolveComponent(editingContainer, device);
      const bounds = unionWorkspaceRects(current.components.flatMap((component) => {
        if (!selected.has(component.id) || slotIdForDescendant(current, component, editingContainer.id) !== editingSlot.slotId) return [];
        const frame = resolveComponent(component, device);
        if (frame.hidden) return [];
        return [{
          x: SLOT_EDITOR_CANVAS_INSETS.left + frame.x - containerFrame.x,
          y: SLOT_EDITOR_HEADER_HEIGHT + SLOT_EDITOR_CANVAS_INSETS.top + frame.y - containerFrame.y,
          width: frame.width,
          height: frame.height
        }];
      }));
      if (!bounds) return;
      const padding = Math.max(24, Math.min(80, Math.max(bounds.width, bounds.height) * 0.12));
      setWorkspaceCamera(fitWorkspaceRect(
        { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
        { width: viewport.clientWidth, height: viewport.clientHeight },
        { top: 112, right: 96, bottom: 104, left: 96 },
        2.5
      ));
      return;
    }
    if (!workspacePlacement) return;
    if (sceneDocument) {
      const artboard = activeWorkspaceArtboard;
      const local = artboard
        ? sceneArtboardSelectionBounds(sceneDocument, artboard.pageId, artboard.viewportWidth, selectedIds)
        : undefined;
      const bounds = local ? { ...local } : undefined;
      if (!bounds) return;
      const padding = Math.max(24, Math.min(80, Math.max(bounds.width, bounds.height) * 0.12));
      setWorkspaceCamera(fitWorkspaceRect(
        { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
        { width: viewport.clientWidth, height: viewport.clientHeight },
        { top: 112, right: 96, bottom: 104, left: 96 },
        2.5
      ));
      return;
    }
    const artboard = activeWorkspaceArtboard;
    if (!artboard) return;
    const bounds = unionWorkspaceRects(current.components.flatMap((component) => {
      if (!selected.has(component.id) || (component.pageId ?? pagesForDocument(current)[0].id) !== artboard.pageId) return [];
      const frame = resolveComponent(component, deviceForWorkspaceArtboard(current, artboard));
      if (frame.hidden) return [];
      return [{ x: frame.x, y: frame.y, width: frame.width, height: frame.height }];
    }));
    if (!bounds) return;
    const padding = Math.max(24, Math.min(80, Math.max(bounds.width, bounds.height) * 0.12));
    setWorkspaceCamera(fitWorkspaceRect(
      { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
      { width: viewport.clientWidth, height: viewport.clientHeight },
      { top: 112, right: 96, bottom: 104, left: 96 },
      2.5
    ));
  }

  function focusWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    activateWorkspaceArtboard(artboard);
  }

  function focusWorkspaceArtboardByPageId(targetPageId: string) {
    const target = workspacePlacement?.artboards.find((artboard) => artboard.pageId === targetPageId);
    if (target) focusWorkspaceArtboard(target);
  }

  function updateBreakpoint(width: number, height: number, previewSelection?: ViewportSelection, reflow = false) {
    const safeWidth = Math.min(10000, Math.max(320, Math.round(width)));
    const safeHeight = Math.min(50000, Math.max(240, Math.round(height)));
    commitWithCanvasGrowth((current) => {
      const previousWidth = breakpointFor(current, device).width;
      const reflowed = reflow && previousWidth !== safeWidth
        ? pagesForDocument(current).reduce(
          (next, page) => reflowPageForViewport(next, page.id, device, previousWidth, safeWidth),
          current
        )
        : current;
      const breakpoints = {
        desktop: { ...(reflowed.breakpoints?.desktop ?? { width: reflowed.viewport.width, height: reflowed.viewport.height }) },
        tablet: { ...(reflowed.breakpoints?.tablet ?? { width: 768, height: 1100 }) },
        mobile: { ...(reflowed.breakpoints?.mobile ?? { width: 390, height: 844 }) }
      };
      breakpoints[device] = {
        ...breakpoints[device],
        width: safeWidth,
        height: safeHeight,
        preview: previewSelection ? {
          presetId: previewSelection.presetId,
          orientation: previewSelection.orientation,
          viewportHeight: previewSelection.customHeight
        } : breakpoints[device].preview
      };
      return { ...reflowed, breakpoints, viewport: device === 'desktop' ? { ...reflowed.viewport, width: safeWidth, height: safeHeight } : reflowed.viewport };
    });
  }

  async function selectViewportPreset(presetId: string) {
    const preset = viewportPresets.find((candidate) => candidate.id === presetId);
    if (!preset) return;
    const dimensions = viewportDimensions(preset, 'default');
    const selection: ViewportSelection = { presetId: preset.id, orientation: 'default', customHeight: dimensions.height };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    await updateActiveWorkspaceViewport(dimensions.width, dimensions.height);
    if (sceneDocumentRef.current) {
      return;
    }
    updateBreakpoint(dimensions.width, breakpoint.height, selection, true);
    window.setTimeout(() => fitCanvasToWidth(dimensions.width), 0);
  }

  function updateCustomViewportWidth(width: number) {
    if (!Number.isFinite(width)) return;
    const selection: ViewportSelection = { ...viewportSelection, presetId: undefined };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    void updateActiveWorkspaceViewport(width, previewViewportHeight);
    if (sceneDocumentRef.current) return;
    updateBreakpoint(width, breakpoint.height, selection, true);
  }

  function updateCustomViewportHeight(height: number) {
    if (!Number.isFinite(height)) return;
    const safeHeight = Math.min(30000, Math.max(320, Math.round(height)));
    const selection: ViewportSelection = { presetId: undefined, orientation: 'default', customHeight: safeHeight };
    setViewportSelections((current) => ({
      ...current,
      [device]: selection
    }));
    void updateActiveWorkspaceViewport(activeViewportWidth, safeHeight);
    if (sceneDocumentRef.current) return;
    updateBreakpoint(activeViewportWidth, breakpoint.height, selection);
  }

  function withGeneratedResponsiveLayouts(active: WebDesignDocument, targetPageId: string) {
    const desktopWidth = breakpointFor(active, 'desktop').width;
    const tabletWidth = breakpointFor(active, 'tablet').width;
    const mobileWidth = breakpointFor(active, 'mobile').width;
    const withTablet = deriveResponsivePageFromDevice(active, targetPageId, 'desktop', 'tablet', desktopWidth, tabletWidth);
    return deriveResponsivePageFromDevice(withTablet, targetPageId, 'desktop', 'mobile', desktopWidth, mobileWidth);
  }

  function generateResponsiveLayouts() {
    if (!documentRef.current) return;
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts(active, pageId), ['tablet', 'mobile']);
    showToast('已补齐中等与窄宽度布局，已有人工调整保持不变');
  }

  function fitCanvasToWidth(targetWidth: number) {
    const viewport = canvasScroll.current;
    if (!viewport) return;
    const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId);
    setWorkspaceCamera(fitWorkspaceWidth(
      { x: artboard?.x ?? 0, y: artboard?.y ?? 0, width: targetWidth, height: renderedCanvasHeight },
      { width: viewport.clientWidth, height: viewport.clientHeight }
    ));
  }

  function fitCanvasWidth() {
    fitCanvasToWidth(activeViewportWidth);
  }

  function setCanvasZoom(nextZoom: number, anchor?: { x: number; y: number }) {
    const viewport = canvasScroll.current;
    if (!viewport) {
      setWorkspaceCamera((current) => ({ ...current, zoom: nextZoom }));
      return;
    }
    setWorkspaceCamera((current) => zoomWorkspaceCameraAt(current, nextZoom, anchor ?? {
      x: viewport.clientWidth / 2,
      y: viewport.clientHeight / 2
    }));
  }

  function toggleInteractionMode() {
    if (interactionMode) {
      setInteractionMode(false);
      setWorkspaceCamera((current) => ({ ...current, zoom: interactionZoom.current }));
      return;
    }
    interactionZoom.current = zoom;
    setSelectedId(undefined);
    setSelectedIds([]);
    setInteractionMode(true);
    window.setTimeout(() => fitCanvasWidth(), 0);
  }


  return {
    activateWorkspaceArtboard,
    updateActiveWorkspaceViewport,
    addWorkspaceSurface,
    fitWorkspaceArtboard,
    fitActiveWorkspaceArtboard,
    fitSlotEditorContent,
    fitWorkspaceSelection,
    focusWorkspaceArtboard,
    focusWorkspaceArtboardByPageId,
    updateBreakpoint,
    selectViewportPreset,
    updateCustomViewportWidth,
    updateCustomViewportHeight,
    withGeneratedResponsiveLayouts,
    generateResponsiveLayouts,
    fitCanvasToWidth,
    fitCanvasWidth,
    setCanvasZoom,
    toggleInteractionMode
  };
}
