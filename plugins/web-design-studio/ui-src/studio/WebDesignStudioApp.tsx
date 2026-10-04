import { uiLibraryByName } from '../../src/ui-libraries';
import type { WebDesignLibraryName } from '../../src/schema';
import { workspaceArtboardSignature } from './WebDesignStudioSupport';
import { createWebDesignCoreActions } from './WebDesignCoreActions';
import { createWebDesignInsertActions } from './WebDesignInsertActions';
import { createWebDesignCanvasActions } from './WebDesignCanvasActions';
import { createWebDesignViewportActions } from './WebDesignViewportActions';
import { createWebDesignSelectionActions } from './WebDesignSelectionActions';
import { createWebDesignAssetActions } from './WebDesignAssetActions';
import { createWebDesignDocumentActions } from './WebDesignDocumentActions';
import { formatProjectDate } from './WebDesignInspectorFields';
import { createWebDesignRenderHelpers } from './WebDesignRenderHelpers';
import { renderWebDesignStudioWorkspace } from './WebDesignStudioWorkspace';
import { createWebDesignWorkspaceContext } from './WebDesignWorkspaceContext';
import { useWebDesignStudioState } from './useWebDesignStudioState';
import type { WebDesignDeferredActions } from './WebDesignActionContracts';
export function WebDesignStudioApp() {
  const studioState = useWebDesignStudioState();
  const {
    actionsRef,
    repository,
    activeProject,
    document,
    ready,
    screen,
    toast,
    newDesignOpen,
    setNewDesignOpen,
    newDesignName,
    setNewDesignName,
    deleteDesignTarget,
    setDeleteDesignTarget,
    deletingDesign,
    selectedSceneNode,
    activeProjectDocuments,
  } = studioState;
  const selectedSceneLibrary = selectedSceneNode?.type === 'library-instance'
    ? uiLibraryByName(selectedSceneNode.library as WebDesignLibraryName)
    : undefined;
  const selectedSceneLibraryDefinition = selectedSceneNode?.type === 'library-instance'
    ? selectedSceneLibrary?.components.find((item) => item.id === selectedSceneNode.component)
    : undefined;
  const selectedSceneLibraryVariants = selectedSceneNode?.type === 'library-instance' && selectedSceneLibrary
    ? selectedSceneLibrary.variants[selectedSceneNode.component]
      ?? [{ id: 'default', label: '默认款式', props: {} }]
    : [];

  let webDesignCanvasActions!: ReturnType<typeof createWebDesignCanvasActions>;
  let webDesignViewportActions!: ReturnType<typeof createWebDesignViewportActions>;
  let webDesignSelectionActions!: ReturnType<typeof createWebDesignSelectionActions>;
  const deferredActions: WebDesignDeferredActions = {
    editComponentSlot: (...args) => webDesignCanvasActions.editComponentSlot(...args),
    activateWorkspaceArtboard: (...args) => webDesignViewportActions.activateWorkspaceArtboard(...args),
    withGeneratedResponsiveLayouts: (...args) =>
      webDesignViewportActions.withGeneratedResponsiveLayouts(...args),
    selectComponent: (...args) => webDesignSelectionActions.selectComponent(...args),
    selectableNodesForCurrentEditor: (...args) =>
      webDesignSelectionActions.selectableNodesForCurrentEditor(...args),
    copySceneSelection: () => webDesignSelectionActions.copySceneSelection(),
    duplicateSceneSelection: () => webDesignSelectionActions.duplicateSceneSelection(),
    pasteSceneClipboard: () => webDesignSelectionActions.pasteSceneClipboard()
  };

  const webDesignCoreActions = createWebDesignCoreActions(studioState);
  const {
    showToast, updateScenePreviewHeight, chooseLibraryTab, activateWorkspaceArea, activateWorkspaceTool, setCurrent,
    openDocument, commit, commitWithCanvasGrowth, changeLive, changeLiveWithCanvasGrowth, historyDocument,
    applySceneDocument, refreshSceneHistory, refreshGenerationState, runGenerationReviewAction, commitSceneCommand, undoScene,
    redoScene, undo, redo, updateComponent, save, refresh,
    createNew, refreshCatalog, createDesignFromSheet, openProjectDocument, goToActiveProject, confirmDeleteProjectDocument
  } = webDesignCoreActions;

  const webDesignInsertActions = createWebDesignInsertActions({
    ...studioState,
    ...webDesignCoreActions,
    ...deferredActions
  });
  const {
    onPaletteDrag, addUiLibraryComponent, scenePageRoot, insertSceneLibraryComponent, insertSceneBasicShape, onSceneCanvasDrop,
    insertUiLibraryComponent, chooseUiLibraryPreviewElement, beginUiLibraryPreviewPointerDrag, moveUiLibraryPreviewPointerDragAt, finishUiLibraryPreviewPointerDragAt, handleUiLibraryPreviewPointerEvent,
    dropUiLibraryPreviewElement
  } = webDesignInsertActions;

  webDesignCanvasActions = createWebDesignCanvasActions({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...deferredActions
  });
  const {
    enterSlotEditor, resetSlotEditorCamera, editComponentSlot, exitSlotEditor, insertSlotTemplate, onCanvasDrop,
    beginInteraction, beginCanvasPan, beginCanvasMarquee, updateSelected, updateSelectedFrame, updateInspectedFrame,
    updateSelectedStyle, clearSelectedVisualState, updateSelectedCustomCss, updateSelectedHorizontalConstraint, updateSelectedSizeConstraints, deleteSelected,
    duplicateSelected, copySelected, pasteClipboard, reorderSelected, alignSelected, nudgeSelected,
    toggleHidden, toggleLocked
  } = webDesignCanvasActions;

  webDesignViewportActions = createWebDesignViewportActions({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...webDesignCanvasActions
  });
  const {
    activateWorkspaceArtboard, updateActiveWorkspaceViewport, addWorkspaceSurface, fitWorkspaceArtboard, fitActiveWorkspaceArtboard, fitSlotEditorContent,
    fitWorkspaceSelection, focusWorkspaceArtboard, focusWorkspaceArtboardByPageId, updateBreakpoint, selectViewportPreset, updateCustomViewportWidth,
    updateCustomViewportHeight, withGeneratedResponsiveLayouts, generateResponsiveLayouts, fitCanvasToWidth, fitCanvasWidth, setCanvasZoom,
    toggleInteractionMode
  } = webDesignViewportActions;

  webDesignSelectionActions = createWebDesignSelectionActions({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...webDesignCanvasActions,
    ...webDesignViewportActions,
    selectedSceneLibrary,
    selectedSceneLibraryDefinition,
    selectedSceneLibraryVariants
  });
  const {
    selectComponent, selectableNodesForCurrentEditor,
    selectionOverlayItemsFor, selectSelectionChild, selectSelectionParent, sceneSelectionRootIds, cloneSceneSubtree, insertSceneCopies,
    copySceneSelection, duplicateSceneSelection, pasteSceneClipboard, wrapSceneSelection, ungroupSceneSelection, nudgeSceneSelection,
    alignSceneSelection, distributeSceneSelection, reorderSceneSelection, deleteSceneSelection, updateSceneNodeById, updateSceneNode,
    applySelectedSceneLibraryVariant, focusSceneContent, updateSelectedSceneResponsiveOverride, replaceSelectedSceneResponsiveOverride, clearSelectedSceneResponsiveOverride
  } = webDesignSelectionActions;

  const webDesignAssetActions = createWebDesignAssetActions({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...webDesignCanvasActions,
    ...webDesignViewportActions,
    ...webDesignSelectionActions
  });
  const {
    groupSelected, ungroupSelected, updateSelectedLayout, applySelectedAutoLayout, saveSelectionAsSymbol, insertSymbol,
    renamePersonalSymbol, removePersonalSymbol, saveSceneSelectionAsSnippet, insertSceneSnippet, renameSceneSnippet, removeSceneSnippet,
    applySceneVariablesDraft, seedSceneVariables, toggleSelectedSymbolOverride, synchronizeSelectedSymbol, updateSelectedSymbolDefinition, updateSelectedLibraryProp,
    applySelectedLibraryVariant, detachSelectedSymbol, updateTokens, applyDesignTheme, updateTokenColor, applyColorToken,
    applyRadiusToken
  } = webDesignAssetActions;

  const webDesignDocumentActions = createWebDesignDocumentActions({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...webDesignCanvasActions,
    ...webDesignViewportActions,
    ...webDesignSelectionActions,
    ...webDesignAssetActions
  });
  const {
    switchPage, addPage, duplicateScenePage, duplicatePage, deleteCurrentPage, updateCurrentPage,
    useAsset, importAssets, downloadTextFile, exportCurrentPage, exportReact, exportVue,
    activatePreviewInteraction, activateScenePrototype, addLegacyAnnotation, prepareSceneAnnotation, addSceneAnnotation, changeSceneAnnotationStatus,
    submitSceneAiInstruction, addAiRequest
  } = webDesignDocumentActions;

  actionsRef.current = {
    openDocument,
    showToast,
    changeLiveWithCanvasGrowth,
    toggleInteractionMode,
    save,
    undo,
    redo,
    ungroupSelected,
    groupSelected,
    duplicateSelected,
    copySelected,
    pasteClipboard,
    selectSelectionChild,
    selectSelectionParent,
    activateWorkspaceTool,
    deleteSceneSelection,
    deleteSelected,
    nudgeSceneSelection,
    nudgeSelected
  };

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

  const webDesignRenderHelpers = createWebDesignRenderHelpers({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...webDesignCanvasActions,
    ...webDesignViewportActions,
    ...webDesignSelectionActions,
    ...webDesignAssetActions,
    ...webDesignDocumentActions
  });
  return renderWebDesignStudioWorkspace(createWebDesignWorkspaceContext({
    ...studioState,
    ...webDesignCoreActions,
    ...webDesignInsertActions,
    ...webDesignCanvasActions,
    ...webDesignViewportActions,
    ...webDesignSelectionActions,
    ...webDesignAssetActions,
    ...webDesignDocumentActions,
    ...webDesignRenderHelpers,
    selectedSceneLibrary,
    selectedSceneLibraryDefinition,
    selectedSceneLibraryVariants,
    storageBadge,
    newDesignModal,
    deleteDesignModal,
  }));
}
