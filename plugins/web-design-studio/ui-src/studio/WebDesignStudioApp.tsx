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
import { officialRuntimePresentation } from '../library-runtime/registry';
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
import { libraryPreviewSelection, type LibraryPreviewPointerEvent, type LibraryPreviewSelection } from '../library-runtime/element-selection';
import { indexSceneDocument, isSceneContainer, isSceneSlotContainer, type SceneDocument, type SceneNode, type ScenePrototypeLink, type SceneResponsiveNodeOverride, type SceneVariableCollection } from '../../src/v2/scene-schema';
import { inspectorCapabilities as resolveInspectorCapabilities } from './inspector-model';
import { editableSlotsForSceneLibraryNode, resolveSceneInsertionTarget, type SceneInsertionFocus, type SceneInsertionTarget } from './scene-insertion-target';

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
import { createWebDesignRenderHelpers } from './WebDesignRenderHelpers';
import { renderWebDesignStudioWorkspace } from './WebDesignStudioWorkspace';
import { useWebDesignStudioState } from './useWebDesignStudioState';
export function WebDesignStudioApp() {
  const studioState = useWebDesignStudioState();
  const actionContext: Record<string, any> = { ...studioState };
  for (const actionName of ["showToast","updateScenePreviewHeight","chooseLibraryTab","activateWorkspaceArea","activateWorkspaceTool","setCurrent","openDocument","commit","commitWithCanvasGrowth","changeLive","changeLiveWithCanvasGrowth","historyDocument","applySceneDocument","refreshSceneHistory","refreshGenerationState","runGenerationReviewAction","commitSceneCommand","undoScene","redoScene","undo","redo","updateComponent","save","refresh","createNew","refreshCatalog","createDesignFromSheet","openProjectDocument","goToActiveProject","confirmDeleteProjectDocument","onPaletteDrag","addUiLibraryComponent","scenePageRoot","insertSceneLibraryComponent","insertSceneBasicShape","onSceneCanvasDrop","insertUiLibraryComponent","chooseUiLibraryPreviewElement","beginUiLibraryPreviewPointerDrag","moveUiLibraryPreviewPointerDragAt","finishUiLibraryPreviewPointerDragAt","handleUiLibraryPreviewPointerEvent","dropUiLibraryPreviewElement","enterSlotEditor","resetSlotEditorCamera","editComponentSlot","exitSlotEditor","insertSlotTemplate","onCanvasDrop","beginInteraction","beginCanvasPan","beginCanvasMarquee","updateSelected","updateSelectedFrame","updateInspectedFrame","updateSelectedStyle","clearSelectedVisualState","updateSelectedCustomCss","updateSelectedHorizontalConstraint","updateSelectedSizeConstraints","deleteSelected","duplicateSelected","copySelected","pasteClipboard","reorderSelected","alignSelected","nudgeSelected","toggleHidden","toggleLocked","activateWorkspaceArtboard","updateActiveWorkspaceViewport","addWorkspaceSurface","fitWorkspaceArtboard","fitActiveWorkspaceArtboard","fitSlotEditorContent","fitWorkspaceSelection","focusWorkspaceArtboard","focusWorkspaceArtboardByPageId","updateBreakpoint","selectViewportPreset","updateCustomViewportWidth","updateCustomViewportHeight","withGeneratedResponsiveLayouts","generateResponsiveLayouts","fitCanvasToWidth","fitCanvasWidth","setCanvasZoom","toggleInteractionMode","selectComponent","selectableNodesForCurrentEditor","selectionOverlayItemsFor","selectSelectionChild","selectSelectionParent","sceneSelectionRootIds","cloneSceneSubtree","insertSceneCopies","copySceneSelection","duplicateSceneSelection","pasteSceneClipboard","wrapSceneSelection","ungroupSceneSelection","nudgeSceneSelection","alignSceneSelection","distributeSceneSelection","reorderSceneSelection","deleteSceneSelection","updateSceneNodeById","updateSceneNode","applySelectedSceneLibraryVariant","focusSceneContent","updateSelectedSceneResponsiveOverride","replaceSelectedSceneResponsiveOverride","clearSelectedSceneResponsiveOverride","groupSelected","ungroupSelected","updateSelectedLayout","applySelectedAutoLayout","saveSelectionAsSymbol","insertSymbol","renamePersonalSymbol","removePersonalSymbol","saveSceneSelectionAsSnippet","insertSceneSnippet","renameSceneSnippet","removeSceneSnippet","applySceneVariablesDraft","seedSceneVariables","toggleSelectedSymbolOverride","synchronizeSelectedSymbol","updateSelectedSymbolDefinition","updateSelectedLibraryProp","applySelectedLibraryVariant","detachSelectedSymbol","updateTokens","applyDesignTheme","updateTokenColor","applyColorToken","applyRadiusToken","switchPage","addPage","duplicateScenePage","duplicatePage","deleteCurrentPage","updateCurrentPage","useAsset","importAssets","downloadTextFile","exportCurrentPage","exportReact","exportVue","activatePreviewInteraction","activateScenePrototype","addLegacyAnnotation","prepareSceneAnnotation","addSceneAnnotation","changeSceneAnnotationStatus","submitSceneAiInstruction","addAiRequest","renderGenerationReviewPanel","renderWorkspaceArtboard","renderPreviewSurfaceOverlay"]) {
    actionContext[actionName] = (...args: any[]) => actionContext['implementation:' + actionName](...args);
  }
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
    editingVisibleComponents, editingSlotCanvasSize, inspectedFrame, inspectedStyle
  } = studioState;
  const webDesignCoreActions = createWebDesignCoreActions(actionContext);
  const {
    showToast, updateScenePreviewHeight, chooseLibraryTab, activateWorkspaceArea, activateWorkspaceTool, setCurrent,
    openDocument, commit, commitWithCanvasGrowth, changeLive, changeLiveWithCanvasGrowth, historyDocument,
    applySceneDocument, refreshSceneHistory, refreshGenerationState, runGenerationReviewAction, commitSceneCommand, undoScene,
    redoScene, undo, redo, updateComponent, save, refresh,
    createNew, refreshCatalog, createDesignFromSheet, openProjectDocument, goToActiveProject, confirmDeleteProjectDocument
  } = webDesignCoreActions;
  for (const [name, implementation] of Object.entries(webDesignCoreActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const webDesignInsertActions = createWebDesignInsertActions(actionContext);
  const {
    onPaletteDrag, addUiLibraryComponent, scenePageRoot, insertSceneLibraryComponent, insertSceneBasicShape, onSceneCanvasDrop,
    insertUiLibraryComponent, chooseUiLibraryPreviewElement, beginUiLibraryPreviewPointerDrag, moveUiLibraryPreviewPointerDragAt, finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent,
    dropUiLibraryPreviewElement
  } = webDesignInsertActions;
  for (const [name, implementation] of Object.entries(webDesignInsertActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const webDesignCanvasActions = createWebDesignCanvasActions(actionContext);
  const {
    enterSlotEditor, resetSlotEditorCamera, editComponentSlot, exitSlotEditor, insertSlotTemplate, onCanvasDrop,
    beginInteraction, beginCanvasPan, beginCanvasMarquee, updateSelected, updateSelectedFrame, updateInspectedFrame,
    updateSelectedStyle, clearSelectedVisualState, updateSelectedCustomCss, updateSelectedHorizontalConstraint, updateSelectedSizeConstraints, deleteSelected,
    duplicateSelected, copySelected, pasteClipboard, reorderSelected, alignSelected, nudgeSelected,
    toggleHidden, toggleLocked
  } = webDesignCanvasActions;
  for (const [name, implementation] of Object.entries(webDesignCanvasActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const webDesignViewportActions = createWebDesignViewportActions(actionContext);
  const {
    activateWorkspaceArtboard, updateActiveWorkspaceViewport, addWorkspaceSurface, fitWorkspaceArtboard, fitActiveWorkspaceArtboard, fitSlotEditorContent,
    fitWorkspaceSelection, focusWorkspaceArtboard, focusWorkspaceArtboardByPageId, updateBreakpoint, selectViewportPreset, updateCustomViewportWidth,
    updateCustomViewportHeight, withGeneratedResponsiveLayouts, generateResponsiveLayouts, fitCanvasToWidth, fitCanvasWidth, setCanvasZoom,
    toggleInteractionMode
  } = webDesignViewportActions;
  for (const [name, implementation] of Object.entries(webDesignViewportActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const webDesignSelectionActions = createWebDesignSelectionActions(actionContext);
  const {
    selectComponent, selectableNodesForCurrentEditor,
    selectionOverlayItemsFor, selectSelectionChild, selectSelectionParent, sceneSelectionRootIds, cloneSceneSubtree, insertSceneCopies,
    copySceneSelection, duplicateSceneSelection, pasteSceneClipboard, wrapSceneSelection, ungroupSceneSelection, nudgeSceneSelection,
    alignSceneSelection, distributeSceneSelection, reorderSceneSelection, deleteSceneSelection, updateSceneNodeById, updateSceneNode,
    applySelectedSceneLibraryVariant, focusSceneContent, updateSelectedSceneResponsiveOverride, replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride
  } = webDesignSelectionActions;
  for (const [name, implementation] of Object.entries(webDesignSelectionActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const webDesignAssetActions = createWebDesignAssetActions(actionContext);
  const {
    groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout, saveSelectionAsSymbol, insertSymbol,
    renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet, renameSceneSnippet, removeSceneSnippet,
    applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol, updateSelectedSymbolDefinition, updateSelectedLibraryProp,
    applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme, updateTokenColor, applyColorToken,
    applyRadiusToken
  } = webDesignAssetActions;
  for (const [name, implementation] of Object.entries(webDesignAssetActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const webDesignDocumentActions = createWebDesignDocumentActions(actionContext);
  const {
    switchPage, addPage, duplicateScenePage, duplicatePage, deleteCurrentPage, updateCurrentPage,
    useAsset, importAssets, downloadTextFile, exportCurrentPage, exportReact, exportVue,
    activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation, prepareSceneAnnotation, addSceneAnnotation, changeSceneAnnotationStatus,
    submitSceneAiInstruction, addAiRequest
  } = webDesignDocumentActions;
  for (const [name, implementation] of Object.entries(webDesignDocumentActions)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  const storageBadge = <span className={`service-pill ${repository?.mode === 'server' ? 'online' : ''}`}>{repository?.mode === 'server' ? '本地服务' : '浏览器存储'}</span>;
  const newDesignModal = newDesignOpen && activeProject && <div className="studio-modal-backdrop" onPointerDown={() => setNewDesignOpen(false)}>
    <section className="studio-modal project-create-modal" onPointerDown={(event) => event.stopPropagation()}>
      <header><div><span className="eyebrow">{activeProject.name}</span><h2>新建设计工作区</h2><p>先创建空的设计范围，再让 AI 逐页规划、分步骤生成和视觉验收；不会自动塞入演示模板。</p></div><button onClick={() => setNewDesignOpen(false)}>×</button></header>
      <div className="project-form-body">
        <label>设计名称<input autoFocus maxLength={240} value={newDesignName} onChange={(event) => setNewDesignName(event.target.value)} placeholder="例如：官网改版 2026" /></label>
        <div className="ai-first-create-note"><span>✦</span><div><strong>AI 分步设计</strong><small>先定页面清单和视觉方向，再一次完成一个有界步骤。复杂页面可以多轮完善。</small></div></div>
      </div>
      <footer className="project-modal-actions"><button className="quiet-button" onClick={() => setNewDesignOpen(false)}>取消</button><button className="primary-button" disabled={!newDesignName.trim()} onClick={() => void createDesignFromSheet()}>创建并打开</button></footer>
    </section>
  </div>;
  const deleteDesignModal = deleteDesignTarget && <div
    className="studio-modal-backdrop"
    onPointerDown={() => { if (!deletingDesign) setDeleteDesignTarget(undefined); }}
  >
    <section
      className="studio-modal design-delete-modal"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="delete-design-title"
      aria-describedby="delete-design-description"
      onPointerDown={(event) => event.stopPropagation()}
    >
      <header><div><span className="eyebrow">永久删除</span><h2 id="delete-design-title">删除“{deleteDesignTarget.title}”？</h2><p id="delete-design-description">画板、组件、批注和设计历史都会被删除，此操作无法撤销。</p></div><button disabled={deletingDesign} onClick={() => setDeleteDesignTarget(undefined)} aria-label="关闭删除确认">×</button></header>
      <footer className="project-modal-actions"><button className="quiet-button" autoFocus disabled={deletingDesign} onClick={() => setDeleteDesignTarget(undefined)}>取消</button><button className="primary-button destructive-button" disabled={deletingDesign} onClick={() => void confirmDeleteProjectDocument()}>{deletingDesign ? '正在删除…' : '永久删除'}</button></footer>
    </section>
  </div>;

  if (!ready) return <div className="loading-screen"><div className="loading-dot" />正在准备 Web Design Studio…</div>;

  if (screen === 'project' && activeProject) return <div className="web-project-shell">
    <header className="web-project-toolbar"><div className="brand"><span className="brand-mark">W</span><span>{activeProject.name}</span>{storageBadge}</div><button className="primary-button" onClick={() => void createNew()}>＋ 新建设计</button></header>
    <main className="web-project-home">
      <section className="web-project-intro"><div><span className="eyebrow">网站项目</span><h1>{activeProject.name}</h1><p>{activeProject.description || `项目内共有 ${activeProjectDocuments.length} 份网站设计。`}</p></div><button className="web-project-new-card" onClick={() => void createNew()}><span>＋</span><strong>新建设计工作区</strong><small>由 AI 逐页规划、分步生成，人负责审阅和批注</small></button></section>
      <section className="web-project-section"><div className="web-project-section-heading"><h2>项目设计</h2><span>{activeProjectDocuments.length} 份</span></div>
        {activeProjectDocuments.length ? <div className="web-design-grid">{activeProjectDocuments.map((item) => <article className="web-design-card" key={item.documentId}>
          <button className="web-design-card-open" onClick={() => void openProjectDocument(item.documentId)}><span className="web-design-thumbnail"><i /><i /><i /></span><span className="web-project-card-copy"><strong>{item.title}</strong><small>{item.pageCount ?? 1} 个页面 · {item.componentCount} 个组件 · v{item.revision}</small></span><time>{formatProjectDate(item.updatedAt)}</time><b>›</b></button>
          <button className="web-design-delete" aria-label={`删除设计 ${item.title}`} onClick={() => setDeleteDesignTarget(item)}>×</button>
        </article>)}</div> : <div className="web-project-empty"><span>▧</span><strong>这个项目还没有网站设计</strong><p>先创建一份设计，为它单独命名，再进入画布设计页面。</p><button className="primary-button" onClick={() => void createNew()}>＋ 新建网站设计</button></div>}
      </section>
    </main>{newDesignModal}{deleteDesignModal}{toast && <div className="toast">{toast}</div>}
  </div>;

  if (!document || !activeProject) return <div className="loading-screen"><div className="loading-dot" />正在打开网站项目…</div>;

  const layerComponents = flattenComponentTree(document, pageId);
  const sceneLayerNodes = sceneDocument
    ? [...indexSceneDocument(sceneDocument).values()]
      .filter((entry) => entry.pageId === pageId)
      .map((entry) => ({ node: entry.node, depth: Math.max(0, entry.path.length - 2) }))
    : [];
  const directChildCount = selected ? document.components.filter((component) => component.parentId === selected.id).length : 0;
  const canUngroup = Boolean(selected?.id.startsWith('group-') && directChildCount > 0);
  const canUngroupScene = Boolean(selectedSceneNode
    && (selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame')
    && selectedSceneNode.layout.mode === 'free');
  const selectedSceneLibrary = selectedSceneNode?.type === 'library-instance' ? uiLibraryByName(selectedSceneNode.library as WebDesignLibraryName) : undefined;
  const selectedSceneLibraryDefinition = selectedSceneNode?.type === 'library-instance'
    ? selectedSceneLibrary?.components.find((item) => item.id === selectedSceneNode.component)
    : undefined;
  const selectedSceneLibraryVariants = selectedSceneNode?.type === 'library-instance' && selectedSceneLibrary
    ? selectedSceneLibrary.variants[selectedSceneNode.component] ?? [{ id: 'default', label: '默认款式', props: {} }]
    : [];
  const selectedSceneRegistryElement = selectedSceneNode?.type === 'library-instance'
    ? libraryPreviewSelection(selectedSceneNode.properties.registryElement)
    : undefined;
  const selectedSceneEditableSlots = selectedSceneNode?.type === 'library-instance'
    ? editableSlotsForSceneLibraryNode(selectedSceneNode)
    : [];
  const selectedSymbol = selected?.symbolId ? document.symbols?.find((symbol) => symbol.id === selected.symbolId) : undefined;
  const selectedLibrary = uiLibraryByName(selected?.library?.name);
  const selectedLibraryDefinition = selected?.library ? selectedLibrary?.components.find((item) => item.id === selected.library?.component) : undefined;
  const selectedLibraryVariants = selected?.library ? variantsForBoundComponent(selected) : [];
  const selectedRegistryElement = libraryPreviewSelection(selected?.library?.props.registryElement);
  const selectedEditableSlots = selected ? editableSlotsForUiComponent(selected) : [];
  const inspectorCapabilities = selected ? resolveInspectorCapabilities(selected.type, {
    library: Boolean(selected.library),
    directChildCount,
    editableSlotCount: selectedEditableSlots.length
  }) : undefined;
  const selectedInspectableLibraryProps = selected?.library ? inspectableLibraryProps(selected.library.props) : [];
  const aiTarget = selected ?? editingContainer;
  const normalizedPaletteQuery = paletteQuery.trim().toLowerCase();
  const filteredPalette = palette.filter((item) => !normalizedPaletteQuery
    || `${item.label} ${item.id} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredPersonalSymbols = personalSymbols.filter((symbol) => !normalizedPaletteQuery
    || `${symbol.name} ${symbol.components.map((component) => component.name).join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredSceneSnippets = sceneSnippets.filter((snippet) => !normalizedPaletteQuery
    || `${snippet.name} ${snippet.nodes.map((node) => node.name).join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const activeUiLibrary = libraryTab !== 'components' && libraryTab !== 'my' && libraryTab !== 'layers' ? uiLibraryByName(libraryTab) : undefined;
  const filteredUiLibraryComponents = activeUiLibrary?.components.filter((item) => !normalizedPaletteQuery
    || `${item.id} ${item.label} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery)) ?? [];
  const variantPickerLibrary = variantPickerTarget ? uiLibraryByName(variantPickerTarget.library) : undefined;
  const variantPickerDefinition = variantPickerTarget ? variantPickerLibrary?.components.find((item) => item.id === variantPickerTarget.componentId) : undefined;
  const variantPickerVariants = variantPickerDefinition && variantPickerLibrary ? variantPickerLibrary.variants[variantPickerDefinition.id] ?? [{ id: 'default', label: '默认款式', props: {} }] : [];
  const variantPickerPresentation = variantPickerDefinition && variantPickerLibrary
    ? officialRuntimePresentation(variantPickerLibrary.id, String(variantPickerDefinition.props?.componentSlug ?? variantPickerDefinition.id))
    : undefined;
  const sceneAiTarget = selectedSceneNode ?? activeScenePage?.children[0];
  const sceneAnnotationTasks = sceneDocument
    ? [...indexSceneDocument(sceneDocument).values()].flatMap((entry) => entry.node.annotations
      .filter((annotation) => annotation.status === 'open')
      .map((annotation) => ({ annotation, node: entry.node, pageId: entry.pageId })))
    : [];
  const aiQuickPrompts = selectedSceneNode
    ? ['让这个组件更精致、更有层次', '优化尺寸、间距和对齐', '给我 3 个更好看的视觉方案']
    : ['设计一个像 Apple 官网一样克制高级的页面', '统一整页的字号、间距、圆角和色彩', '检查并修复页面中不协调的视觉细节'];

  Object.assign(actionContext, {
    layerComponents, sceneLayerNodes, directChildCount, canUngroup, canUngroupScene,
    selectedSceneLibrary, selectedSceneLibraryDefinition, selectedSceneLibraryVariants,
    selectedSceneRegistryElement, selectedSceneEditableSlots, selectedSymbol, selectedLibrary,
    selectedLibraryDefinition, selectedLibraryVariants, selectedRegistryElement, selectedEditableSlots,
    inspectorCapabilities, selectedInspectableLibraryProps, aiTarget, normalizedPaletteQuery,
    filteredPalette, filteredPersonalSymbols, filteredSceneSnippets, activeUiLibrary,
    filteredUiLibraryComponents, variantPickerLibrary, variantPickerDefinition, variantPickerVariants,
    variantPickerPresentation, sceneAiTarget, sceneAnnotationTasks, aiQuickPrompts
  });
  const webDesignRenderHelpers = createWebDesignRenderHelpers(actionContext);
  const {
    renderGenerationReviewPanel,
    renderWorkspaceArtboard,
    renderPreviewSurfaceOverlay
  } = webDesignRenderHelpers;
  for (const [name, implementation] of Object.entries(webDesignRenderHelpers)) {
    actionContext['implementation:' + name] = implementation;
    actionContext[name] = implementation;
  }
  return renderWebDesignStudioWorkspace({
    repository, activeProject, document, sceneDocument, sceneHistory, ready,
    screen, persistedRevision, selectedId, setSelectedId, selectedIds, setSelectedIds,
    selectionCandidatePopover, setSelectionCandidatePopover, marqueeRect, pageId, clipboard, sceneClipboard,
    dirty, saving, interactionMode, device, workspaceCamera, workspacePlacement,
    newSurfaceKind, setNewSurfaceKind, past, future, toast, annotationText,
    setAnnotationText, aiInstruction, setAiInstruction, sceneAiContext, sceneAnnotationPreparingId, generationPlan,
    paletteQuery, setPaletteQuery, libraryTab, sceneVariablesDraft, setSceneVariablesDraft, setVariantPickerTarget,
    sceneContentFocus, setSceneContentFocus, variantPickerDrag, themePickerOpen, setThemePickerOpen, projectLibraryOpen,
    setProjectLibraryOpen, setDeleteDesignTarget, editingSlot, inspectorVisualState, setInspectorVisualState, inspectorTab,
    setInspectorTab, workspaceShell, dispatchWorkspaceShell, interaction, assetInput, canvasStage,
    canvasScroll, zoom, canvasPanning, canvasPanReady, selected, selectedSceneEntry,
    selectedSceneNode, sceneResponsiveRuleSpec, selectedSceneResponsiveOverride, selectedScenePositionEditable, activeScenePage, sceneEditingActive,
    selectedIdSet, activeWorkspaceArtboard, viewportPresets, renderedCanvasHeight, pages, selectedPrototypeTarget,
    activeProjectDocuments, tokens, currentPage, editingContainer, editingSlotDefinition, editingSlotComponents,
    editingVisibleComponents, editingSlotCanvasSize, inspectedFrame, inspectedStyle, showToast, chooseLibraryTab,
    activateWorkspaceArea, activateWorkspaceTool, undo, redo, save, refresh,
    createNew, openProjectDocument, goToActiveProject, onPaletteDrag, chooseUiLibraryPreviewElement, moveUiLibraryPreviewPointerDragAt,
    finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent, editComponentSlot, exitSlotEditor, insertSlotTemplate, onCanvasDrop,
    beginInteraction, beginCanvasPan, beginCanvasMarquee, updateSelected, updateInspectedFrame, updateSelectedStyle,
    clearSelectedVisualState, updateSelectedCustomCss, updateSelectedHorizontalConstraint, updateSelectedSizeConstraints, deleteSelected, duplicateSelected,
    copySelected, pasteClipboard, reorderSelected, alignSelected, toggleHidden, toggleLocked,
    addWorkspaceSurface, fitActiveWorkspaceArtboard, fitSlotEditorContent, fitWorkspaceSelection, focusWorkspaceArtboardByPageId, selectViewportPreset,
    updateCustomViewportWidth, updateCustomViewportHeight, setCanvasZoom, toggleInteractionMode, selectComponent, selectionOverlayItemsFor,
    copySceneSelection, duplicateSceneSelection, pasteSceneClipboard, wrapSceneSelection, ungroupSceneSelection, alignSceneSelection,
    distributeSceneSelection, reorderSceneSelection, deleteSceneSelection, updateSceneNodeById, updateSceneNode, applySelectedSceneLibraryVariant,
    focusSceneContent, updateSelectedSceneResponsiveOverride, replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride, groupSelected, ungroupSelected,
    updateSelectedLayout, applySelectedAutoLayout, saveSelectionAsSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet, renameSceneSnippet,
    removeSceneSnippet, applySceneVariablesDraft, toggleSelectedSymbolOverride, synchronizeSelectedSymbol, updateSelectedSymbolDefinition, updateSelectedLibraryProp,
    applySelectedLibraryVariant, detachSelectedSymbol, applyDesignTheme, applyColorToken, applyRadiusToken, switchPage,
    addPage, duplicatePage, deleteCurrentPage, updateCurrentPage, useAsset, importAssets,
    activatePreviewInteraction, addLegacyAnnotation, prepareSceneAnnotation, addSceneAnnotation, changeSceneAnnotationStatus, submitSceneAiInstruction,
    addAiRequest, deleteDesignModal, sceneLayerNodes, directChildCount, canUngroup, canUngroupScene,
    selectedSceneLibrary, selectedSceneLibraryDefinition, selectedSceneLibraryVariants, selectedSceneRegistryElement, selectedSceneEditableSlots, selectedSymbol,
    selectedLibrary, selectedLibraryDefinition, selectedLibraryVariants, selectedRegistryElement, selectedEditableSlots, inspectorCapabilities,
    selectedInspectableLibraryProps, filteredPalette, filteredSceneSnippets, activeUiLibrary, filteredUiLibraryComponents, variantPickerLibrary,
    variantPickerDefinition, variantPickerVariants, variantPickerPresentation, sceneAiTarget, sceneAnnotationTasks, aiQuickPrompts,
    renderGenerationReviewPanel, renderWorkspaceArtboard, renderPreviewSurfaceOverlay
  });
}
