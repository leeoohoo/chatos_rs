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

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState>;

export function createWebDesignCoreActions(context: Record<string, any>) {
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
    editingVisibleComponents, editingSlotCanvasSize, inspectedFrame, inspectedStyle, onPaletteDrag, addUiLibraryComponent,
    scenePageRoot, insertSceneLibraryComponent, insertSceneBasicShape, onSceneCanvasDrop, insertUiLibraryComponent, chooseUiLibraryPreviewElement,
    beginUiLibraryPreviewPointerDrag, moveUiLibraryPreviewPointerDragAt, finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent, dropUiLibraryPreviewElement, enterSlotEditor,
    resetSlotEditorCamera, editComponentSlot, exitSlotEditor, insertSlotTemplate, onCanvasDrop, beginInteraction,
    beginCanvasPan, beginCanvasMarquee, updateSelected, updateSelectedFrame, updateInspectedFrame, updateSelectedStyle,
    clearSelectedVisualState, updateSelectedCustomCss, updateSelectedHorizontalConstraint, updateSelectedSizeConstraints, deleteSelected, duplicateSelected,
    copySelected, pasteClipboard, reorderSelected, alignSelected, nudgeSelected, toggleHidden,
    toggleLocked, activateWorkspaceArtboard, updateActiveWorkspaceViewport, addWorkspaceSurface, fitWorkspaceArtboard, fitActiveWorkspaceArtboard,
    fitSlotEditorContent, fitWorkspaceSelection, focusWorkspaceArtboard, focusWorkspaceArtboardByPageId, updateBreakpoint, selectViewportPreset,
    updateCustomViewportWidth, updateCustomViewportHeight, withGeneratedResponsiveLayouts, generateResponsiveLayouts, fitCanvasToWidth, fitCanvasWidth,
    setCanvasZoom, toggleInteractionMode, nodeIds, wrapperId, deltaX, nodeId,
    patches, selectComponent, selectableNodesForCurrentEditor, selectionOverlayItemsFor, selectSelectionChild, selectSelectionParent,
    sceneSelectionRootIds, cloneSceneSubtree, insertSceneCopies, copySceneSelection, duplicateSceneSelection, pasteSceneClipboard,
    wrapSceneSelection, ungroupSceneSelection, nudgeSceneSelection, alignSceneSelection, distributeSceneSelection, reorderSceneSelection,
    deleteSceneSelection, updateSceneNodeById, updateSceneNode, applySelectedSceneLibraryVariant, focusSceneContent, updateSelectedSceneResponsiveOverride,
    replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride, groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout,
    saveSelectionAsSymbol, insertSymbol, renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet,
    renameSceneSnippet, removeSceneSnippet, applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol,
    updateSelectedSymbolDefinition, updateSelectedLibraryProp, applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme,
    updateTokenColor, applyColorToken, applyRadiusToken, switchPage, addPage, duplicateScenePage,
    duplicatePage, deleteCurrentPage, updateCurrentPage, useAsset, importAssets, downloadTextFile,
    exportCurrentPage, exportReact, exportVue, activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation,
    prepareSceneAnnotation, addSceneAnnotation, changeSceneAnnotationStatus, submitSceneAiInstruction, addAiRequest, renderGenerationReviewPanel,
    renderWorkspaceArtboard, renderPreviewSurfaceOverlay
  } = context as WebDesignActionContext & Record<string, any>;

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast((current) => current === message ? undefined : current), 2600);
  }

  function updateScenePreviewHeight(targetPageId: string, height?: number) {
    setScenePreviewHeights((current) => {
      if (height === undefined) {
        if (!(targetPageId in current)) return current;
        const next = { ...current };
        delete next[targetPageId];
        return next;
      }
      if (current[targetPageId] === height) return current;
      return { ...current, [targetPageId]: height };
    });
  }

  function chooseLibraryTab(tab: LibraryTab) {
    setLibraryTab(tab);
    const area: WorkspaceArea = tab === 'layers' ? 'layers' : tab === 'components' ? 'tools' : tab === 'my' ? 'my' : 'assets';
    dispatchWorkspaceShell({ type: 'select-area', area });
  }

  function activateWorkspaceArea(area: WorkspaceArea) {
    dispatchWorkspaceShell({ type: 'select-area', area });
    if (area === 'layers' || area === 'variables' || area === 'ai') setLibraryTab('layers');
    else if (area === 'tools') setLibraryTab('components');
    else if (area === 'my') setLibraryTab('my');
    else if (libraryTab === 'layers' || libraryTab === 'components' || libraryTab === 'my') setLibraryTab('antd');
  }

  function activateWorkspaceTool(tool: WorkspaceTool) {
    dispatchWorkspaceShell({ type: 'select-tool', tool });
    if (tool === 'ai') {
      activateWorkspaceArea('ai');
      return;
    }
    if (tool === 'insert') {
      activateWorkspaceArea('tools');
      showToast('从左侧选择元素，再拖到画布');
      return;
    }
    if (tool === 'comment') {
      setInspectorTab('ai');
      if (!workspaceShell.rightPanelOpen) dispatchWorkspaceShell({ type: 'toggle-right-panel' });
      showToast(selected ? '可以在右侧添加批注或让 AI 修改' : '批注工具已开启，请点击画布中的组件');
    }
  }

  function setCurrent(next: WebDesignDocument) {
    documentRef.current = next;
    setDocument(next);
  }

  function openDocument(next: WebDesignDocument) {
    const opened = structuredClone(next);
    workspaceCameraBeforeSlot.current = undefined;
    slotCameraContext.current = undefined;
    workspaceCameraContext.current = undefined;
    persistedWorkspaceArtboards.current = '';
    setWorkspacePlacement(undefined);
    setActiveArtboardId(undefined);
    sceneDocumentRef.current = undefined;
    setSceneDocument(undefined);
    setSceneHistory(undefined);
    setSceneLoadState('loading');
    setSceneReloadToken((value) => value + 1);
    setCurrent(opened);
    setPersistedRevision(next.revision);
    setSelectedId(undefined);
    setSelectedIds([]);
    setDirty(false);
    setPast([]);
    setFuture([]);
    setDevice('desktop');
    setViewportSelections(viewportSelectionsForDocument(opened));
    setPageId(pagesForDocument(next)[0].id);
    setEditingSlot(undefined);
  }

  function commit(updater: (current: WebDesignDocument) => WebDesignDocument) {
    const current = documentRef.current;
    if (!current) return;
    const next = updater(current);
    setPast((items) => [...items.slice(-59), structuredClone(current)]);
    setFuture([]);
    setCurrent(next);
    setDirty(true);
  }

  function commitWithCanvasGrowth(
    updater: (current: WebDesignDocument) => WebDesignDocument,
    targetDevices: readonly WebDesignDevice[] = [device],
    targetPageId = pageId
  ) {
    commit((current) => targetDevices.reduce(
      (next, targetDevice) => growCanvasForDevice(next, targetPageId, targetDevice),
      updater(current)
    ));
  }

  function changeLive(updater: (current: WebDesignDocument) => WebDesignDocument) {
    const current = documentRef.current;
    if (!current) return;
    setCurrent(updater(current));
    setDirty(true);
  }

  function changeLiveWithCanvasGrowth(
    updater: (current: WebDesignDocument) => WebDesignDocument,
    targetPageId = pageId,
    targetDevice = device
  ) {
    changeLive((current) => growCanvasForDevice(
      updater(current),
      targetPageId,
      targetDevice,
      breakpointFor(current, targetDevice).height
    ));
  }

  function historyDocument(snapshot: WebDesignDocument, current: WebDesignDocument): WebDesignDocument {
    return { ...structuredClone(snapshot), revision: current.revision, createdAt: current.createdAt, updatedAt: current.updatedAt };
  }

  function applySceneDocument(next: SceneDocument) {
    sceneDocumentRef.current = next;
    setSceneDocument(next);
    setSceneLoadState('ready');
  }

  async function refreshSceneHistory(documentId: string) {
    if (!repository) return;
    const requestId = ++sceneHistoryRequestId.current;
    const history = await repository.readSceneHistory(documentId);
    if (requestId === sceneHistoryRequestId.current) setSceneHistory(history);
  }

  async function refreshGenerationState(showLoading = false) {
    const currentDocument = documentRef.current;
    if (!repository || !currentDocument || repository.mode !== 'server') return;
    if (showLoading) setGenerationLoading(true);
    try {
      const plan = await repository.readGenerationPlan(currentDocument.documentId);
      setGenerationPlan(plan);
      const activeStep = plan?.activeStep;
      if (activeStep?.stepId && activeStep.activeAttemptId) {
        setGenerationReview(await repository.inspectGenerationStep(currentDocument.documentId, activeStep.stepId, activeStep.activeAttemptId));
      } else {
        setGenerationReview(undefined);
      }
    } finally {
      if (showLoading) setGenerationLoading(false);
    }
  }

  async function runGenerationReviewAction(action: 'accept' | 'reject' | 'rollback' | 'pause' | 'resume') {
    const currentDocument = documentRef.current;
    const plan = generationPlan;
    if (!repository || !currentDocument || !plan || generationAction) return;
    const review = generationReview;
    setGenerationAction(action);
    try {
      if (action === 'accept') {
        if (!review?.candidate) throw new Error('当前步骤还没有可接受的设计候选。');
        let result = await repository.acceptGenerationStep(currentDocument.documentId, plan.revision, review.step.stepId, review.candidate.attemptId);
        if (result.status === 'requires-protection-review') {
          const approved = window.confirm('这个候选会修改你人工调整过的字段。是否明确允许本次覆盖？');
          if (!approved) return;
          result = await repository.acceptGenerationStep(currentDocument.documentId, result.plan.revision, review.step.stepId, review.candidate.attemptId, true);
        }
        showToast(result.status === 'committed' ? '已接受这一小步，AI 可以继续下一步' : `候选状态：${result.status}`);
        setSceneReloadToken((value) => value + 1);
      } else if (action === 'reject') {
        if (!review?.candidate) throw new Error('当前步骤还没有可退回的设计候选。');
        const reason = generationRejectionReason.trim();
        if (!reason) throw new Error('请写明视觉问题，AI 才能有针对性地重做。');
        await repository.rejectGenerationStep(currentDocument.documentId, plan.revision, review.step.stepId, review.candidate.attemptId, reason);
        setGenerationRejectionReason('');
        showToast('已退回这一小步，AI 将按视觉意见重做');
      } else if (action === 'rollback') {
        if (!review) throw new Error('没有可回滚的步骤。');
        await repository.rollbackGenerationStep(currentDocument.documentId, plan.revision, review.step.stepId);
        setSceneReloadToken((value) => value + 1);
        showToast('已回滚最近接受的 AI 步骤');
      } else if (action === 'pause') {
        await repository.pauseGeneration(currentDocument.documentId, plan.revision);
        showToast('AI 设计流程已暂停');
      } else {
        await repository.resumeGeneration(currentDocument.documentId, plan.revision);
        showToast('AI 设计流程已继续');
      }
      await refreshGenerationState();
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
      await refreshGenerationState().catch(() => undefined);
    } finally {
      setGenerationAction(undefined);
    }
  }

  function commitSceneCommand(command: SceneEditorCommand, reason?: string): Promise<SceneDocument> {
    const execute = async () => {
      const scene = sceneDocumentRef.current;
      if (!repository || !scene) throw new Error('当前设计还没有可编辑的 Scene。');
      try {
        const result = await repository.editScene(scene.documentId, {
          transactionId: `studio:${crypto.randomUUID()}`,
          expectedRevision: scene.revision,
          reason,
          command
        });
        applySceneDocument(result.document);
        void refreshSceneHistory(scene.documentId).catch(() => undefined);
        return result.document;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (/revision|版本|更新到/i.test(message)) {
          const latest = await repository.readScene(scene.documentId).catch(() => undefined);
          if (latest) {
            applySceneDocument(latest);
            await refreshSceneHistory(scene.documentId).catch(() => undefined);
          }
        }
        throw error;
      }
    };
    const queued = sceneCommandQueue.current.then(execute, execute);
    sceneCommandQueue.current = queued.then(() => undefined, () => undefined);
    return queued;
  }

  async function undoScene() {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene || !sceneHistory?.undoCount) return;
    try {
      const next = await repository.undoScene(scene.documentId, scene.revision);
      applySceneDocument(next);
      await refreshSceneHistory(scene.documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function redoScene() {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene || !sceneHistory?.redoCount) return;
    try {
      const next = await repository.redoScene(scene.documentId, scene.revision);
      applySceneDocument(next);
      await refreshSceneHistory(scene.documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function undo() {
    if (sceneEditingActive) {
      void undoScene();
      return;
    }
    const current = documentRef.current;
    const previous = past[past.length - 1];
    if (!current || !previous) return;
    setPast((items) => items.slice(0, -1));
    setFuture((items) => [structuredClone(current), ...items].slice(0, 60));
    setCurrent(historyDocument(previous, current));
    setDirty(true);
  }

  function redo() {
    if (sceneEditingActive) {
      void redoScene();
      return;
    }
    const current = documentRef.current;
    const next = future[0];
    if (!current || !next) return;
    setFuture((items) => items.slice(1));
    setPast((items) => [...items.slice(-59), structuredClone(current)]);
    setCurrent(historyDocument(next, current));
    setDirty(true);
  }

  function updateComponent(componentId: string, updater: (component: WebDesignComponent) => WebDesignComponent) {
    commitWithCanvasGrowth((current) => ({ ...current, components: current.components.map((component) => component.id === componentId ? updater(component) : component) }));
  }

  async function save(force = false, silent = false) {
    const current = documentRef.current;
    if (!repository || !current || saving || (!dirty && !force)) return;
    setSaving(true);
    try {
      const snapshot = structuredClone(current);
      const saved = await repository.save(snapshot, persistedRevision);
      if (editableDocumentPayload(saved) !== editableDocumentPayload(snapshot)) {
        throw new Error('保存返回的数据改变了当前设计，已停止应用该结果以保护画布布局。');
      }
      setCurrent(saved);
      setPersistedRevision(saved.revision);
      setDirty(false);
      setDocuments(await repository.list());
      if (!silent) showToast('已保存');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  async function refresh() {
    const current = documentRef.current;
    if (!repository || !current) return;
    if (dirty && !window.confirm('当前有未保存修改，确定刷新并丢弃吗？')) return;
    try {
      openDocument(await repository.read(current.documentId));
      setDocuments(await repository.list());
      showToast('已读取 AI 或其他编辑器的最新修改');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function createNew() {
    if (!activeProject) return;
    setNewDesignName('');
    setNewDesignOpen(true);
    setProjectLibraryOpen(false);
  }

  async function refreshCatalog() {
    if (!repository) return;
    setDocuments(await repository.list());
  }

  async function createDesignFromSheet() {
    if (!repository || !activeProject || !newDesignName.trim()) return;
    try {
      const created = await repository.createInProject(activeProject.projectId, newDesignName, true);
      setActiveProject(await repository.readProject(activeProject.projectId));
      await refreshCatalog();
      setNewDesignOpen(false);
      setNewDesignName('');
      openDocument(created);
      setScreen('editor');
      replaceStudioLocation(activeProject.projectId, created.documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function openProjectDocument(documentId: string) {
    if (!repository) return;
    if (dirty && !window.confirm('切换设计会丢弃未保存修改，确定继续吗？')) return;
    try {
      openDocument(await repository.read(documentId));
      setProjectLibraryOpen(false);
      setScreen('editor');
      replaceStudioLocation(activeProject?.projectId, documentId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function goToActiveProject() {
    if (dirty && !window.confirm('返回项目首页会丢弃未保存修改，确定继续吗？')) return;
    setDocument(undefined);
    setDirty(false);
    setScreen('project');
    setProjectLibraryOpen(false);
    replaceStudioLocation(activeProject?.projectId);
  }

  async function confirmDeleteProjectDocument() {
    const target = deleteDesignTarget;
    if (!repository || !activeProject) return;
    if (!target || deletingDesign) return;
    setDeletingDesign(true);
    try {
      await repository.remove(target.documentId);
      setActiveProject(await repository.readProject(activeProject.projectId));
      if (document?.documentId === target.documentId) setDocument(undefined);
      await refreshCatalog();
      setScreen('project');
      replaceStudioLocation(activeProject.projectId);
      setDeleteDesignTarget(undefined);
      showToast(`已删除“${target.title}”`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setDeletingDesign(false);
    }
  }


  return {
    showToast,
    updateScenePreviewHeight,
    chooseLibraryTab,
    activateWorkspaceArea,
    activateWorkspaceTool,
    setCurrent,
    openDocument,
    commit,
    commitWithCanvasGrowth,
    changeLive,
    changeLiveWithCanvasGrowth,
    historyDocument,
    applySceneDocument,
    refreshSceneHistory,
    refreshGenerationState,
    runGenerationReviewAction,
    commitSceneCommand,
    undoScene,
    redoScene,
    undo,
    redo,
    updateComponent,
    save,
    refresh,
    createNew,
    refreshCatalog,
    createDesignFromSheet,
    openProjectDocument,
    goToActiveProject,
    confirmDeleteProjectDocument
  };
}
