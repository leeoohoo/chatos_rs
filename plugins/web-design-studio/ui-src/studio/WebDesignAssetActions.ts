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
import { createWebDesignViewportActions } from './WebDesignViewportActions';
import { createWebDesignSelectionActions } from './WebDesignSelectionActions';

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions> &
  ReturnType<typeof import('./WebDesignViewportActions').createWebDesignViewportActions> &
  ReturnType<typeof import('./WebDesignSelectionActions').createWebDesignSelectionActions>;

export function createWebDesignAssetActions(context: Record<string, any>) {
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
    toggleLocked, activateWorkspaceArtboard, updateActiveWorkspaceViewport, addWorkspaceSurface, fitWorkspaceArtboard, fitActiveWorkspaceArtboard,
    fitSlotEditorContent, fitWorkspaceSelection, focusWorkspaceArtboard, focusWorkspaceArtboardByPageId, updateBreakpoint, selectViewportPreset,
    updateCustomViewportWidth, updateCustomViewportHeight, withGeneratedResponsiveLayouts, generateResponsiveLayouts, fitCanvasToWidth, fitCanvasWidth,
    setCanvasZoom, toggleInteractionMode, nodeIds, wrapperId, deltaX, nodeId,
    patches, selectComponent, selectableNodesForCurrentEditor, selectionOverlayItemsFor, selectSelectionChild, selectSelectionParent,
    sceneSelectionRootIds, cloneSceneSubtree, insertSceneCopies, copySceneSelection, duplicateSceneSelection, pasteSceneClipboard,
    wrapSceneSelection, ungroupSceneSelection, nudgeSceneSelection, alignSceneSelection, distributeSceneSelection, reorderSceneSelection,
    deleteSceneSelection, updateSceneNodeById, updateSceneNode, applySelectedSceneLibraryVariant, focusSceneContent, updateSelectedSceneResponsiveOverride,
    replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride, switchPage, addPage, duplicateScenePage, duplicatePage,
    deleteCurrentPage, updateCurrentPage, useAsset, importAssets, downloadTextFile, exportCurrentPage,
    exportReact, exportVue, activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation, prepareSceneAnnotation,
    addSceneAnnotation, changeSceneAnnotationStatus, submitSceneAiInstruction, addAiRequest, renderGenerationReviewPanel, renderWorkspaceArtboard,
    renderPreviewSurfaceOverlay
  } = context as WebDesignActionContext & Record<string, any>;

  function groupSelected() {
    if (sceneEditingActive) {
      void wrapSceneSelection('group');
      return;
    }
    const current = documentRef.current;
    if (!current || selectedIds.length < 2) return;
    const roots = selectedRootIds(current, selectedIds);
    if (roots.length < 2) return;
    const components = roots.map((id) => current.components.find((component) => component.id === id)).filter(Boolean) as WebDesignComponent[];
    const padding = 20;
    const boundsFor = (target: WebDesignDevice) => {
      const frames = components.map((component) => resolveComponent(component, target));
      const x = Math.min(...frames.map((frame) => frame.x)) - padding;
      const y = Math.min(...frames.map((frame) => frame.y)) - padding;
      const right = Math.max(...frames.map((frame) => frame.x + frame.width)) + padding;
      const bottom = Math.max(...frames.map((frame) => frame.y + frame.height)) + padding;
      return { x, y, width: right - x, height: bottom - y };
    };
    const desktopBounds = boundsFor('desktop');
    const tabletBounds = boundsFor('tablet');
    const mobileBounds = boundsFor('mobile');
    const parentIds = new Set(components.map((component) => component.parentId));
    const slotIds = new Set(components.map((component) => component.slot));
    const group = componentDefaults('section', desktopBounds.x, desktopBounds.y);
    group.id = `group-${crypto.randomUUID().slice(0, 8)}`;
    group.name = `新建分组 · ${roots.length} 项`;
    group.pageId = pageId;
    group.width = desktopBounds.width;
    group.height = desktopBounds.height;
    group.parentId = parentIds.size === 1 ? components[0].parentId : undefined;
    group.slot = slotIds.size === 1 ? components[0].slot : undefined;
    group.zIndex = Math.max(0, Math.min(...components.map((component) => component.zIndex)) - 1);
    group.layout = { mode: 'free', gap: 16, padding, align: 'start' };
    group.responsive = { tablet: tabletBounds, mobile: mobileBounds };
    commitWithCanvasGrowth((active) => ({
      ...active,
      components: [...active.components.map((component) => roots.includes(component.id) ? { ...component, parentId: group.id } : component), group]
    }));
    setSelectedId(group.id);
    setSelectedIds([group.id]);
    showToast('已创建分组，拖动分组外框即可整体移动');
  }

  function ungroupSelected() {
    if (sceneEditingActive) {
      void ungroupSceneSelection();
      return;
    }
    const current = documentRef.current;
    if (!current || !selected) return;
    const children = current.components.filter((component) => component.parentId === selected.id);
    if (children.length === 0) return;
    const childIds = children.map((component) => component.id);
    commit((active) => ({
      ...active,
      components: active.components
        .filter((component) => component.id !== selected.id)
        .map((component) => component.parentId === selected.id ? { ...component, parentId: selected.parentId, slot: selected.slot } : component),
      requests: active.requests.filter((request) => request.componentId !== selected.id)
    }));
    setSelectedId(childIds[0]);
    setSelectedIds(childIds);
    showToast('已取消分组，内部组件保持在原位置');
  }

  function updateSelectedLayout(changes: Partial<NonNullable<WebDesignComponent['layout']>>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      layout: { mode: 'free', gap: 16, padding: 16, align: 'start', justify: 'start', wrap: false, ...component.layout, ...changes }
    }, 'frame', true));
  }

  function applySelectedAutoLayout() {
    if (!selected) return;
    commitWithCanvasGrowth((current) => {
      const changedIds = new Set(current.components.filter((component) => component.parentId === selected.id)
        .flatMap((component) => [component.id, ...descendantIds(current, component.id)]));
      const laidOut = autoLayoutContainer(current, selected.id, device);
      return { ...laidOut, components: laidOut.components.map((component) => changedIds.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
    });
  }

  function saveSelectionAsSymbol() {
    const current = documentRef.current;
    if (!current || selectedIds.length === 0) return;
    const defaultName = selectedIds.length > 1 ? `${selected?.name ?? '组合'} · ${selectedIds.length} 层` : selected?.name ?? '我的组件';
    const name = window.prompt('给这个组合起个名字', defaultName)?.trim();
    if (!name) return;
    const symbol = createSymbolFromSelection(current, selectedIds, name);
    commit((active) => ({ ...active, symbols: [...(active.symbols ?? []), symbol] }));
    setPersonalSymbols((symbols) => [...symbols.filter((candidate) => candidate.id !== symbol.id), structuredClone(symbol)]);
    chooseLibraryTab('my');
    showToast(`已保存到“我的”：${symbol.name}`);
  }

  function insertSymbol(symbol: WebDesignSymbol) {
    const current = documentRef.current;
    if (!current) return;
    const instance = instantiateSymbol(current, symbol, pageId);
    commitWithCanvasGrowth(
      (active) => ({
        ...active,
        symbols: (active.symbols ?? []).some((candidate) => candidate.id === symbol.id)
          ? (active.symbols ?? []).map((candidate) => candidate.id === symbol.id ? structuredClone(symbol) : candidate)
          : [...(active.symbols ?? []), structuredClone(symbol)],
        components: [...active.components, ...instance.components]
      }),
      ['desktop', 'tablet', 'mobile']
    );
    setSelectedId(instance.rootIds[0]);
    setSelectedIds(instance.rootIds);
    const instanceTop = Math.min(...instance.components.map((component) => resolveComponent(component, device).y));
    if (instanceTop > breakpointFor(current, device).height * .65) {
      window.setTimeout(() => {
        const scroller = window.document.querySelector('.canvas-scroll');
        scroller?.scrollTo({ top: scroller.scrollHeight, behavior: 'smooth' });
      }, 0);
    }
    showToast(`已插入 ${symbol.name}`);
  }

  function renamePersonalSymbol(symbol: WebDesignSymbol) {
    const name = window.prompt('重命名我的组件', symbol.name)?.trim();
    if (!name || name === symbol.name) return;
    setPersonalSymbols((symbols) => symbols.map((candidate) => candidate.id === symbol.id ? { ...candidate, name } : candidate));
    commit((current) => ({ ...current, symbols: current.symbols?.map((candidate) => candidate.id === symbol.id ? { ...candidate, name } : candidate) }));
  }

  function removePersonalSymbol(symbolId: string) {
    if (!window.confirm('从“我的”中移除这个组件？已放入画布的内容不会受影响。')) return;
    setPersonalSymbols((symbols) => symbols.filter((symbol) => symbol.id !== symbolId));
    showToast('已从“我的”移除，画布中的实例保持不变');
  }

  function saveSceneSelectionAsSnippet() {
    const scene = sceneDocumentRef.current;
    if (!scene || selectedIds.length === 0) return;
    const defaultName = selectedIds.length === 1
      ? indexSceneDocument(scene).get(selectedIds[0])?.node.name ?? '我的组件'
      : `设计组合 · ${selectedIds.length} 层`;
    const name = window.prompt('给这个可复用设计组合起个名字', defaultName)?.trim();
    if (!name) return;
    try {
      const snippet = createSceneSnippet(scene, selectedIds, name);
      setSceneSnippets((items) => [...items.filter((item) => item.id !== snippet.id), snippet]);
      chooseLibraryTab('my');
      showToast(`已保存到“我的”：${snippet.name}`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function insertSceneSnippet(snippet: SceneSnippet) {
    const scene = sceneDocumentRef.current;
    if (!scene) return;
    try {
      const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId)
        ?? workspacePlacement?.artboards.find((candidate) => candidate.pageId === pageId);
      const targetPageId = artboard?.pageId ?? pageId;
      const viewportWidth = artboard?.viewportWidth ?? breakpoint.width;
      const target = resolveSceneInsertionTarget({
        document: scene,
        pageId: targetPageId,
        viewportWidth,
        point: { x: 72, y: 72 },
        preferred: sceneContentFocus?.pageId === targetPageId ? sceneContentFocus : undefined
      });
      const node = instantiateSceneSnippet(snippet, target.x, target.y);
      await commitSceneCommand({ type: 'insert-node', parentId: target.nodeId, slot: target.slot, index: target.index, node }, `用户插入“我的”设计组合 ${snippet.name}。`);
      setSelectedId(node.id);
      setSelectedIds([node.id]);
      showToast(`已插入 ${snippet.name}`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function renameSceneSnippet(snippet: SceneSnippet) {
    const name = window.prompt('重命名设计组合', snippet.name)?.trim();
    if (!name || name === snippet.name) return;
    setSceneSnippets((items) => items.map((item) => item.id === snippet.id ? { ...item, name, updatedAt: new Date().toISOString() } : item));
  }

  function removeSceneSnippet(snippetId: string) {
    if (!window.confirm('从“我的”中移除这个设计组合？已经插入画布的内容不会受影响。')) return;
    setSceneSnippets((items) => items.filter((item) => item.id !== snippetId));
    showToast('已从“我的”移除，画布中的内容保持不变');
  }

  async function applySceneVariablesDraft() {
    if (!sceneDocumentRef.current) return;
    try {
      const collections = JSON.parse(sceneVariablesDraft) as SceneVariableCollection[];
      if (!Array.isArray(collections)) throw new Error('变量数据必须是数组。');
      await commitSceneCommand({ type: 'set-variable-collections', collections }, '用户更新 Scene 变量与模式。');
      showToast('Scene 变量已保存');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function seedSceneVariables() {
    const modeId = 'mode:default';
    setSceneVariablesDraft(JSON.stringify([{
      id: 'variables:visual-system',
      name: '视觉系统',
      modes: [{ id: modeId, name: '默认' }],
      variables: [
        { id: 'variable:color-primary', name: '主色', type: 'color', valuesByMode: { [modeId]: '#0A84FF' } },
        { id: 'variable:color-surface', name: '表面', type: 'color', valuesByMode: { [modeId]: '#FFFFFF' } },
        { id: 'variable:color-text', name: '正文', type: 'color', valuesByMode: { [modeId]: '#1D1D1F' } },
        { id: 'variable:spacing-base', name: '基础间距', type: 'number', valuesByMode: { [modeId]: 8 } },
        { id: 'variable:radius-card', name: '卡片圆角', type: 'number', valuesByMode: { [modeId]: 20 } },
        { id: 'variable:font-family', name: '字体', type: 'string', valuesByMode: { [modeId]: '-apple-system, BlinkMacSystemFont, sans-serif' } }
      ]
    } satisfies SceneVariableCollection], null, 2));
  }

  function toggleSelectedSymbolOverride(override: WebSymbolOverride) {
    if (!selected) return;
    const enabled = !(selected.symbolOverrides ?? []).includes(override);
    updateComponent(selected.id, (component) => setSymbolOverride(component, override, enabled));
  }

  function synchronizeSelectedSymbol() {
    if (!selected?.symbolId) return;
    commit((current) => syncSymbolInstances(current, selected.symbolId!));
    showToast('已同步全部组件实例');
  }

  function updateSelectedSymbolDefinition() {
    if (!selected?.symbolInstanceId) return;
    commit((current) => updateSymbolFromInstance(current, selected.id));
    showToast('已更新组件定义并同步其他实例');
  }

  function updateSelectedLibraryProp(key: string, value: WebDesignJsonValue) {
    if (!selected?.library) return;
    updateSelected({ library: { ...selected.library, props: { ...selected.library.props, [key]: value } } });
  }

  function applySelectedLibraryVariant(variantId: string) {
    if (!selected?.library) return;
    if (selected.library.props.editorDetachedContent !== true) {
      updateComponent(selected.id, (component) => applyUiLibraryVariant(component, variantId));
      return;
    }
    if (!window.confirm('切换官方款式会替换当前已经拆分的内部设计，是否继续？')) return;
    commit((current) => {
      const slotRoots = current.components.filter((component) => component.parentId === selected.id && component.slot);
      const removed = new Set(slotRoots.flatMap((component) => [component.id, ...descendantIds(current, component.id)]));
      return {
        ...current,
        components: current.components
          .filter((component) => !removed.has(component.id))
          .map((component) => {
            if (component.id !== selected.id || !component.library) return component;
            const props = { ...component.library.props };
            delete props.editorDetachedContent;
            return applyUiLibraryVariant({ ...component, library: { ...component.library, props } }, variantId);
          })
      };
    });
    setSelectedId(selected.id);
    setSelectedIds([selected.id]);
    showToast('已切换官方款式；再次进入“内部内容”即可拆分编辑');
  }

  function detachSelectedSymbol() {
    if (!selected?.symbolInstanceId) return;
    commit((current) => detachSymbolInstance(current, selected.id));
    showToast('当前实例已脱离组件库');
  }

  function updateTokens(updater: (tokens: WebDesignTokens) => WebDesignTokens) {
    commit((current) => ({ ...current, tokens: updater(structuredClone(tokensForDocument(current))) }));
  }

  async function applyDesignTheme(preset: WebDesignThemePreset) {
    const scene = sceneDocumentRef.current;
    if (!scene) {
      showToast('请先让 AI 建立 Scene，再应用视觉变量');
      return;
    }
    const modeId = 'theme:default';
    const collection: SceneVariableCollection = {
      id: 'theme:visual-system',
      name: `${preset.name} 视觉系统`,
      modes: [{ id: modeId, name: 'Default' }],
      variables: [
        ...Object.entries({ canvas: preset.canvasBackground, ...preset.tokens.colors }).map(([key, value]) => ({ id: `theme:color:${key}`, name: `Color / ${key}`, type: 'color' as const, valuesByMode: { [modeId]: value } })),
        ...Object.entries(preset.tokens.radii).map(([key, value]) => ({ id: `theme:radius:${key}`, name: `Radius / ${key}`, type: 'number' as const, valuesByMode: { [modeId]: value } })),
        { id: 'theme:font:family', name: 'Typography / font family', type: 'string', valuesByMode: { [modeId]: preset.tokens.typography.fontFamily } },
        { id: 'theme:font:base-size', name: 'Typography / base size', type: 'number', valuesByMode: { [modeId]: preset.tokens.typography.baseFontSize } }
      ]
    };
    try {
      await commitSceneCommand({
        type: 'set-variable-collections',
        collections: [...scene.variableCollections.filter((item) => item.id !== collection.id), collection]
      }, `用户从 ${preset.name} 建立 Scene 视觉变量。`);
      setThemePickerOpen(false);
      showToast(`已建立 ${preset.name} Scene 视觉变量`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function updateTokenColor(key: keyof WebDesignTokens['colors'], value: string) {
    updateTokens((current) => ({ ...current, colors: { ...current.colors, [key]: value } }));
  }

  function applyColorToken(property: 'background' | 'color', token: keyof WebDesignTokens['colors']) {
    updateSelectedStyle({ [property]: `var(--color-${token})` });
  }

  function applyRadiusToken(token: keyof WebDesignTokens['radii']) {
    updateSelectedStyle({ borderRadius: tokens?.radii[token] ?? 0 });
  }


  return {
    groupSelected,
    ungroupSelected,
    updateSelectedLayout,
    applySelectedAutoLayout,
    saveSelectionAsSymbol,
    insertSymbol,
    renamePersonalSymbol,
    removePersonalSymbol,
    saveSceneSelectionAsSnippet,
    insertSceneSnippet,
    renameSceneSnippet,
    removeSceneSnippet,
    applySceneVariablesDraft,
    seedSceneVariables,
    toggleSelectedSymbolOverride,
    synchronizeSelectedSymbol,
    updateSelectedSymbolDefinition,
    updateSelectedLibraryProp,
    applySelectedLibraryVariant,
    detachSelectedSymbol,
    updateTokens,
    applyDesignTheme,
    updateTokenColor,
    applyColorToken,
    applyRadiusToken
  };
}
