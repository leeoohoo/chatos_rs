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
import { createWebDesignAssetActions } from './WebDesignAssetActions';
import { createWebDesignDocumentActions } from './WebDesignDocumentActions';
import {
  NumberField,
  SceneNumberField,
  ColorValueField,
  AdvancedCssEditor,
  JsonPropertyEditor,
  JsonObjectEditor,
  runtimeSlotContentMap,
  RuntimeSlotContent,
  RuntimeSlotCanvasComponent,
  formatProjectDate
} from './WebDesignInspectorFields';

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions> &
  ReturnType<typeof import('./WebDesignViewportActions').createWebDesignViewportActions> &
  ReturnType<typeof import('./WebDesignSelectionActions').createWebDesignSelectionActions> &
  ReturnType<typeof import('./WebDesignAssetActions').createWebDesignAssetActions> &
  ReturnType<typeof import('./WebDesignDocumentActions').createWebDesignDocumentActions>;

export function createWebDesignRenderHelpers(context: Record<string, any>) {
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
    replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride, groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout,
    saveSelectionAsSymbol, insertSymbol, renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet,
    renameSceneSnippet, removeSceneSnippet, applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol,
    updateSelectedSymbolDefinition, updateSelectedLibraryProp, applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme,
    updateTokenColor, applyColorToken, applyRadiusToken, switchPage, addPage, duplicateScenePage,
    duplicatePage, deleteCurrentPage, updateCurrentPage, useAsset, importAssets, downloadTextFile,
    exportCurrentPage, exportReact, exportVue, activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation,
    prepareSceneAnnotation, addSceneAnnotation, changeSceneAnnotationStatus, submitSceneAiInstruction, addAiRequest, layerComponents,
    sceneLayerNodes, directChildCount, canUngroup, canUngroupScene, selectedSceneLibrary, selectedSceneLibraryDefinition,
    selectedSceneLibraryVariants, selectedSceneRegistryElement, selectedSceneEditableSlots, selectedSymbol, selectedLibrary, selectedLibraryDefinition,
    selectedLibraryVariants, selectedRegistryElement, selectedEditableSlots, inspectorCapabilities, selectedInspectableLibraryProps, aiTarget,
    normalizedPaletteQuery, filteredPalette, filteredPersonalSymbols, filteredSceneSnippets, activeUiLibrary, filteredUiLibraryComponents,
    variantPickerLibrary, variantPickerDefinition, variantPickerVariants, variantPickerPresentation, sceneAiTarget, sceneAnnotationTasks,
    aiQuickPrompts, storageBadge, newDesignModal, deleteDesignModal
  } = context as WebDesignActionContext & Record<string, any>;

  function renderGenerationReviewPanel() {
    if (generationLoading && !generationPlan) return <div className="generation-review-empty"><span className="loading-dot" /><strong>正在读取 AI 设计进度…</strong></div>;
    if (!generationPlan) return <div className="generation-review-empty">
      <div className="empty-icon">✦</div>
      <strong>AI 还没有开始分步设计</strong>
      <p>AI 会先规划网站和当前页面，再一次只提交一个可审阅的视觉步骤。页面不需要一轮完成。</p>
      <small>开始后，这里会显示真实截图、视觉差异、质量结论和接受/退回操作。</small>
      <button className="secondary-button" onClick={() => void refreshGenerationState(true).catch((error) => showToast(error instanceof Error ? error.message : String(error)))}>刷新进度</button>
    </div>;
    const plan = generationPlan;
    const activePage = plan.pages.find((page) => page.pageId === plan.activePage?.pageId);
    const candidate = generationReview?.candidate;
    const artifacts = [...new Map([...(candidate?.artifacts ?? []), ...(generationReview?.attempt.artifacts ?? [])]
      .map((artifact) => [artifact.artifactId, artifact])).values()];
    const imageArtifacts = artifacts.filter((artifact) => ['page-snapshot', 'region-crop', 'visual-diff'].includes(artifact.kind));
    const statusLabel: Record<string, string> = {
      draft: '规划中', ready: '待开始', running: '设计中', paused: '已暂停', completed: '已完成', failed: '需要处理',
      planned: '已规划', generating: '生成中', validating: '视觉验收中', 'awaiting-review': '等待你审阅', accepted: '已接受',
      rejected: '已退回', retryable: '等待重做', blocked: '被阻塞', stale: '需要更新', 'rolled-back': '已回滚'
    };
    return <div className="generation-review-panel">
      <section className="generation-plan-summary">
        <header><div><span>AI 设计计划</span><strong>{plan.objective}</strong></div><em className={`generation-status ${plan.status}`}>{statusLabel[plan.status] ?? plan.status}</em></header>
        <p>{plan.audience.join(' · ')}</p>
        <small>Plan r{plan.revision} · {plan.mode === 'auto-current-page' ? '当前页面自动推进' : '逐步审阅模式'}</small>
      </section>
      {!plan.deliveryGate.taskCompletionAllowed && <section className={`generation-delivery-gate ${plan.deliveryGate.visibleSceneReady ? 'in-progress' : 'empty'}`}>
        <strong>{plan.deliveryGate.visibleSceneReady ? '设计仍在进行，不能整体验收' : '当前只有规划，还没有可见设计'}</strong>
        <span>{plan.deliveryGate.visibleSceneReady
          ? `已接受 ${plan.deliveryGate.acceptedVisibleStepCount} 个视觉步骤，完成 ${plan.deliveryGate.completedArtboardCount}/${plan.deliveryGate.plannedArtboardCount} 个画板。`
          : 'AI 必须继续生成并接受第一个可见 Scene 步骤，不能转去只改项目代码。'}</span>
        <small>下一步：{String(plan.deliveryGate.requiredNextAction.tool ?? plan.deliveryGate.requiredNextAction.type ?? '继续当前画板')}</small>
      </section>}
      <div className="generation-plan-pages">
        {plan.pages.map((plannedPage) => <article key={plannedPage.pageId} className={plannedPage.pageId === plan.activePage?.pageId ? 'active' : ''}>
          <span>{plannedPage.order + 1}</span><div><strong>{plannedPage.name}</strong><small>{plannedPage.purpose}</small></div><em>{statusLabel[plannedPage.status] ?? plannedPage.status}</em>
        </article>)}
      </div>
      {activePage?.design && <details className="generation-design-intent" open>
        <summary>当前页面视觉方向</summary>
        <dl><div><dt>美术方向</dt><dd>{activePage.design.artDirection}</dd></div><div><dt>构图</dt><dd>{activePage.design.compositionIntent}</dd></div><div><dt>排版</dt><dd>{activePage.design.typographyIntent}</dd></div><div><dt>图片策略</dt><dd>{activePage.design.imageStrategy}</dd></div></dl>
        <ul>{activePage.design.designAcceptanceCriteria.map((criterion) => <li key={criterion}>{criterion}</li>)}</ul>
      </details>}
      {plan.activeStep && <section className={`generation-active-step ${plan.activeStep.status}`}>
        <header><div><span>当前只做这一小步</span><strong>{plan.activeStep.title}</strong></div><em>{statusLabel[plan.activeStep.status] ?? plan.activeStep.status}</em></header>
        <p>{plan.activeStep.kind} · {plan.activeStep.target.nodeIds.join('、')}</p>
        {plan.activeStep.target.viewportWidths.length > 0 && <small>验收宽度：{plan.activeStep.target.viewportWidths.join(' / ')} px</small>}
      </section>}
      {generationReview && <section className="generation-candidate-review">
        <header><div><span>候选方案</span><strong>{generationReview.step.title}</strong></div><em>Scene r{generationReview.attempt.baseRevision} → r{generationReview.attempt.baseRevision + 1}</em></header>
        {imageArtifacts.length > 0 && <div className="generation-candidate-images">{imageArtifacts.map((artifact) => <figure key={artifact.artifactId}>
          <img src={repository?.generationArtifactImageUrl(documentRef.current?.documentId ?? '', artifact.artifactId)} alt={`${artifact.kind} ${artifact.artifactId}`} onError={(event) => { event.currentTarget.closest('figure')?.classList.add('image-unavailable'); }} />
          <figcaption><strong>{artifact.kind === 'visual-diff' ? '视觉差异' : artifact.kind === 'region-crop' ? '局部截图' : '页面截图'}</strong><span>{artifact.viewportWidth ? `${artifact.viewportWidth}px` : `r${artifact.revision}`}</span></figcaption>
        </figure>)}</div>}
        {candidate && <><p className="generation-quality-summary">{candidate.qualitySummary}</p>
          {candidate.issueIds.length > 0 && <div className="generation-issues"><strong>仍需注意</strong>{candidate.issueIds.map((issue) => <span key={issue}>{issue}</span>)}</div>}
          {candidate.protectionConflicts.length > 0 && <div className="generation-protection-warning"><strong>会触及人工调整</strong><span>{candidate.protectionConflicts.length} 个受保护字段，需要你明确确认。</span></div>}
        </>}
        {generationReview.attempt.error && <div className="generation-attempt-error"><strong>{generationReview.attempt.error.code}</strong><span>{generationReview.attempt.error.message}</span></div>}
      </section>}
      {candidate && plan.activeStep?.status === 'awaiting-review' && <section className="generation-review-actions">
        <button className="ai-button" disabled={Boolean(generationAction)} onClick={() => void runGenerationReviewAction('accept')}>{generationAction === 'accept' ? '正在提交…' : '接受这一小步'}</button>
        <textarea rows={3} maxLength={4000} value={generationRejectionReason} onChange={(event) => setGenerationRejectionReason(event.target.value)} placeholder="指出具体视觉问题，例如层级、留白、构图或图片不符合方向…" />
        <button className="secondary-button danger" disabled={Boolean(generationAction) || !generationRejectionReason.trim()} onClick={() => void runGenerationReviewAction('reject')}>{generationAction === 'reject' ? '正在退回…' : '退回并让 AI 重做'}</button>
      </section>}
      <div className="generation-plan-controls">
        {plan.status === 'paused'
          ? <button disabled={Boolean(generationAction)} onClick={() => void runGenerationReviewAction('resume')}>继续 AI 设计</button>
          : plan.status === 'running' && <button disabled={Boolean(generationAction)} onClick={() => void runGenerationReviewAction('pause')}>暂停 AI 设计</button>}
        <button disabled={generationLoading} onClick={() => void refreshGenerationState(true).catch((error) => showToast(error instanceof Error ? error.message : String(error)))}>刷新</button>
      </div>
    </div>;
  }

  function renderWorkspaceArtboard(artboard: WorkspaceArtboardPlacement) {
    const currentDocument = documentRef.current;
    if (!currentDocument) return null;
    const targetDevice = deviceForWorkspaceArtboard(currentDocument, artboard);
    const targetCanvasHeight = sceneDocument
      ? scenePreviewHeights[artboard.pageId]
        ?? sceneArtboardContentHeight(sceneDocument, artboard.pageId, artboard.viewportWidth, artboard.viewportHeight)
      : artboard.viewportHeight;
    const targetPage = pages.find((page) => page.id === artboard.pageId);
    const active = artboard.artboardId === activeArtboardId;
    const surfaceLabel = WORKSPACE_SURFACE_LABELS[artboard.surfaceKind];
    const viewport = canvasScroll.current;
    const viewportSize = viewport ? { width: viewport.clientWidth, height: viewport.clientHeight } : undefined;
    const renderTier = workspaceViewportReady(viewportSize)
      ? workspaceArtboardRenderTier(
          workspaceCamera,
          workspaceArtboardContentBounds(currentDocument, { ...artboard, x: 0, y: 0 }, sceneDocument),
          viewportSize,
          active
        )
      : 'runtime';
    if (renderTier === 'anchor') return <div
      key={artboard.artboardId}
      className="workspace-artboard-anchor"
      style={{ left: 0, top: 0, width: artboard.viewportWidth, height: targetCanvasHeight }}
      data-artboard-id={artboard.artboardId}
      data-render-tier="anchor"
      aria-hidden="true"
    />;
    const contentVisible = renderTier === 'content' || renderTier === 'runtime';
    return <div
      key={artboard.artboardId}
      className={`workspace-artboard surface-${artboard.surfaceKind} ${active ? 'active' : ''}`}
      style={{ left: 0, top: 0, width: artboard.viewportWidth, height: targetCanvasHeight }}
      data-artboard-id={artboard.artboardId}
      data-surface-kind={artboard.surfaceKind}
      data-render-tier={renderTier}
    >
      <div className="workspace-artboard-header">
        <span className="workspace-artboard-status" />
        <strong>{targetPage?.name ?? artboard.pageId}</strong>
        <span className="workspace-surface-kind">{surfaceLabel}</span>
        <em>{artboard.viewportWidth} × 自动 {Math.round(targetCanvasHeight)}</em>
        <small>当前画板</small>
      </div>
      <div className={`design-canvas workspace-projected-canvas device-${targetDevice}`} style={{
        width: artboard.viewportWidth,
        height: targetCanvasHeight,
        background: currentDocument.viewport.background,
        fontFamily: tokens?.typography.fontFamily,
        fontSize: tokens?.typography.baseFontSize,
        '--color-primary': tokens?.colors.primary,
        '--color-accent': tokens?.colors.accent,
        '--color-surface': tokens?.colors.surface,
        '--color-text': tokens?.colors.text,
        '--color-muted': tokens?.colors.muted,
        '--radius-small': `${tokens?.radii.small ?? 8}px`,
        '--radius-medium': `${tokens?.radii.medium ?? 16}px`,
        '--radius-large': `${tokens?.radii.large ?? 28}px`
      } as CSSProperties}
        data-page-id={artboard.pageId}
        data-artboard-id={artboard.artboardId}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => { if (sceneDocument) void onSceneCanvasDrop(event, artboard); }}>
        {contentVisible && sceneDocument && <SceneArtboardCanvas
          scene={sceneDocument}
          pageId={artboard.pageId}
          viewportWidth={artboard.viewportWidth}
          viewportHeight={artboard.viewportHeight}
          active={active && !interactionMode}
          interactive={interactionMode}
          selectionOnly={workspaceShell.activeTool === 'comment'}
          selectedIds={active ? selectedIds : []}
          primaryId={active ? selectedId : undefined}
          onSelectionChange={(ids, primary) => {
            setSelectedIds(ids);
            setSelectedId(primary);
            if (primary && workspaceShell.activeTool === 'comment') {
              setInspectorTab('ai');
              if (!workspaceShell.rightPanelOpen) dispatchWorkspaceShell({ type: 'toggle-right-panel' });
              showToast('已定位 Scene 图层，请在右侧添加视觉批注');
            }
          }}
          onCommit={commitSceneCommand}
          onError={showToast}
          onPrototypeActivate={(link) => activateScenePrototype(link)}
          contentFocus={sceneContentFocus?.pageId === artboard.pageId ? sceneContentFocus : undefined}
          onContentHeightChange={(height) => updateScenePreviewHeight(artboard.pageId, height)}
        />}
        {contentVisible && !sceneDocument && <div className={`scene-v2-load-state ${sceneLoadState}`} style={{ minHeight: artboard.viewportHeight }}>
          {sceneLoadState === 'loading'
            ? <><span className="loading-dot" /><strong>正在读取 AI 设计场景…</strong></>
            : generationPlan
              ? <><strong>AI 只完成了规划，还没有生成画面</strong><span>当前设计不能交付；AI 必须继续执行 {String(generationPlan.deliveryGate.requiredNextAction.tool ?? '下一个视觉步骤')}。</span></>
              : <><strong>这个设计还没有 Scene 画布</strong><span>请让 AI 先规划当前页面并生成第一个视觉步骤。</span></>}
        </div>}
        {!active && <button className="workspace-artboard-activation" onClick={() => activateWorkspaceArtboard(artboard)}><span>选择此画板</span></button>}
      </div>
    </div>;
  }

  function renderPreviewSurfaceOverlay() {
    if (!previewOverlayPage || !documentRef.current || !sceneDocument) return null;
    const currentDocument = documentRef.current;
    const surfaceKind = previewOverlayPage.surfaceKind ?? previewOverlayArtboard?.surfaceKind ?? 'modal';
    const defaultSize = surfaceKind === 'page' || surfaceKind === 'state' ? { width: 960, height: 720 } : WORKSPACE_SURFACE_SIZES[surfaceKind];
    const width = previewOverlayArtboard?.viewportWidth ?? defaultSize.width;
    const height = previewOverlayArtboard?.viewportHeight ?? defaultSize.height;
    const overlayHeight = sceneArtboardContentHeight(sceneDocument, previewOverlayPage.id, width, height);
    const frameHeight = Math.min(overlayHeight, Math.max(320, window.innerHeight - 96));
    return <div className={`preview-surface-backdrop surface-${surfaceKind}`} onPointerDown={() => setPreviewOverlayPageId(undefined)}>
      <div className="preview-surface-frame" style={{ width, height: frameHeight }} onPointerDown={(event) => event.stopPropagation()}>
        <button className="preview-surface-close" aria-label="关闭叠层" onClick={() => setPreviewOverlayPageId(undefined)}>×</button>
        <div className="preview-surface-canvas design-canvas device-desktop" style={{
          width,
          height: overlayHeight,
          background: currentDocument.viewport.background,
          fontFamily: tokens?.typography.fontFamily,
          fontSize: tokens?.typography.baseFontSize,
          '--color-primary': tokens?.colors.primary,
          '--color-accent': tokens?.colors.accent,
          '--color-surface': tokens?.colors.surface,
          '--color-text': tokens?.colors.text,
          '--color-muted': tokens?.colors.muted,
          '--radius-small': `${tokens?.radii.small ?? 8}px`,
          '--radius-medium': `${tokens?.radii.medium ?? 16}px`,
          '--radius-large': `${tokens?.radii.large ?? 28}px`
        } as CSSProperties}>
          <SceneArtboardCanvas
            scene={sceneDocument}
            pageId={previewOverlayPage.id}
            viewportWidth={width}
            viewportHeight={height}
            active={false}
            interactive
            selectedIds={[]}
            onSelectionChange={() => undefined}
            onCommit={commitSceneCommand}
            onError={showToast}
            onPrototypeActivate={(link) => activateScenePrototype(link)}
          />
        </div>
      </div>
    </div>;
  }


  return {
    renderGenerationReviewPanel,
    renderWorkspaceArtboard,
    renderPreviewSurfaceOverlay
  };
}
