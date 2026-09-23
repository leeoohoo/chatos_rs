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
import { materializeOfficialDemoContent } from './WebDesignStudioSupport';
import { createWebDesignCoreActions } from './WebDesignCoreActions';
import { createWebDesignInsertActions } from './WebDesignInsertActions';

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions>;

export function createWebDesignCanvasActions(context: Record<string, any>) {
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
    beginUiLibraryPreviewPointerDrag, moveUiLibraryPreviewPointerDragAt, finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent, dropUiLibraryPreviewElement, activateWorkspaceArtboard,
    updateActiveWorkspaceViewport, addWorkspaceSurface, fitWorkspaceArtboard, fitActiveWorkspaceArtboard, fitSlotEditorContent, fitWorkspaceSelection,
    focusWorkspaceArtboard, focusWorkspaceArtboardByPageId, updateBreakpoint, selectViewportPreset, updateCustomViewportWidth, updateCustomViewportHeight,
    withGeneratedResponsiveLayouts, generateResponsiveLayouts, fitCanvasToWidth, fitCanvasWidth, setCanvasZoom, toggleInteractionMode,
    nodeIds, wrapperId, deltaX, nodeId, patches, selectComponent,
    selectableNodesForCurrentEditor, selectionOverlayItemsFor, selectSelectionChild, selectSelectionParent, sceneSelectionRootIds, cloneSceneSubtree,
    insertSceneCopies, copySceneSelection, duplicateSceneSelection, pasteSceneClipboard, wrapSceneSelection, ungroupSceneSelection,
    nudgeSceneSelection, alignSceneSelection, distributeSceneSelection, reorderSceneSelection, deleteSceneSelection, updateSceneNodeById,
    updateSceneNode, applySelectedSceneLibraryVariant, focusSceneContent, updateSelectedSceneResponsiveOverride, replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride,
    groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout, saveSelectionAsSymbol, insertSymbol,
    renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet, renameSceneSnippet, removeSceneSnippet,
    applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol, updateSelectedSymbolDefinition, updateSelectedLibraryProp,
    applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme, updateTokenColor, applyColorToken,
    applyRadiusToken, switchPage, addPage, duplicateScenePage, duplicatePage, deleteCurrentPage,
    updateCurrentPage, useAsset, importAssets, downloadTextFile, exportCurrentPage, exportReact,
    exportVue, activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation, prepareSceneAnnotation, addSceneAnnotation,
    changeSceneAnnotationStatus, submitSceneAiInstruction, addAiRequest, renderGenerationReviewPanel, renderWorkspaceArtboard, renderPreviewSurfaceOverlay
  } = context as WebDesignActionContext & Record<string, any>;

  function enterSlotEditor(next: EditingSlot) {
    if (!editingSlot) workspaceCameraBeforeSlot.current = { ...workspaceCamera };
    setEditingSlot(next);
  }

  function resetSlotEditorCamera() {
    workspaceCameraBeforeSlot.current = undefined;
    slotCameraContext.current = undefined;
  }

  async function editComponentSlot(component: WebDesignComponent, slotId: string, options: { compoundOnly?: boolean } = {}): Promise<boolean> {
    if (interactionMode) setInteractionMode(false);
    const current = documentRef.current;
    const slot = editableSlotsForUiComponent(component).find((candidate) => candidate.id === slotId);
    let first = current ? componentsInSlot(current, component.id, slotId)[0] : undefined;
    if (current && slot && !first) {
      const officialDemo = await materializeOfficialDemoContent(component, slot, pageId, device);
      const officialRoots = officialDemo.filter((candidate) => candidate.parentId === component.id);
      if (options.compoundOnly && officialRoots.length < 2) return false;
      const materialized = officialDemo.length > 0 ? officialDemo : materializeExistingSlotContent(component, slot, pageId, device);
      if (options.compoundOnly && officialDemo.length === 0) return false;
      if (materialized.length > 0) {
        commitWithCanvasGrowth((active) => ({
          ...active,
          components: [
            ...active.components.map((candidate) => candidate.id === component.id ? {
              ...candidate,
              content: '',
              library: officialDemo.length > 0 && candidate.library ? {
                ...candidate.library,
                props: { ...candidate.library.props, editorDetachedContent: true }
              } : candidate.library
            } : candidate),
            ...materialized
          ]
        }));
        first = (officialDemo.length > 0
          ? officialDemo.filter((candidate) => candidate.parentId === component.id)
            .sort((left, right) => resolveComponent(left, device).y - resolveComponent(right, device).y)[0]
          : undefined) ?? materialized[0];
      }
    }
    enterSlotEditor({ componentId: component.id, slotId });
    setSelectedId(first?.id);
    setSelectedIds(first ? [first.id] : []);
    return true;
  }

  function exitSlotEditor() {
    const containerId = editingSlot?.componentId;
    const previousCamera = workspaceCameraBeforeSlot.current;
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    if (previousCamera) setWorkspaceCamera(previousCamera);
    setSelectedId(containerId);
    setSelectedIds(containerId ? [containerId] : []);
  }

  function insertSlotTemplate(template: 'form' | 'details') {
    const current = documentRef.current;
    if (!current || !editingContainer || !editingSlotDefinition) return;
    const existing = componentsInSlot(current, editingContainer.id, editingSlotDefinition.id);
    if (existing.length > 0 && !window.confirm('当前内容区域已有组件，继续会在现有内容下方添加模板，是否继续？')) return;
    const starter = createSlotStarterComponents(editingContainer, editingSlotDefinition, template, pageId, device);
    const offsetY = existing.length === 0 ? 0 : Math.max(...existing.map((component) => resolveComponent(component, device).y - resolveComponent(editingContainer, device).y + resolveComponent(component, device).height)) + 24;
    const adjusted = offsetY === 0 ? starter : starter.map((component) => {
      const frame = resolveComponent(component, device);
      return updateComponentFrame(component, device, { y: frame.y + offsetY });
    });
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts({
      ...active,
      components: [...active.components, ...adjusted]
    }, pageId), ['desktop', 'tablet', 'mobile']);
    setSelectedId(adjusted[0]?.id);
    setSelectedIds(adjusted[0] ? [adjusted[0].id] : []);
    showToast(template === 'form' ? '已插入可编辑表单' : '已插入可编辑详情内容');
  }

  function onCanvasDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    const current = documentRef.current;
    if (!current || interactionMode) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = Math.round((event.clientX - bounds.left) / zoom);
    const y = Math.round((event.clientY - bounds.top) / zoom);
    const libraryPayload = event.dataTransfer.getData('application/x-web-design-library');
    if (libraryPayload) {
      try {
        const parsed = JSON.parse(libraryPayload) as VariantPickerTarget & { definitionId?: string; variantId?: string; registryElement?: LibraryPreviewSelection };
        const definitionId = parsed.definitionId ?? parsed.componentId;
        if (uiLibraryByName(parsed.library)?.components.some((item) => item.id === definitionId)) {
          addUiLibraryComponent(parsed.library, definitionId, x, y, parsed.variantId, editingSlot, parsed.registryElement);
          setVariantPickerDrag(undefined);
          if (parsed.registryElement) setVariantPickerTarget(undefined);
          return;
        }
      } catch { /* Ignore malformed drag payloads. */ }
    }
    const shapeId = event.dataTransfer.getData('application/x-web-design-shape') as BasicShapeId;
    if (!palette.some((item) => item.id === shapeId)) return;
    let component = basicShapeDefaults(shapeId, x, y);
    component.pageId = pageId;
    if (editingSlot && editingContainer) {
      const parentFrame = resolveComponent(editingContainer, device);
      component.x = editingContainer.x + x;
      component.y = editingContainer.y + y;
      component.parentId = editingContainer.id;
      component.slot = editingSlot.slotId;
      component.zIndex = Math.max(0, ...current.components.filter((item) => item.parentId === editingContainer.id && item.slot === editingSlot.slotId).map((item) => item.zIndex)) + 1;
      if (device !== 'desktop') component = updateComponentFrame(component, device, { x: parentFrame.x + x, y: parentFrame.y + y });
    } else {
      if (device !== 'desktop') component = updateComponentFrame(component, device, { x, y });
      component.zIndex = Math.max(1, ...componentsForPage(current, pageId).filter((item) => !contentContainerAncestor(current, item)).map((item) => item.zIndex)) + 1;
    }
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts({ ...active, components: [...active.components, component] }, pageId), ['desktop', 'tablet', 'mobile']);
    setSelectedId(component.id);
    setSelectedIds([component.id]);
  }

  function beginInteraction(event: ReactPointerEvent, component: WebDesignComponent, kind: Interaction['kind']) {
    if (interactionMode) return;
    if (spacePressed.current || event.button === 1) return;
    if (workspaceShell.activeTool === 'hand') return;
    event.preventDefault();
    event.stopPropagation();
    if ((event.metaKey || event.ctrlKey) && kind === 'move') {
      const current = documentRef.current;
      const canvas = (event.currentTarget as HTMLElement).closest<HTMLElement>('.design-canvas');
      if (!current || !canvas) return;
      const bounds = canvas.getBoundingClientRect();
      const point = {
        x: (event.clientX - bounds.left) / Math.max(bounds.width / Math.max(1, canvas.offsetWidth), .0001),
        y: (event.clientY - bounds.top) / Math.max(bounds.height / Math.max(1, canvas.offsetHeight), .0001)
      };
      const candidates = selectionCandidatesAtPoint(selectableNodesForCurrentEditor(current), point);
      if (candidates.length <= 1) {
        setSelectionCandidatePopover(undefined);
        selectComponent(candidates[0]?.id ?? component.id);
      } else {
        setSelectionCandidatePopover({ clientX: event.clientX, clientY: event.clientY, candidates });
      }
      return;
    }
    setSelectionCandidatePopover(undefined);
    if (workspaceShell.activeTool === 'comment') {
      setSelectedId(component.id);
      setSelectedIds([component.id]);
      setInspectorTab('ai');
      if (!workspaceShell.rightPanelOpen) dispatchWorkspaceShell({ type: 'toggle-right-panel' });
      showToast(`已选择“${component.name}”，请在右侧添加批注`);
      return;
    }
    if (event.shiftKey) {
      const next = selectedIds.includes(component.id) ? selectedIds.filter((id) => id !== component.id) : [...selectedIds, component.id];
      setSelectedIds(next);
      setSelectedId(next.includes(component.id) ? component.id : next[0]);
      return;
    }
    const nextSelectedIds = selectedIds.includes(component.id) ? selectedIds : [component.id];
    setSelectedId(component.id);
    setSelectedIds(nextSelectedIds);
    if (component.locked) {
      showToast('组件已锁定');
      return;
    }
    const current = documentRef.current;
    if (!current) return;
    interaction.current = {
      kind,
      componentId: component.id,
      pointerX: event.clientX,
      pointerY: event.clientY,
      frame: resolveComponent(component, device),
      selectedIds: nextSelectedIds,
      snapshot: structuredClone(current),
      scale: zoom,
      scoped: Boolean(editingSlot)
    };
  }

  function beginCanvasPan(event: ReactPointerEvent<HTMLDivElement>) {
    const handTool = event.button === 0 && workspaceShell.activeTool === 'hand';
    if (event.button !== 1 && !(event.button === 0 && spacePressed.current) && !handTool) return;
    event.preventDefault();
    canvasPan.current = {
      pointerX: event.clientX,
      pointerY: event.clientY,
      camera: workspaceCamera
    };
    setCanvasPanning(true);
  }

  function beginCanvasMarquee(event: ReactPointerEvent<HTMLElement>) {
    if (interactionMode || event.button !== 0 || spacePressed.current) return;
    if (workspaceShell.activeTool === 'hand' || workspaceShell.activeTool === 'comment') return;
    const current = documentRef.current;
    if (!current) return;
    event.preventDefault();
    setSelectionCandidatePopover(undefined);
    const canvas = event.currentTarget;
    const bounds = canvas.getBoundingClientRect();
    const scaleX = bounds.width / Math.max(1, canvas.offsetWidth);
    const scaleY = bounds.height / Math.max(1, canvas.offsetHeight);
    canvasMarquee.current = {
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startPoint: {
        x: (event.clientX - bounds.left) / Math.max(scaleX, .0001),
        y: (event.clientY - bounds.top) / Math.max(scaleY, .0001)
      },
      canvas,
      nodes: selectableNodesForCurrentEditor(current),
      initialIds: event.shiftKey ? [...selectedIds] : [],
      initialPrimaryId: event.shiftKey ? selectedId : undefined,
      additive: event.shiftKey,
      moved: false
    };
    setMarqueeRect(undefined);
  }

  function updateSelected(changes: Partial<WebDesignComponent>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({ ...component, ...changes }, 'content', true));
  }

  function updateSelectedFrame(changes: Partial<Pick<ResolvedWebDesignComponent, 'x' | 'y' | 'width' | 'height' | 'hidden'>>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => {
      const constrained = changes.width !== undefined || changes.height !== undefined ? constrainComponentFrame(component, device, changes) : undefined;
      return setSymbolOverride(updateComponentFrame(component, device, { ...changes, ...constrained }), 'frame', true);
    });
  }

  function updateInspectedFrame(changes: Partial<Pick<ResolvedWebDesignComponent, 'x' | 'y' | 'width' | 'height' | 'hidden'>>) {
    if (!editingContainer || !editingSlot || !selected || !documentRef.current || slotIdForDescendant(documentRef.current, selected, editingContainer.id) !== editingSlot.slotId) {
      updateSelectedFrame(changes);
      return;
    }
    const containerFrame = resolveComponent(editingContainer, device);
    const translated = { ...changes };
    if (changes.x !== undefined) translated.x = containerFrame.x + changes.x;
    if (changes.y !== undefined) translated.y = containerFrame.y + changes.y;
    updateSelectedFrame(translated);
  }

  function updateSelectedStyle(changes: Partial<WebComponentStyle>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => inspectorVisualState === 'default'
      ? setSymbolOverride(updateComponentStyle(component, device, changes), 'style', true)
      : setSymbolOverride({
        ...component,
        states: { ...component.states, [inspectorVisualState]: { ...component.states?.[inspectorVisualState], ...changes } }
      }, 'style', true));
  }

  function clearSelectedVisualState() {
    if (!selected || inspectorVisualState === 'default') return;
    updateComponent(selected.id, (component) => {
      const states = { ...component.states };
      delete states[inspectorVisualState];
      return setSymbolOverride({ ...component, states: Object.keys(states).length > 0 ? states : undefined }, 'style', true);
    });
  }

  function updateSelectedCustomCss(customCss: Record<string, string | number>) {
    updateSelectedStyle({ customCss: Object.keys(customCss).length > 0 ? customCss : undefined });
  }

  function updateSelectedHorizontalConstraint(horizontal: WebHorizontalConstraint) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      constraints: { ...component.constraints, [device]: { ...component.constraints?.[device], horizontal } }
    }, 'frame', true));
  }

  function updateSelectedSizeConstraints(changes: Partial<WebComponentConstraints>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      constraints: { ...component.constraints, [device]: { horizontal: component.constraints?.[device]?.horizontal ?? 'auto', ...component.constraints?.[device], ...changes } }
    }, 'frame', true));
  }

  function deleteSelected() {
    const current = documentRef.current;
    if (selectedIds.length === 0 || !current) return;
    const removed = new Set(selectedIds.flatMap((id) => [id, ...descendantIds(current, id)]));
    commit((active) => ({
      ...active,
      components: active.components
        .filter((component) => !removed.has(component.id))
        .map((component) => component.parentId && removed.has(component.parentId) ? { ...component, parentId: undefined } : component),
      requests: active.requests.filter((request) => !request.componentId || !removed.has(request.componentId))
    }));
    setSelectedId(undefined);
    if (editingSlot && removed.has(editingSlot.componentId)) {
      resetSlotEditorCamera();
      setEditingSlot(undefined);
    }
    setSelectedIds([]);
  }

  function duplicateSelected() {
    if (sceneEditingActive) {
      duplicateSceneSelection();
      return;
    }
    const current = documentRef.current;
    if (selectedIds.length === 0 || !current) return;
    const cloned = cloneComponentSubtrees(current, selectedIds, pageId, 20, current);
    commitWithCanvasGrowth((active) => ({ ...active, components: [...active.components, ...cloned.components] }));
    setSelectedId(cloned.rootIds[0]);
    setSelectedIds(cloned.rootIds);
  }

  function copySelected() {
    if (sceneEditingActive) {
      copySceneSelection();
      return;
    }
    const current = documentRef.current;
    if (!current || selectedIds.length === 0) return;
    setClipboard({ document: structuredClone(current), componentIds: [...selectedIds] });
    showToast(`已复制 ${selectedIds.length} 个组件`);
  }

  function pasteClipboard() {
    if (sceneEditingActive) {
      pasteSceneClipboard();
      return;
    }
    const current = documentRef.current;
    if (!current || !clipboard) return;
    const cloned = cloneComponentSubtrees(clipboard.document, clipboard.componentIds, pageId, 20, current);
    commitWithCanvasGrowth((active) => ({ ...active, components: [...active.components, ...cloned.components] }));
    setSelectedId(cloned.rootIds[0]);
    setSelectedIds(cloned.rootIds);
    showToast('已粘贴到当前页面');
  }

  function reorderSelected(action: LayerAction) {
    if (!selectedId) return;
    commit((current) => {
      const ordered = [...componentsForPage(current, pageId)].sort((left, right) => left.zIndex - right.zIndex);
      const index = ordered.findIndex((component) => component.id === selectedId);
      if (index < 0) return current;
      const [item] = ordered.splice(index, 1);
      const targetIndex = action === 'front' ? ordered.length : action === 'back' ? 0 : action === 'forward' ? Math.min(ordered.length, index + 1) : Math.max(0, index - 1);
      ordered.splice(targetIndex, 0, item);
      const zIndexes = new Map(ordered.map((component, zIndex) => [component.id, zIndex + 1]));
      return { ...current, components: current.components.map((component) => zIndexes.has(component.id) ? { ...component, zIndex: zIndexes.get(component.id)! } : component) };
    });
  }

  function alignSelected(action: AlignAction) {
    if (!inspectedFrame) return;
    const targetWidth = editingSlotCanvasSize?.width ?? breakpoint.width;
    const targetHeight = editingSlotCanvasSize?.height ?? breakpoint.height;
    const changes: Partial<ResolvedWebDesignComponent> = {};
    if (action === 'left') changes.x = 0;
    if (action === 'center') changes.x = Math.round((targetWidth - inspectedFrame.width) / 2);
    if (action === 'right') changes.x = targetWidth - inspectedFrame.width;
    if (action === 'top') changes.y = 0;
    if (action === 'middle') changes.y = Math.round((targetHeight - inspectedFrame.height) / 2);
    if (action === 'bottom') changes.y = targetHeight - inspectedFrame.height;
    updateInspectedFrame(changes);
  }

  function nudgeSelected(dx: number, dy: number) {
    if (selectedIds.length === 0 || selectedIds.some((id) => documentRef.current?.components.find((component) => component.id === id)?.locked)) return;
    commitWithCanvasGrowth((current) => {
      const moving = new Set(selectedRootIds(current, selectedIds).flatMap((id) => [id, ...descendantIds(current, id)]));
      const moved = moveComponentsWithDescendants(current, selectedIds, device, dx, dy);
      return { ...moved, components: moved.components.map((component) => moving.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
    });
  }

  function toggleHidden(component: WebDesignComponent) {
    const resolved = resolveComponent(component, device);
    updateComponent(component.id, (current) => updateComponentFrame(current, device, { hidden: !resolved.hidden }));
  }

  function toggleLocked(component: WebDesignComponent) {
    updateComponent(component.id, (current) => ({ ...current, locked: !current.locked }));
  }


  return {
    enterSlotEditor,
    resetSlotEditorCamera,
    editComponentSlot,
    exitSlotEditor,
    insertSlotTemplate,
    onCanvasDrop,
    beginInteraction,
    beginCanvasPan,
    beginCanvasMarquee,
    updateSelected,
    updateSelectedFrame,
    updateInspectedFrame,
    updateSelectedStyle,
    clearSelectedVisualState,
    updateSelectedCustomCss,
    updateSelectedHorizontalConstraint,
    updateSelectedSizeConstraints,
    deleteSelected,
    duplicateSelected,
    copySelected,
    pasteClipboard,
    reorderSelected,
    alignSelected,
    nudgeSelected,
    toggleHidden,
    toggleLocked
  };
}
