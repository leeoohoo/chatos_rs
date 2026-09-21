import type { CSSProperties } from 'react';
import { resolveComponent } from '../../src/editor-model';
import { componentsInSlot, editableSlotsForUiComponent } from '../../src/library-slots';
import { applyUiLibraryVariant, createComponentFromUiLibrary } from '../../src/ui-libraries';
import { WEB_DESIGN_THEME_PRESETS } from '../../src/design-themes';
import type { WebComponentStyle, WebDesignComponent, WebHorizontalConstraint } from '../../src/schema';
import { CanvasComponent as WorkspaceCanvasComponent } from './CanvasComponent';
import { WorkspaceBottomToolbar, WorkspaceNavigationBar, WorkspacePanelResizeHandle } from './WorkspaceShellChrome';
import { workspaceShellGridStyle } from './workspace-shell-model';
import type { WorkspaceSurfaceKind } from '../../src/v2/workspace-placement-store';
import { isSceneContainer, type SceneResponsiveNodeOverride } from '../../src/v2/scene-schema';
import { SelectionOverlay } from './SelectionOverlay';
import {
  palette, fillPresets, shadowPresets, horizontalConstraintOptions,
  WORKSPACE_SURFACE_LABELS, variantDifferenceLabels, OPEN_OVERLAY_PREVIEWS,
  WIDE_VARIANT_PREVIEWS, variantIsInteractive, SelectableVariantCard
} from './WebDesignStudioSupport';
import {
  NumberField, SceneNumberField, ColorValueField, AdvancedCssEditor,
  JsonPropertyEditor, JsonObjectEditor, runtimeSlotContentMap
} from './WebDesignInspectorFields';

type WebDesignWorkspaceContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  ReturnType<typeof import('./WebDesignInsertActions').createWebDesignInsertActions> &
  ReturnType<typeof import('./WebDesignCanvasActions').createWebDesignCanvasActions> &
  ReturnType<typeof import('./WebDesignViewportActions').createWebDesignViewportActions> &
  ReturnType<typeof import('./WebDesignSelectionActions').createWebDesignSelectionActions> &
  ReturnType<typeof import('./WebDesignAssetActions').createWebDesignAssetActions> &
  ReturnType<typeof import('./WebDesignDocumentActions').createWebDesignDocumentActions> &
  ReturnType<typeof import('./WebDesignRenderHelpers').createWebDesignRenderHelpers>;

export function renderWebDesignStudioWorkspace(context: Record<string, any>) {
  const {
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
    renderGenerationReviewPanel, renderWorkspaceArtboard, renderPreviewSurfaceOverlay, layerComponents, aiTarget, normalizedPaletteQuery,
    filteredPersonalSymbols, storageBadge, newDesignModal
  } = context as WebDesignWorkspaceContext & Record<string, any>;

  if (!activeProject || !document) return null;

return (
  <div className="studio-shell">
    <header className="topbar">
      <div className="brand editor-brand"><button className="web-home-button" onClick={goToActiveProject} aria-label="返回当前项目首页">‹</button><span className="brand-mark">W</span></div>
      <button className={`project-library-trigger ${projectLibraryOpen ? 'active' : ''}`} onClick={() => setProjectLibraryOpen((open) => !open)} aria-label="打开当前项目的设计列表">
        <span>⌘</span><strong>{activeProject.name}</strong><small>{activeProjectDocuments.length}</small><b>⌄</b>
      </button>
      <div className="editor-document-title"><span>项目：{activeProject.name}<i>/</i></span><strong>{document.title}</strong><small>{saving ? '正在保存…' : dirty ? '未保存修改' : `已保存 · v${persistedRevision}`}</small></div>
      <button className="quiet-button compact-new-design" aria-label="在当前项目中新建设计" onClick={() => void createNew()}>＋ 新建设计</button>
      <button className="quiet-button style-trigger" onClick={() => setThemePickerOpen(true)}>设计风格</button>
      <div className="history-tools">
        <button title="撤销 ⌘Z" disabled={sceneEditingActive ? !sceneHistory?.undoCount : past.length === 0} onClick={undo}>↶</button>
        <button title="重做 ⇧⌘Z" disabled={sceneEditingActive ? !sceneHistory?.redoCount : future.length === 0} onClick={redo}>↷</button>
      </div>
      <div className="topbar-spacer" />
      <span className={`service-pill ${repository?.mode === 'server' ? 'online' : ''}`}>{repository?.mode === 'server' ? '本地服务' : '浏览器存储'}</span>
      <button className="quiet-button" onClick={() => void refresh()}>刷新</button>
      <button className="ai-design-trigger" onClick={() => activateWorkspaceArea('ai')}>✦ AI 设计</button>
      <button className="primary-button" disabled={!dirty || saving} onClick={() => void save()}>{saving ? '保存中…' : dirty ? '保存' : '已保存'}</button>
    </header>

    {projectLibraryOpen && <div className="project-library-popover">
      <header><div><span className="eyebrow">当前网站项目</span><strong>{activeProject.name}</strong></div><button onClick={() => setProjectLibraryOpen(false)} aria-label="关闭项目设计列表">×</button></header>
      <div className="project-library-list">
        {activeProjectDocuments.map((item) => <div key={item.documentId} className={`project-library-item ${item.documentId === document.documentId ? 'active' : ''}`}>
          <button onClick={() => void openProjectDocument(item.documentId)}><span className="project-library-thumb"><i /><i /><i /></span><span><strong>{item.title}</strong><small>{item.pageCount ?? 1} 个页面 · {item.componentCount} 个组件 · v{item.revision}</small></span></button>
          <button className="project-library-delete" onClick={() => setDeleteDesignTarget(item)} aria-label={`删除设计 ${item.title}`}>×</button>
        </div>)}
      </div>
      <footer><button onClick={() => void createNew()}>＋ 在当前项目中新建设计</button><button onClick={goToActiveProject}>查看项目首页</button></footer>
    </div>}

    <main className="workspace workspace-v3-shell" style={workspaceShellGridStyle(workspaceShell) as CSSProperties}>
      <WorkspaceNavigationBar activeArea={workspaceShell.activeArea} leftPanelOpen={workspaceShell.leftPanelOpen} onSelect={activateWorkspaceArea} onToggleLeft={() => dispatchWorkspaceShell({ type: 'toggle-left-panel' })} />
      {workspaceShell.leftPanelOpen && <aside className="palette-panel">
        {workspaceShell.activeArea === 'assets' && <div className="library-tabs workspace-library-tabs">
          <button className={libraryTab === 'antd' ? 'active' : ''} onClick={() => chooseLibraryTab('antd')}>AntD</button>
          <button className={libraryTab === 'chakra' ? 'active' : ''} onClick={() => chooseLibraryTab('chakra')}>Chakra</button>
          <button className={libraryTab === 'shadcn' ? 'active' : ''} onClick={() => chooseLibraryTab('shadcn')}>shadcn</button>
          <button className={libraryTab === 'magicui' ? 'active' : ''} onClick={() => chooseLibraryTab('magicui')}>Magic</button>
          <button className={libraryTab === 'spell' ? 'active' : ''} onClick={() => chooseLibraryTab('spell')}>Spell</button>
          <button className={libraryTab === 'inspira' ? 'active' : ''} onClick={() => chooseLibraryTab('inspira')}>Inspira</button>
          <button className={libraryTab === 'daisyui' ? 'active' : ''} onClick={() => chooseLibraryTab('daisyui')}>daisyUI</button>
        </div>}
        <div className="palette-panel-content">
          {['assets', 'tools', 'my'].includes(workspaceShell.activeArea) && <input className="component-search" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} placeholder={workspaceShell.activeArea === 'tools' ? '搜索视觉原语…' : activeUiLibrary ? `搜索 ${activeUiLibrary.displayName} 组件…` : '搜索我的组件…'} />}

          {workspaceShell.activeArea === 'tools' && <>
            <div className="panel-intro"><strong>视觉原语</strong><span>用矩形、圆形和直线组合背景、光效、装饰与容器；产品控件使用成熟 UI 库</span></div>
            <div className="palette-grid shapes-grid">{filteredPalette.map((item: any) => <div key={item.id} className="palette-item" draggable onDragStart={(event) => onPaletteDrag(event, item.id)}><span className="palette-icon">{item.icon}</span><span>{item.label}</span></div>)}</div>
          </>}

          {workspaceShell.activeArea === 'assets' && activeUiLibrary && <>
            <div className={`ui-library-heading library-${activeUiLibrary.id}`}><div className="ui-library-logo-mark">{activeUiLibrary.brandMark}</div><div><strong>{activeUiLibrary.displayName}</strong><span>{activeUiLibrary.license ? `开源组件 · ${activeUiLibrary.license} · ${activeUiLibrary.version}` : activeUiLibrary.id === 'shadcn' ? `本地源码组件 · ${activeUiLibrary.version}` : `官方运行时 · v${activeUiLibrary.version}`}</span></div></div>
            <div className="panel-intro"><strong>{activeUiLibrary.displayName} 组件总览</strong><span>先打开组件，再点击或拖动你真正需要的单个官方示例</span></div>
            {activeUiLibrary.categories.map((category: string) => {
              const items = filteredUiLibraryComponents.filter((item: any) => item.category === category);
              return items.length > 0 && <div key={category} className="ui-library-category"><div className="ui-library-category-title">{category}</div><div className="ui-library-component-list">
                {items.map((item: any) => <button key={item.id} onClick={() => setVariantPickerTarget({ library: activeUiLibrary.id, componentId: item.id })}><span className="ui-library-list-icon">{item.icon}</span><strong>{item.id}</strong><small>{item.label}</small><em>{item.status === 'deprecated' ? `已废弃 · ${activeUiLibrary.variants[item.id]?.length ?? 1} 款` : item.introduced ? `v${item.introduced} · ${activeUiLibrary.variants[item.id]?.length ?? 1} 款` : `${activeUiLibrary.variants[item.id]?.length ?? 1} 款`}</em><b>›</b></button>)}
              </div></div>;
            })}
          </>}

          {workspaceShell.activeArea === 'my' && <>
            <div className="panel-intro my-library-intro"><strong>我的设计组合</strong><span>保存真实 Scene 子树，下次插入后仍可继续拆分、移动、批注和让 AI 修改。</span></div>
            {sceneDocument && selectedIds.length > 0 && <button className="my-library-save" onClick={saveSceneSelectionAsSnippet}><span>＋</span><div><strong>保存当前选中</strong><small>{selectedIds.length === 1 ? selectedSceneNode?.name : `${selectedIds.length} 个 Scene 图层`}</small></div></button>}
            {filteredSceneSnippets.length > 0 ? <div className="my-library-grid">{filteredSceneSnippets.map((snippet: any) => <article key={snippet.id} className="my-library-card"><button className="my-library-insert" onClick={() => void insertSceneSnippet(snippet)}><span className="my-library-preview"><i /><i /><i /></span><span><strong>{snippet.name}</strong><small>{snippet.nodes.length} 个根层 · {Math.round(snippet.width)} × {Math.round(snippet.height)}</small></span></button><footer><button onClick={() => renameSceneSnippet(snippet)}>重命名</button><button className="danger" onClick={() => removeSceneSnippet(snippet.id)}>移除</button></footer></article>)}</div> : <div className="my-library-empty"><span>◇</span><strong>还没有保存的设计组合</strong><p>{sceneDocument ? '在画布中选择一个 Scene 图层，或按住 Shift 选择同一容器里的多个图层，再保存为自己的组合。' : '请先让 AI 创建第一个 Scene 页面，再保存可复用的视觉组合。'}</p></div>}
          </>}

          {workspaceShell.activeArea === 'layers' && <>
            <div className="panel-title layer-title"><span>Scene 图层</span><small>{sceneDocument ? sceneLayerNodes.length : 0}</small></div>
            <div className={`layer-group-actions ${selectedIds.length > 1 || canUngroup || canUngroupScene ? 'ready' : ''}`}>
              <div><strong>{selectedIds.length > 1 ? `已选择 ${selectedIds.length} 个图层` : canUngroup || canUngroupScene ? '当前是一个容器' : '创建可整体移动的分组'}</strong><small>{selectedIds.length > 1 ? 'Group、Frame 或 Auto Layout 都会成为真实 Scene 容器' : canUngroup || canUngroupScene ? '可以取消容器并保持内部图层视觉位置' : '按住 Shift 点击画布或图层进行多选'}</small></div>
              {selectedIds.length > 1
                ? <button onClick={groupSelected}>创建分组 <kbd>⌘G</kbd></button>
                : canUngroup || canUngroupScene
                  ? <button onClick={ungroupSelected}>取消分组 <kbd>⇧⌘G</kbd></button>
                  : undefined}
            </div>
            <div className="layers-list expanded">
              {sceneDocument ? sceneLayerNodes.map(({ node, depth }: any) => <div key={node.id} className={`layer-row ${selectedIdSet.has(node.id) ? 'selected' : ''} ${!node.visible ? 'hidden' : ''}`} style={{ paddingLeft: 4 + depth * 14 }} onClick={(event) => {
                const additive = event.shiftKey || event.metaKey || event.ctrlKey;
                if (!additive) {
                  setSelectedId(node.id);
                  setSelectedIds([node.id]);
                  return;
                }
                const next = selectedIds.includes(node.id) ? selectedIds.filter((id) => id !== node.id) : [...selectedIds, node.id];
                setSelectedIds(next);
                setSelectedId(next.includes(node.id) ? node.id : next.at(-1));
              }}>
                <button title={node.visible ? '隐藏' : '显示'} onClick={(event) => { event.stopPropagation(); void updateSceneNodeById(node.id, [{ path: ['visible'], value: !node.visible }]); }}>{node.visible ? '●' : '○'}</button>
                <span className="layer-type">{node.type === 'text' ? 'T' : node.type === 'media' ? '▧' : node.type === 'group' ? '◇' : node.type === 'frame' ? '▣' : '◆'}</span>
                <span className="layer-name">{depth > 0 ? '└ ' : ''}{node.name}</span>
                <button title={node.locked ? '解锁' : '锁定'} onClick={(event) => { event.stopPropagation(); void updateSceneNodeById(node.id, [{ path: ['locked'], value: !node.locked }]); }}>{node.locked ? '🔒' : '⌁'}</button>
              </div>) : <div className="scene-sidebar-empty"><strong>等待 AI 建立 Scene</strong><span>先规划页面和视觉方向，再生成第一个有界步骤。</span></div>}
            </div>

            <div className="panel-title section-title layer-title"><span>画板目录</span><small>{pages.length}</small></div>
            <div className="artboard-directory" role="list" aria-label="画板目录">{pages.map((page, index) => {
              const artboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === page.id);
              return <button key={page.id} role="listitem" className={page.id === currentPage?.id ? 'active' : ''} onClick={() => switchPage(page.id)}>
                <span>{index + 1}</span><div><strong>{page.name}</strong><small>{WORKSPACE_SURFACE_LABELS[page.surfaceKind ?? 'page']}{artboard ? ` · ${artboard.viewportWidth} × ${artboard.viewportHeight}` : ''}</small></div><em>{page.id === currentPage?.id ? '正在编辑' : '打开'}</em>
              </button>;
            })}</div>
            {sceneDocument ? <div className="page-actions"><button onClick={addPage}>＋ 新建设计面</button><button onClick={duplicatePage}>复制画板</button><button disabled={pages.length <= 1} onClick={deleteCurrentPage}>删除</button></div> : <p className="helper-text">页面清单由 AI 先写入 Plan；开始当前页后才建立可编辑 Scene 画板。</p>}
            {currentPage && <><label className="field-label">画板类型<select value={currentPage.surfaceKind ?? 'page'} onChange={(event) => updateCurrentPage({ surfaceKind: event.target.value as WorkspaceSurfaceKind })}>{(Object.keys(WORKSPACE_SURFACE_LABELS) as WorkspaceSurfaceKind[]).map((kind) => <option key={kind} value={kind}>{WORKSPACE_SURFACE_LABELS[kind]}</option>)}</select></label>{sceneDocument
              ? <><label className="field-label">设计面名称<input key={`${currentPage.id}:${currentPage.name}`} defaultValue={currentPage.name} onBlur={(event) => updateCurrentPage({ name: event.currentTarget.value })} /></label><label className="field-label page-slug">稳定页面 ID<input value={currentPage.id} readOnly /></label></>
              : <><label className="field-label">设计面名称<input value={currentPage.name} readOnly /></label><label className="field-label page-slug">等待 Scene 页面 ID<input value="由 AI Plan 创建" readOnly /></label></>}</>}
            <div className="panel-title section-title">视口与页面</div>
            {sceneDocument && activeWorkspaceArtboard ? <>
              <div className="size-row"><SceneNumberField label="当前宽度" value={activeWorkspaceArtboard.viewportWidth} min={240} max={10000} onCommit={updateCustomViewportWidth} /><SceneNumberField label="最小高度" value={activeWorkspaceArtboard.viewportHeight} min={240} max={50000} onCommit={updateCustomViewportHeight} /></div>
              <p className="helper-text viewport-helper">尺寸不锁定：宽度可随时修改，内容超过最小高度时画板自动向下增长。画板类型不限制尺寸。</p>
            </> : <div className="scene-sidebar-empty"><strong>等待 Scene 画板</strong><span>画板尺寸会在 AI 开始当前页时建立，内容边界随后由真实 Scene 节点自动增长。</span></div>}

            <div className="panel-title section-title layer-title"><span>图片资源</span><small>{document.assets?.length ?? 0}</small></div>
            <input ref={assetInput} className="asset-input" type="file" accept="image/*" multiple onChange={(event) => void importAssets(event.target.files).catch((error) => showToast(String(error)))} />
            <button className="secondary-button" onClick={() => assetInput.current?.click()}>导入图片</button>
            <div className="asset-grid">{(document.assets ?? []).map((asset) => <button key={asset.id} title={`使用 ${asset.name}`} onClick={() => useAsset(asset)}><img src={asset.dataUrl} alt={asset.name} /><span>{asset.name}</span></button>)}</div>
          </>}

          {workspaceShell.activeArea === 'variables' && <div className="workspace-sidebar-section variables-sidebar">
            <div className="panel-intro"><strong>Scene Variables</strong><span>这里编辑的就是 Scene v2 Variable Collections 与 Modes，不再维护另一份旧 token 数据。</span></div>
            {sceneDocument ? <>
              <div className="panel-title layer-title"><span>变量集合</span><small>{sceneDocument.variableCollections.length}</small></div>
              <div className="scene-variable-summary">{sceneDocument.variableCollections.map((collection) => <article key={collection.id}><strong>{collection.name}</strong><span>{collection.modes.length} 个模式 · {collection.variables.length} 个变量</span></article>)}</div>
              <label className="field-label">完整变量数据<textarea className="scene-variable-editor" rows={16} spellCheck={false} value={sceneVariablesDraft} onChange={(event) => setSceneVariablesDraft(event.target.value)} /></label>
              <button className="secondary-button" onClick={() => void applySceneVariablesDraft()}>应用 Scene 变量</button>
              <button className="quiet-button variables-theme-button" onClick={() => setThemePickerOpen(true)}>从视觉风格建立变量</button>
            </> : <div className="scene-sidebar-empty"><strong>还没有 Scene 变量</strong><span>AI 开始第一个页面后，可在这里管理颜色、排版、圆角和响应式模式。</span></div>}
          </div>}

          {workspaceShell.activeArea === 'ai' && <div className="workspace-sidebar-section ai-tasks-sidebar">
            <div className="panel-intro"><strong>AI 视觉设计</strong><span>计划、候选截图、视觉 Diff 与人工批注都绑定 Scene 稳定节点；AI 一次只推进一个有界步骤。</span></div>
            <div className="scene-ai-sidebar-progress">{renderGenerationReviewPanel()}</div>
            <div className="panel-title layer-title"><span>待处理视觉批注</span><small>{sceneAnnotationTasks.length}</small></div>
            <div className="ai-sidebar-request-list">{sceneAnnotationTasks.map(({ node, annotation }: any) => <article key={annotation.id}><strong>{node.name}</strong><p>{annotation.body}</p><small>Scene r{sceneDocument?.revision} · {new Date(annotation.createdAt).toLocaleString()}</small><button disabled={sceneAnnotationPreparingId === annotation.id} onClick={() => void prepareSceneAnnotation(node.id, annotation.id)}>准备视觉上下文</button></article>)}</div>
            {sceneAnnotationTasks.length === 0 && <div className="ai-sidebar-empty"><span>✓</span><strong>没有待处理视觉批注</strong><p>{sceneDocument ? '选择图层后写下具体的构图、层级、留白、字体或图片问题。' : '先让 AI 规划一个页面并开始第一个视觉步骤。'}</p></div>}
            <div className="panel-title section-title">给 AI 一个小任务</div>
            <div className="ai-quick-prompts sidebar-prompts">{aiQuickPrompts.map((prompt: string) => <button key={prompt} onClick={() => setAiInstruction(prompt)}>{prompt}</button>)}</div>
            <textarea className="composer" rows={5} value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} placeholder={selectedSceneNode ? `描述“${selectedSceneNode.name}”需要改好的视觉问题…` : sceneAiTarget ? `描述“${activeScenePage?.name ?? '当前页面'}”需要改好的整体视觉问题…` : 'AI 建立 Scene 后，可在这里对页面或具体图层发起视觉任务。'} />
            <button className="ai-button" disabled={!aiInstruction.trim() || !sceneAiTarget || Boolean(sceneAnnotationPreparingId)} onClick={() => void submitSceneAiInstruction()}>{sceneAnnotationPreparingId ? '正在生成截图…' : '提交视觉任务'}</button>
          </div>}
        </div>
        <WorkspacePanelResizeHandle side="left" width={workspaceShell.leftPanelWidth} onResize={(width) => dispatchWorkspaceShell({ type: 'resize-left-panel', width })} />
      </aside>}

      <section ref={canvasStage} className="canvas-stage">
        <div className="canvas-toolbar device-toolbar" role="toolbar" aria-label={editingSlot ? '容器内部编辑工具' : '当前画板工具'}>
          {editingSlot && editingContainer && editingSlotDefinition ? <>
            <button className="slot-editor-back" onClick={exitSlotEditor}>‹ 返回页面</button>
            <span className="slot-editor-path"><b>{editingContainer.library?.component}</b><i>/</i>{editingSlotDefinition.label}</span>
            <span className="toolbar-divider" />
            <button title="缩小内部画布" onClick={() => setCanvasZoom(zoom / 1.2)}>−</button>
            <span className="zoom-value">{Math.round(zoom * 100)}%</span>
            <button title="放大内部画布" onClick={() => setCanvasZoom(zoom * 1.2)}>＋</button>
            <span className="toolbar-divider" />
            <button className="fit-button" disabled={selectedIds.length === 0} title="将内部选中的一个或多个组件放到可见区域中心" onClick={fitWorkspaceSelection}>适应选择</button>
            <button className="fit-button" title="完整显示当前可编辑内容区域" onClick={fitSlotEditorContent}>适应内容</button>
            <button className="fit-button" title="恢复内部画布为 100%" onClick={() => setCanvasZoom(1)}>100%</button>
            <span className="toolbar-divider" />
            <button className="fit-button" onClick={() => insertSlotTemplate('form')}>＋ 表单模板</button>
            <button className="fit-button" onClick={() => insertSlotTemplate('details')}>＋ 详情模板</button>
          </> : <>
            {activeWorkspaceArtboard && <div className="toolbar-artboard-dimensions" aria-label="当前画板可编辑尺寸">
              <label><span>布局宽度</span><input key={`w:${activeWorkspaceArtboard.artboardId}:${activeWorkspaceArtboard.viewportWidth}`} aria-label="当前画板布局宽度" type="number" min={240} max={10000} defaultValue={activeWorkspaceArtboard.viewportWidth} onBlur={(event) => updateCustomViewportWidth(Number(event.currentTarget.value))} onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }} /></label>
              <span>×</span>
              <output title="画板高度由当前画板内所有可见内容的最底边自动计算">高度自动 {Math.round(renderedCanvasHeight)}</output>
            </div>}
            <span className="toolbar-divider" />
            <select className="viewport-preset-select" aria-label="套用画板尺寸模板" value="" onChange={(event) => { if (event.target.value) void selectViewportPreset(event.target.value); }}>
              <option value="">套用尺寸模板…</option>
              <optgroup label="常用画板尺寸">{viewportPresets.filter((preset) => !preset.group).map((preset) => <option key={preset.id} value={preset.id}>{preset.label} · {preset.width} × {preset.height}</option>)}</optgroup>
              {viewportPresets.some((preset) => Boolean(preset.group)) && <optgroup label="大型画板尺寸">{viewportPresets.filter((preset) => Boolean(preset.group)).map((preset) => <option key={preset.id} value={preset.id}>{preset.label} · {preset.width} × {preset.height}</option>)}</optgroup>}
            </select>
            <button className="fit-button responsive-generate-button" disabled={!sceneDocument} title="编辑当前画板内部的自适应布局、约束和宽度规则" onClick={() => { activateWorkspaceArea('layers'); showToast(`正在编辑“${currentPage?.name ?? '当前画板'}”的布局规则`); }}>布局规则</button>
            <span className="toolbar-divider" />
            <button className="fit-button" disabled={selectedIds.length === 0} title="将当前选中的一个或多个组件放到可见区域中心" onClick={fitWorkspaceSelection}>适应选择</button>
            <button className="fit-button" title="完整显示正在编辑的当前画板" onClick={fitActiveWorkspaceArtboard}>聚焦当前画板</button>
            <button className="fit-button" title="复制正在编辑的当前画板" onClick={duplicatePage}>复制画板</button>
            <span className="toolbar-divider" /><button className={`fit-button interaction-mode-button ${interactionMode ? 'active' : ''}`} title="操作当前画板里的输入框、选择器、抽屉和标签页" onClick={toggleInteractionMode}>{interactionMode ? '退出交互' : '交互当前画板'}</button>
          </>}
        </div>
        {interactionMode && <div className="interaction-mode-banner"><span>●</span> 交互模式：可以输入、选择、展开和打开弹层；退出后继续拖动编辑</div>}
        {selected && <div className="selection-toolbar">
          <button title="左对齐" onClick={() => alignSelected('left')}>⇤</button><button title="水平居中" onClick={() => alignSelected('center')}>↔</button><button title="右对齐" onClick={() => alignSelected('right')}>⇥</button>
          <button title="顶部对齐" onClick={() => alignSelected('top')}>↥</button><button title="垂直居中" onClick={() => alignSelected('middle')}>↕</button><button title="底部对齐" onClick={() => alignSelected('bottom')}>↧</button>
          <span /><button title="置于顶层" onClick={() => reorderSelected('front')}>⤒</button><button title="上移一层" onClick={() => reorderSelected('forward')}>↑</button><button title="下移一层" onClick={() => reorderSelected('backward')}>↓</button><button title="置于底层" onClick={() => reorderSelected('back')}>⤓</button>
          <span /><button title="复制 ⌘C" onClick={copySelected}>⧉</button><button title="粘贴 ⌘V" disabled={!clipboard} onClick={pasteClipboard}>▣</button>
          {selectedIds.length > 1 && <><span /><button className="wide-tool" title="创建可整体移动的分组 ⌘G" onClick={groupSelected}>创建分组</button></>}
          {canUngroup && <button className="wide-tool" title="取消当前分组 ⇧⌘G" onClick={ungroupSelected}>取消分组</button>}
        </div>}
        {selectedSceneNode && <div className="selection-toolbar scene-selection-toolbar">
          <span className="scene-selection-kind">{selectedSceneNode.type}</span>
          <button title="复制 Scene 图层 ⌘D" onClick={duplicateSceneSelection}>⧉</button>
          <button title="复制到剪贴板 ⌘C" onClick={copySceneSelection}>C</button>
          <button title="从剪贴板粘贴 ⌘V" disabled={sceneClipboard.length === 0} onClick={pasteSceneClipboard}>V</button>
          {selectedIds.length > 1 && <>
            <span />
            <button title="左对齐" onClick={() => void alignSceneSelection('left')}>⇤</button>
            <button title="水平居中对齐" onClick={() => void alignSceneSelection('horizontal-center')}>↔</button>
            <button title="右对齐" onClick={() => void alignSceneSelection('right')}>⇥</button>
            <button title="顶部对齐" onClick={() => void alignSceneSelection('top')}>↥</button>
            <button title="垂直居中对齐" onClick={() => void alignSceneSelection('vertical-center')}>↕</button>
            <button title="底部对齐" onClick={() => void alignSceneSelection('bottom')}>↧</button>
          </>}
          {selectedIds.length > 2 && <>
            <button className="wide-tool" title="水平等间距分布" onClick={() => void distributeSceneSelection('horizontal')}>水平分布</button>
            <button className="wide-tool" title="垂直等间距分布" onClick={() => void distributeSceneSelection('vertical')}>垂直分布</button>
          </>}
          <span />
          <button title="置于顶层" onClick={() => void reorderSceneSelection('front')}>⤒</button>
          <button title="上移一层" onClick={() => void reorderSceneSelection('forward')}>↑</button>
          <button title="下移一层" onClick={() => void reorderSceneSelection('backward')}>↓</button>
          <button title="置于底层" onClick={() => void reorderSceneSelection('back')}>⤓</button>
          {selectedIds.length > 1 && <>
            <span />
            <button className="wide-tool" title="把选中图层组成可整体移动的 Group" onClick={() => void wrapSceneSelection('group')}>Group</button>
            <button className="wide-tool" title="用带内边距的 Frame 包住选中图层" onClick={() => void wrapSceneSelection('frame')}>Frame</button>
            <button title="横向 Auto Layout" onClick={() => void wrapSceneSelection('auto-horizontal')}>⇥</button>
            <button title="纵向 Auto Layout" onClick={() => void wrapSceneSelection('auto-vertical')}>⇣</button>
          </>}
          {(selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame') && selectedSceneNode.layout.mode === 'free'
            && <button className="wide-tool" title="取消当前容器" onClick={() => void ungroupSceneSelection()}>取消容器</button>}
          <button title="删除选中图层" onClick={() => void deleteSceneSelection()}>⌫</button>
        </div>}
        {selectionCandidatePopover && <div className="selection-candidate-popover" style={{
          left: Math.max(8, Math.min(window.innerWidth - 248, selectionCandidatePopover.clientX + 12)),
          top: Math.max(8, Math.min(window.innerHeight - 300, selectionCandidatePopover.clientY + 12))
        }} onPointerDown={(event) => event.stopPropagation()}>
          <header><strong>选择这个位置的图层</strong><span>{selectionCandidatePopover.candidates.length} 个重叠对象</span></header>
          <div>{selectionCandidatePopover.candidates.map((candidate, index) => <button key={candidate.id} onClick={() => {
            selectComponent(candidate.id);
            setSelectionCandidatePopover(undefined);
          }}><span>{index + 1}</span><div><strong>{candidate.name}</strong><small>{candidate.type}{candidate.depth > 0 ? ` · 第 ${candidate.depth + 1} 层` : ' · 外层'}</small></div>{candidate.locked && <em>已锁定</em>}</button>)}</div>
          <footer>Command / Ctrl 点击可再次查看候选</footer>
        </div>}
        {!editingSlot && <nav className="artboard-directory-bar" aria-label="画板目录">
          <div className="artboard-directory-heading"><strong>画板目录</strong><span>{pages.length}</span></div>
          <div className="artboard-directory-scroll" role="tablist" aria-label="项目画板">
            {pages.map((page, index) => <button
              key={page.id}
              type="button"
              role="tab"
              data-page-id={page.id}
              className={`artboard-directory-item ${page.id === currentPage?.id ? 'active' : ''}`}
              aria-selected={page.id === currentPage?.id}
              onClick={() => switchPage(page.id)}
            >
              <b>{index + 1}</b>
              <span><strong>{page.name}</strong><small>{WORKSPACE_SURFACE_LABELS[page.surfaceKind ?? 'page']}</small></span>
            </button>)}
          </div>
          <button className="artboard-directory-add" type="button" disabled={!sceneDocument} onClick={() => void addWorkspaceSurface()} title="新建画板">＋</button>
        </nav>}
        <div
          ref={canvasScroll}
          className={`canvas-scroll workspace-camera-viewport ${editingSlot ? 'slot-editor-scroll' : ''} ${canvasPanReady || workspaceShell.activeTool === 'hand' ? 'pan-ready' : ''} ${canvasPanning ? 'panning' : ''}`}
          style={{
            '--workspace-grid-size': `${16 * zoom}px`,
            '--workspace-grid-x': `${workspaceCamera.x}px`,
            '--workspace-grid-y': `${workspaceCamera.y}px`
          } as CSSProperties}
          onPointerDown={beginCanvasPan}
        >
        {editingSlot && editingContainer && editingSlotDefinition && editingSlotCanvasSize ? <div className="slot-editor-camera-world" style={{ transform: `translate3d(${workspaceCamera.x}px,${workspaceCamera.y}px,0) scale(${zoom})` }}><div className="slot-editor-frame">
            <div className="slot-editor-heading"><div><span>可编辑内容区域</span><strong>{editingSlotDefinition.label}</strong><small>{editingSlotDefinition.description}</small></div><em>{Math.round(editingSlotCanvasSize.width)} × {Math.round(editingSlotCanvasSize.height)}</em></div>
            <div className="slot-editor-canvas-shell">
              <div className="slot-design-canvas design-canvas" style={{ width: editingSlotCanvasSize.width, height: editingSlotCanvasSize.height }} onDragOver={(event) => event.preventDefault()} onDrop={onCanvasDrop} onPointerDown={beginCanvasMarquee}>
                {editingSlotComponents.length === 0 && <div className="slot-empty-state"><span>＋</span><strong>从左侧拖入组件</strong><p>也可以先插入表单或详情模板，再逐项调整。</p><div><button onPointerDown={(event) => event.stopPropagation()} onClick={() => insertSlotTemplate('form')}>插入表单</button><button onPointerDown={(event) => event.stopPropagation()} onClick={() => insertSlotTemplate('details')}>插入详情</button></div></div>}
                {editingVisibleComponents.sort((left, right) => left.zIndex - right.zIndex).map((component) => {
                  const frame = resolveComponent(component, device);
                  const containerFrame = resolveComponent(editingContainer, device);
                  const resolved = { ...frame, x: frame.x - containerFrame.x, y: frame.y - containerFrame.y };
                  if (resolved.hidden) return null;
                  const editableSlot = editableSlotsForUiComponent(component)[0];
                  return <WorkspaceCanvasComponent key={component.id} component={component} resolved={resolved} selected={selectedIdSet.has(component.id)} primary={component.id === selectedId} interactive={false} forcedState={component.id === selectedId && inspectorVisualState !== 'default' ? inspectorVisualState : undefined} tokens={tokens} slotContent={runtimeSlotContentMap(document, component, device, false, tokens, activatePreviewInteraction)} onPointerDown={(event) => beginInteraction(event, component, 'move')} onResizePointerDown={(event) => beginInteraction(event, component, 'resize')} onPreviewActivate={() => activatePreviewInteraction(component)} onEditContents={editableSlot ? () => void editComponentSlot(component, editableSlot.id) : undefined} />;
                })}
                <SelectionOverlay items={selectionOverlayItemsFor(editingVisibleComponents, device, resolveComponent(editingContainer, device))} marqueeRect={marqueeRect} onResizePointerDown={(componentId, _handle, event) => {
                  const component = document.components.find((candidate) => candidate.id === componentId);
                  if (component) beginInteraction(event, component, 'resize');
                }} />
              </div>
            </div>
          </div></div> : <div className="workspace-camera-world" data-canvas-mode="single-artboard" style={{ transform: `translate3d(${workspaceCamera.x}px,${workspaceCamera.y}px,0) scale(${zoom})` }}>
            {activeWorkspaceArtboard ? renderWorkspaceArtboard(activeWorkspaceArtboard) : null}
          </div>}
        </div>
        {interactionMode && renderPreviewSurfaceOverlay()}
        <WorkspaceBottomToolbar
          activeTool={workspaceShell.activeTool}
          leftPanelOpen={workspaceShell.leftPanelOpen}
          rightPanelOpen={workspaceShell.rightPanelOpen}
          canvasMaximized={workspaceShell.canvasMaximized}
          workspaceActions={<>
            <div className="workspace-new-artboard-control">
              <select aria-label="新画板类型" value={newSurfaceKind} onChange={(event) => setNewSurfaceKind(event.target.value as WorkspaceSurfaceKind)}>
                {(Object.keys(WORKSPACE_SURFACE_LABELS) as WorkspaceSurfaceKind[]).map((kind) => <option key={kind} value={kind}>{WORKSPACE_SURFACE_LABELS[kind]}</option>)}
              </select>
              <button disabled={!sceneDocument} title="在工作区新建独立设计面" onClick={() => void addWorkspaceSurface()}><b>＋</b><em>画板</em></button>
            </div>
            <button title="缩小当前画板" onClick={() => setCanvasZoom(zoom / 1.2)}><b>−</b></button>
            <output className="workspace-zoom-value" aria-label={`当前画板缩放 ${Math.round(zoom * 100)}%`}>{Math.round(zoom * 100)}%</output>
            <button title="放大当前画板" onClick={() => setCanvasZoom(zoom * 1.2)}><b>＋</b></button>
            <button title="当前画板恢复为 100%" onClick={() => setCanvasZoom(1)}><b>1:1</b></button>
          </>}
          onSelectTool={activateWorkspaceTool}
          onToggleLeft={() => dispatchWorkspaceShell({ type: 'toggle-left-panel' })}
          onToggleRight={() => dispatchWorkspaceShell({ type: 'toggle-right-panel' })}
          onToggleMaximize={() => dispatchWorkspaceShell({ type: 'toggle-canvas-maximized' })}
        />
      </section>

      {workspaceShell.rightPanelOpen && <aside className="inspector-panel">
        <WorkspacePanelResizeHandle side="right" width={workspaceShell.rightPanelWidth} onResize={(width) => dispatchWorkspaceShell({ type: 'resize-right-panel', width })} />
        <div className="inspector-review-switch">
          <button className={inspectorTab === 'review' ? 'active' : ''} onClick={() => setInspectorTab(inspectorTab === 'review' ? 'design' : 'review')}><span>✦</span><strong>AI 设计进度</strong>{generationPlan?.activeStep?.status === 'awaiting-review' && <em>待审阅</em>}</button>
        </div>
        {inspectorTab === 'review' ? renderGenerationReviewPanel() : selectedSceneNode ? <>
          <div className="inspector-heading"><div><span className="eyebrow">Scene v2 · {selectedIds.length > 1 ? `${selectedIds.length} 项` : selectedSceneNode.type}</span><strong>{selectedSceneNode.name}</strong></div><span className="scene-revision-badge">r{sceneDocument?.revision}</span></div>
          <div className="inspector-actions">
            <button onClick={duplicateSceneSelection}>复制 ⌘D</button>
            <button onClick={saveSceneSelectionAsSnippet}>保存到“我的”</button>
            <button className={selectedSceneNode.locked ? 'active' : ''} onClick={() => void updateSceneNode([{ path: ['locked'], value: !selectedSceneNode.locked }])}>{selectedSceneNode.locked ? '解锁' : '锁定'}</button>
            <button className={!selectedSceneNode.visible ? 'active' : ''} onClick={() => void updateSceneNode([{ path: ['visible'], value: !selectedSceneNode.visible }])}>{selectedSceneNode.visible ? '隐藏' : '显示'}</button>
            {(selectedSceneNode.type === 'group' || selectedSceneNode.type === 'frame') && selectedSceneNode.layout.mode === 'free' && <button onClick={() => void ungroupSceneSelection()}>取消容器</button>}
          </div>
          <div className="inspector-mode-tabs scene-inspector-tabs" role="tablist" aria-label="Scene 属性栏模式">
            <button role="tab" aria-selected={inspectorTab === 'design'} className={inspectorTab === 'design' ? 'active' : ''} onClick={() => setInspectorTab('design')}>设计</button>
            <button role="tab" aria-selected={inspectorTab === 'prototype'} className={inspectorTab === 'prototype' ? 'active' : ''} onClick={() => setInspectorTab('prototype')}>原型</button>
            <button role="tab" aria-selected={inspectorTab === 'ai'} className={inspectorTab === 'ai' ? 'active' : ''} onClick={() => setInspectorTab('ai')}>批注与 AI</button>
          </div>
          {inspectorTab === 'design' && <>
          <div className="panel-title section-title">图层</div>
          <label className="field-label">名称<input key={`${selectedSceneNode.id}:${selectedSceneNode.name}`} defaultValue={selectedSceneNode.name} maxLength={240} onBlur={(event) => {
            const name = event.currentTarget.value.trim();
            if (name && name !== selectedSceneNode.name) void updateSceneNode([{ path: ['name'], value: name }]);
          }} /></label>
          {selectedSceneNode.type === 'text' && <label className="field-label">文字内容<textarea key={`${selectedSceneNode.id}:${selectedSceneNode.content}`} rows={4} defaultValue={selectedSceneNode.content} onBlur={(event) => {
            if (event.currentTarget.value !== selectedSceneNode.content) void updateSceneNode([{ path: ['content'], value: event.currentTarget.value }]);
          }} /></label>}
          {selectedSceneNode.type === 'library-instance' && <section className="scene-library-inspector">
            <header><div><span>官方组件</span><strong>{selectedSceneLibrary?.displayName ?? selectedSceneNode.library} · {selectedSceneLibraryDefinition?.label ?? selectedSceneNode.component}</strong></div><em>{selectedSceneLibrary?.version ?? 'runtime'}</em></header>
            <div className="scene-library-binding-grid"><label className="field-label">组件库<input value={selectedSceneNode.library} readOnly /></label><label className="field-label">组件<input value={selectedSceneNode.component} readOnly /></label></div>
            <label className="field-label">官方款式<select value={selectedSceneNode.variant ?? selectedSceneLibraryVariants[0]?.id ?? 'default'} disabled={selectedSceneNode.locked || selectedSceneLibraryVariants.length === 0} onChange={(event) => void applySelectedSceneLibraryVariant(event.target.value)}>{selectedSceneLibraryVariants.map((variant: any) => <option key={variant.id} value={variant.id}>{variant.label}</option>)}</select></label>
            <label className="field-label">展示内容<textarea key={`${selectedSceneNode.id}:${selectedSceneNode.content ?? ''}`} rows={3} defaultValue={selectedSceneNode.content ?? ''} disabled={selectedSceneNode.locked} onBlur={(event) => {
              if (event.currentTarget.value !== (selectedSceneNode.content ?? '')) void updateSceneNode([{ path: ['content'], value: event.currentTarget.value }], '用户修改 Scene 官方组件内容。');
            }} /></label>
            <div className="scene-library-runtime-actions"><button disabled={selectedSceneNode.locked || !selectedSceneLibrary} onClick={() => selectedSceneLibrary && setVariantPickerTarget({ library: selectedSceneLibrary.id, componentId: selectedSceneNode.component, replaceComponentId: selectedSceneNode.id })}>{selectedSceneRegistryElement ? '重新选择官方元素' : '浏览官方示例与元素'}</button>{selectedSceneRegistryElement && <span>{selectedSceneRegistryElement.label}</span>}</div>
            <JsonObjectEditor label="组件属性 / 示例数据" value={selectedSceneNode.properties} disabled={selectedSceneNode.locked} onChange={(value) => void updateSceneNode([{ path: ['properties'], value }], '用户修改 Scene 官方组件属性和示例数据。')} />
            {selectedSceneEditableSlots.length > 0 && <div className="scene-content-slots"><div className="panel-title section-title">内部内容区</div><p className="helper-text">进入内容区后，左侧拖入或点击插入的官方组件会成为当前组件的真实 Scene 子层，不会生成另一套编辑器数据。</p>{selectedSceneEditableSlots.map((slot: any) => {
              const activeSlot = Boolean(sceneContentFocus && sceneContentFocus.pageId === selectedSceneEntry?.pageId && sceneContentFocus.nodeId === selectedSceneNode.id && sceneContentFocus.slot === slot.id);
              return <button key={slot.id} className={activeSlot ? 'active' : ''} onClick={() => activeSlot ? setSceneContentFocus(undefined) : focusSceneContent(selectedSceneNode.id, slot.id)}><span><strong>{slot.label}</strong><small>{slot.description}</small></span><em>{selectedSceneNode.slots[slot.id]?.length ?? 0} 层</em><b>{activeSlot ? '退出' : '进入编辑'}</b></button>;
            })}</div>}
          </section>}
          {isSceneContainer(selectedSceneNode) && selectedSceneNode.type !== 'component-set' && <section className="scene-container-focus-card"><div><strong>容器内部编辑</strong><span>把后续组件直接放入这个 {selectedSceneNode.type === 'group' ? 'Group' : 'Frame'}</span></div><button className={sceneContentFocus?.nodeId === selectedSceneNode.id && !sceneContentFocus.slot ? 'active' : ''} onClick={() => sceneContentFocus?.nodeId === selectedSceneNode.id && !sceneContentFocus.slot ? setSceneContentFocus(undefined) : focusSceneContent(selectedSceneNode.id)}>{sceneContentFocus?.nodeId === selectedSceneNode.id && !sceneContentFocus.slot ? '退出内部编辑' : '进入内部编辑'}</button></section>}
          <div className="size-row four">
            <SceneNumberField label="X" value={selectedSceneNode.frame.x} disabled={!selectedScenePositionEditable || selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'x'], value }])} />
            <SceneNumberField label="Y" value={selectedSceneNode.frame.y} disabled={!selectedScenePositionEditable || selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'y'], value }])} />
            <SceneNumberField label="W" value={selectedSceneNode.frame.width} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'width'], value }])} />
            <SceneNumberField label="H" value={selectedSceneNode.frame.height} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['frame', 'height'], value }])} />
          </div>
          {!selectedScenePositionEditable && <p className="helper-text">这个图层由父级 Auto Layout / Grid 排布，X、Y 位置由布局计算。</p>}
          <div className="panel-title section-title">布局</div>
          <label className="field-label">布局方式<select value={selectedSceneNode.layout.mode} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'mode'], value: event.target.value }])}><option value="free">自由布局</option><option value="auto">Auto Layout</option><option value="grid">Grid</option></select></label>
          {selectedSceneNode.layout.mode === 'auto' && <label className="field-label">方向<select value={selectedSceneNode.layout.direction ?? 'vertical'} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'direction'], value: event.target.value }])}><option value="horizontal">横向</option><option value="vertical">纵向</option></select></label>}
          <div className="size-row">
            <label className="field-label">水平尺寸<select value={selectedSceneNode.layout.sizingX} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'sizingX'], value: event.target.value }])}><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满</option></select></label>
            <label className="field-label">垂直尺寸<select value={selectedSceneNode.layout.sizingY} disabled={selectedSceneNode.locked} onChange={(event) => void updateSceneNode([{ path: ['layout', 'sizingY'], value: event.target.value }])}><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满</option></select></label>
          </div>
          <div className="size-row four">
            <SceneNumberField label="上" value={selectedSceneNode.layout.padding.top} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'top'], value }])} />
            <SceneNumberField label="右" value={selectedSceneNode.layout.padding.right} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'right'], value }])} />
            <SceneNumberField label="下" value={selectedSceneNode.layout.padding.bottom} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'bottom'], value }])} />
            <SceneNumberField label="左" value={selectedSceneNode.layout.padding.left} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'padding', 'left'], value }])} />
          </div>
          <div className="size-row"><SceneNumberField label="行间距" value={selectedSceneNode.layout.gap.row} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'gap', 'row'], value }])} /><SceneNumberField label="列间距" value={selectedSceneNode.layout.gap.column} min={0} disabled={selectedSceneNode.locked} onCommit={(value) => void updateSceneNode([{ path: ['layout', 'gap', 'column'], value }])} /></div>
          {sceneResponsiveRuleSpec && <section className="scene-responsive-editor">
            <header><div><strong>{device === 'mobile' ? '窄宽度' : '中等宽度'}布局覆盖</strong><span>仍使用当前画板的同一棵 Scene，不复制组件</span></div><button disabled={!selectedSceneResponsiveOverride} onClick={() => void clearSelectedSceneResponsiveOverride()}>恢复继承</button></header>
            <label className="field-label">此断点可见性<select value={selectedSceneResponsiveOverride?.visible === undefined ? 'inherit' : selectedSceneResponsiveOverride.visible ? 'visible' : 'hidden'} disabled={selectedSceneNode.locked} onChange={(event) => {
              const value = event.target.value;
              if (value === 'inherit') {
                const next: Omit<SceneResponsiveNodeOverride, 'nodeId'> = structuredClone(selectedSceneResponsiveOverride ?? {});
                delete next.visible;
                void replaceSelectedSceneResponsiveOverride(next);
              } else void updateSelectedSceneResponsiveOverride({ visible: value === 'visible' });
            }}><option value="inherit">继承基础设计</option><option value="visible">强制显示</option><option value="hidden">在此断点隐藏</option></select></label>
            <div className="size-row">
              <label className="field-label">水平尺寸<select value={selectedSceneResponsiveOverride?.layout?.sizingX ?? 'inherit'} disabled={selectedSceneNode.locked} onChange={(event) => {
                const layout = structuredClone(selectedSceneResponsiveOverride?.layout ?? {});
                if (event.target.value === 'inherit') delete layout.sizingX;
                else layout.sizingX = event.target.value as 'fixed' | 'hug' | 'fill';
                const next = { ...structuredClone(selectedSceneResponsiveOverride ?? {}), ...(Object.keys(layout).length ? { layout } : {}) };
                if (!Object.keys(layout).length) delete next.layout;
                void replaceSelectedSceneResponsiveOverride(next);
              }}><option value="inherit">继承</option><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满可用宽度</option></select></label>
              <label className="field-label">垂直尺寸<select value={selectedSceneResponsiveOverride?.layout?.sizingY ?? 'inherit'} disabled={selectedSceneNode.locked} onChange={(event) => {
                const layout = structuredClone(selectedSceneResponsiveOverride?.layout ?? {});
                if (event.target.value === 'inherit') delete layout.sizingY;
                else layout.sizingY = event.target.value as 'fixed' | 'hug' | 'fill';
                const next = { ...structuredClone(selectedSceneResponsiveOverride ?? {}), ...(Object.keys(layout).length ? { layout } : {}) };
                if (!Object.keys(layout).length) delete next.layout;
                void replaceSelectedSceneResponsiveOverride(next);
              }}><option value="inherit">继承</option><option value="fixed">固定</option><option value="hug">适应内容</option><option value="fill">填满可用高度</option></select></label>
            </div>
            {isSceneContainer(selectedSceneNode) && <>
              <label className="field-label">内容方向<select value={selectedSceneResponsiveOverride?.layout?.direction ?? 'inherit'} disabled={selectedSceneNode.locked} onChange={(event) => {
                const layout = structuredClone(selectedSceneResponsiveOverride?.layout ?? {});
                if (event.target.value === 'inherit') delete layout.direction;
                else layout.direction = event.target.value as 'horizontal' | 'vertical';
                const next = { ...structuredClone(selectedSceneResponsiveOverride ?? {}), ...(Object.keys(layout).length ? { layout } : {}) };
                if (!Object.keys(layout).length) delete next.layout;
                void replaceSelectedSceneResponsiveOverride(next);
              }}><option value="inherit">继承</option><option value="horizontal">横向</option><option value="vertical">纵向</option></select></label>
              {selectedSceneNode.children.length > 1 && <div className="scene-responsive-order"><strong>此断点子层顺序</strong><span>只改变排列顺序，不复制或删除图层</span>{(selectedSceneResponsiveOverride?.childOrder ?? selectedSceneNode.children.map((child) => child.id)).map((childId, index, order) => {
                const child = selectedSceneNode.children.find((candidate) => candidate.id === childId);
                return <div key={childId}><span>{child?.name ?? childId}</span><button disabled={index === 0 || selectedSceneNode.locked} onClick={() => {
                  const next = [...order];
                  [next[index - 1], next[index]] = [next[index], next[index - 1]];
                  void updateSelectedSceneResponsiveOverride({ childOrder: next });
                }}>↑</button><button disabled={index === order.length - 1 || selectedSceneNode.locked} onClick={() => {
                  const next = [...order];
                  [next[index], next[index + 1]] = [next[index + 1], next[index]];
                  void updateSelectedSceneResponsiveOverride({ childOrder: next });
                }}>↓</button></div>;
              })}</div>}
            </>}
          </section>}
          <section className="scene-ai-policy-card"><header><strong>AI 编辑策略</strong><span>{selectedSceneNode.aiPolicy.editable ? '允许 AI 修改' : '仅人工修改'}</span></header>{selectedSceneNode.aiPolicy.intent && <p>{selectedSceneNode.aiPolicy.intent}</p>}<small>{selectedSceneNode.aiPolicy.lockedFields.length ? `保护字段：${selectedSceneNode.aiPolicy.lockedFields.join('、')}` : '没有单独保护的字段'}</small></section>
          </>}
          {inspectorTab === 'prototype' && <>
            <div className="panel-title section-title">画板连接</div>
            {selectedPrototypeTarget && <div className="prototype-relationship-card">
              <div className="prototype-relationship-node"><span>来源</span><strong>{selectedSceneNode.name}</strong><small>{pages.find((page) => page.id === selectedSceneEntry?.pageId)?.name ?? selectedSceneEntry?.pageId}</small></div>
              <div className="prototype-relationship-action"><i>→</i><span>{selectedSceneNode.prototypeLink?.action === 'navigate' ? '跳转' : `打开${WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}`}</span></div>
              <div className="prototype-relationship-node target"><span>目标</span><strong>{selectedPrototypeTarget.name}</strong><small>{WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}</small></div>
              <div className="prototype-relationship-buttons"><button onClick={() => focusWorkspaceArtboardByPageId(selectedPrototypeTarget.id)}>定位目标画板</button><button onClick={() => void updateSceneNode([{ path: ['prototypeLink'], value: null }], '用户移除 Scene 原型关系。')}>移除关系</button></div>
            </div>}
            <label className="field-label">点击行为<select value={selectedSceneNode.prototypeLink ? 'page' : 'none'} disabled={selectedSceneNode.locked} onChange={(event) => {
              if (event.target.value === 'none') {
                void updateSceneNode([{ path: ['prototypeLink'], value: null }], '用户移除 Scene 原型关系。');
                return;
              }
              const target = pages.find((candidate) => candidate.id !== selectedSceneEntry?.pageId);
              if (!target) {
                showToast('请先创建另一个独立画板');
                return;
              }
              const action = (target.surfaceKind ?? 'page') === 'page' ? 'navigate' : 'overlay';
              void updateSceneNode([{ path: ['prototypeLink'], value: { trigger: 'click', action, targetPageId: target.id } }], '用户创建 Scene 原型关系。');
            }}><option value="none">无连接</option><option value="page">连接到独立画板</option></select></label>
            {selectedSceneNode.prototypeLink && <label className="field-label interaction-target">目标画板<select value={selectedSceneNode.prototypeLink.targetPageId} disabled={selectedSceneNode.locked} onChange={(event) => {
              const target = pages.find((candidate) => candidate.id === event.target.value);
              if (!target) return;
              const action = (target.surfaceKind ?? 'page') === 'page' ? 'navigate' : 'overlay';
              void updateSceneNode([{ path: ['prototypeLink'], value: { trigger: 'click', action, targetPageId: target.id } }], '用户修改 Scene 原型目标。');
            }}>{pages.filter((candidate) => candidate.id !== selectedSceneEntry?.pageId).map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.name} · {WORKSPACE_SURFACE_LABELS[candidate.surfaceKind ?? 'page']}</option>)}</select></label>}
            <p className="helper-text inspector-prototype-help">页面、弹窗、抽屉、菜单与状态都作为独立画板设计。普通页面执行跳转；其他画板在来源页面上叠加预览。工作区会显示这条关系的箭头。</p>
          </>}
          {inspectorTab === 'ai' && <div className="scene-annotation-panel">
            <section className="scene-ai-target-card">
              <header><strong>AI 视觉目标</strong><span>r{sceneDocument?.revision}</span></header>
              <p>{activeScenePage?.name ?? pageId} / {selectedSceneNode.name}</p>
              <small>节点 {selectedSceneNode.id}</small>
            </section>
            <div className="panel-title section-title">图层批注</div>
            <div className="notes-list scene-notes-list">
              {selectedSceneNode.annotations.length === 0 && <span className="empty-hint">还没有批注。写下视觉问题后，AI 会按这个稳定节点逐步修改。</span>}
              {selectedSceneNode.annotations.map((note) => <article key={note.id} className={`note-card scene-note-card ${note.status}`}>
                <span>{note.body}</span>
                <small>{note.status === 'open' ? '待 AI 处理' : '已完成'} · {note.author} · {note.id}</small>
                <div>
                  {note.status === 'open' ? <>
                    <button disabled={sceneAnnotationPreparingId === note.id} onClick={() => void prepareSceneAnnotation(selectedSceneNode.id, note.id)}>{sceneAnnotationPreparingId === note.id ? '正在生成截图…' : '准备给 AI'}</button>
                    <button onClick={() => void changeSceneAnnotationStatus(note.id, 'resolved')}>标记完成</button>
                  </> : <button onClick={() => void changeSceneAnnotationStatus(note.id, 'open')}>重新打开</button>}
                </div>
              </article>)}
            </div>
            <textarea className="composer" rows={3} maxLength={4000} placeholder="例如：标题与按钮的视觉层级不够明确，请加强对比但保留当前布局。" value={annotationText} onChange={(event) => setAnnotationText(event.target.value)} />
            <button className="secondary-button" disabled={!annotationText.trim()} onClick={() => void addSceneAnnotation()}>只添加批注</button>
            <div className="panel-title section-title">直接交给 AI</div>
            <p className="helper-text">提交时会自动创建批注，并截取当前节点的真实画面、Scene revision 与节点坐标，AI 不需要猜元素。</p>
            <textarea className="composer" rows={4} maxLength={4000} placeholder="描述你希望 AI 下一步改善的视觉效果…" value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} />
            <button className="ai-button" disabled={!aiInstruction.trim() || Boolean(sceneAnnotationPreparingId)} onClick={() => void submitSceneAiInstruction()}>{sceneAnnotationPreparingId ? '正在准备视觉上下文…' : '提交视觉任务'}</button>
            {sceneAiContext && <section className="scene-ai-context-card">
              {sceneAiContext.imageDataUrl && <img src={sceneAiContext.imageDataUrl} alt={`批注目标 ${selectedSceneNode.name}`} />}
              <div><strong>视觉上下文已就绪</strong><span>{sceneAiContext.capture.width} × {sceneAiContext.capture.height} · {sceneAiContext.capture.groundingCount} 个稳定节点</span></div>
              <dl><div><dt>页面</dt><dd>{sceneAiContext.task.pageId}</dd></div><div><dt>节点</dt><dd>{sceneAiContext.task.targetNodeId}</dd></div><div><dt>Scene</dt><dd>r{sceneAiContext.task.baseRevision}</dd></div><div><dt>截图</dt><dd>{sceneAiContext.capture.artifact.artifactId}</dd></div></dl>
            </section>}
          </div>}
        </> : selected && inspectedFrame ? <>
          <div className="inspector-heading"><div><span className="eyebrow">已选择 {selectedIds.length > 1 ? `${selectedIds.length} 项` : ''} · {device}</span><strong>{selected.name}</strong></div><button className="danger-link" onClick={deleteSelected}>删除</button></div>
          <div className="inspector-actions"><button onClick={duplicateSelected}>复制 ⌘D</button><button className={selected.locked ? 'active' : ''} onClick={() => toggleLocked(selected)}>{selected.locked ? '解锁' : '锁定'}</button><button className={inspectedFrame.hidden ? 'active' : ''} onClick={() => toggleHidden(selected)}>{inspectedFrame.hidden ? '显示' : '隐藏'}</button></div>
          <div className="inspector-mode-tabs" role="tablist" aria-label="属性栏模式">
            <button role="tab" aria-selected={inspectorTab === 'design'} className={inspectorTab === 'design' ? 'active' : ''} onClick={() => setInspectorTab('design')}>设计</button>
            <button role="tab" aria-selected={inspectorTab === 'prototype'} className={inspectorTab === 'prototype' ? 'active' : ''} onClick={() => setInspectorTab('prototype')}>原型</button>
            <button role="tab" aria-selected={inspectorTab === 'ai'} className={inspectorTab === 'ai' ? 'active' : ''} onClick={() => setInspectorTab('ai')}>批注与 AI</button>
          </div>
          {inspectorTab === 'design' && <>
          <button className="secondary-button save-symbol-button" onClick={saveSelectionAsSymbol}>保存到“我的”</button>
          {selectedSymbol && selected.symbolInstanceId && <div className="symbol-instance-panel">
            <div><span>实例来源</span><strong>{selectedSymbol.name}</strong></div>
            <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('content')} onChange={() => toggleSelectedSymbolOverride('content')} />保留内容</label>
            <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('style')} onChange={() => toggleSelectedSymbolOverride('style')} />保留样式</label>
            <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('frame')} onChange={() => toggleSelectedSymbolOverride('frame')} />保留位置尺寸</label>
            <div className="symbol-instance-actions"><button onClick={updateSelectedSymbolDefinition}>用当前实例更新定义</button><button onClick={synchronizeSelectedSymbol}>同步全部实例</button><button className="danger" onClick={detachSelectedSymbol}>脱离组件库</button></div>
          </div>}
          <label className="field-label">组件名称<input value={selected.name} onChange={(event) => updateSelected({ name: event.target.value })} /></label>
          {inspectorCapabilities?.content && <label className="field-label">{inspectorCapabilities.media ? '资源地址' : '内容'}<textarea rows={3} value={selected.content} onChange={(event) => updateSelected({ content: event.target.value })} /></label>}
          {inspectorCapabilities?.library && selected.library && selectedLibrary && <div className={`ui-library-inspector library-${selected.library.name}`}>
            <div className="panel-title section-title">{selectedLibrary.displayName} 组件</div>
            <div className="antd-binding-summary"><span>组件</span><strong>{selected.library.component}</strong><small>{selected.library.name === 'shadcn' ? selected.library.version : `v${selected.library.version}`}</small></div>
            {selectedLibraryDefinition?.docsUrl && <a className="ui-library-doc-link" href={selectedLibraryDefinition.docsUrl} target="_blank" rel="noreferrer">查看当前官网文档 ↗</a>}
            {selectedLibraryDefinition?.status === 'deprecated' && <div className="ui-library-deprecation-note">官网已将该组件标记为废弃；新设计建议使用 Listy。</div>}
            {selectedEditableSlots.length > 0 && <div className="content-slots-panel">
              <div className="content-slots-heading"><div><strong>内部内容</strong><span>像页面一样继续设计</span></div><em>{selectedEditableSlots.length} 个区域</em></div>
              {selectedEditableSlots.map((slot: any) => {
                const count = componentsInSlot(document, selected.id, slot.id).length;
                const officialDemo = Boolean(selected.library?.props.registryDemo);
                return <button key={slot.id} className={editingSlot?.componentId === selected.id && editingSlot.slotId === slot.id ? 'active' : ''} onClick={() => void editComponentSlot(selected, slot.id)}><span><strong>{slot.label}</strong><small>{slot.description}</small></span><em>{count > 0 ? `${count} 个组件` : officialDemo ? '尚未拆分' : '空白'}</em><b>{officialDemo && count === 0 ? '拆开并编辑 ›' : '进入编辑 ›'}</b></button>;
              })}
            </div>}
            {selectedRegistryElement ? <div className="selected-registry-element-summary"><div><span>已选择的独立元素</span><strong>{selectedRegistryElement.label}</strong><small>{selected.library.variant ? `来源款式：${selectedLibraryVariants.find((variant: any) => variant.id === selected.library?.variant)?.label ?? selected.library.variant}` : '保留官方真实运行时'}</small></div><button onClick={() => setVariantPickerTarget({ library: selected.library!.name, componentId: selected.library!.component, replaceComponentId: selected.id })}>重新选择</button></div>
              : <label className="field-label ui-library-variant-field">展现款式<select value={selected.library.variant ?? selectedLibraryVariants[0]?.id} onChange={(event) => applySelectedLibraryVariant(event.target.value)}>{selectedLibraryVariants.map((variant: any) => <option key={variant.id} value={variant.id}>{variant.label}</option>)}</select></label>}
            {selectedInspectableLibraryProps.filter(([, value]: [string, any]) => ['string', 'number', 'boolean'].includes(typeof value)).map(([key, value]: [string, any]) => typeof value === 'boolean'
              ? <label key={key} className="ui-library-boolean-prop"><input type="checkbox" checked={value} onChange={(event) => updateSelectedLibraryProp(key, event.target.checked)} /><span>{key}</span></label>
              : typeof value === 'number'
                ? <NumberField key={key} label={key} value={value} onChange={(next) => updateSelectedLibraryProp(key, next)} />
                : <label key={key} className="field-label">{key}<input value={String(value)} onChange={(event) => updateSelectedLibraryProp(key, event.target.value)} /></label>)}
            {selectedInspectableLibraryProps.some(([, value]: [string, any]) => value !== null && typeof value === 'object') && <div className="ui-library-data-editors"><div className="panel-title section-title">示例数据</div>{selectedInspectableLibraryProps.filter(([, value]: [string, any]) => value !== null && typeof value === 'object').map(([key, value]: [string, any]) => <JsonPropertyEditor key={key} label={key} value={value} onChange={(next) => updateSelectedLibraryProp(key, next)} />)}</div>}
          </div>}
          </>}
          {inspectorTab === 'prototype' && <>
          <div className="panel-title section-title">预览交互</div>
          {selectedPrototypeTarget && <div className="prototype-relationship-card">
            <div className="prototype-relationship-node"><span>来源</span><strong>{selected.name}</strong><small>{currentPage?.name ?? pageId}</small></div>
            <div className="prototype-relationship-action"><i>→</i><span>{(selectedPrototypeTarget.surfaceKind ?? 'page') === 'page' ? '跳转' : `打开${WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}`}</span></div>
            <div className="prototype-relationship-node target"><span>目标</span><strong>{selectedPrototypeTarget.name}</strong><small>{WORKSPACE_SURFACE_LABELS[selectedPrototypeTarget.surfaceKind ?? 'page']}</small></div>
            <div className="prototype-relationship-buttons"><button onClick={() => focusWorkspaceArtboardByPageId(selectedPrototypeTarget.id)}>定位目标画板</button><button onClick={() => updateSelected({ interaction: undefined })}>移除关系</button></div>
          </div>}
          <label className="field-label">点击行为<select value={selected.interaction?.type ?? 'none'} onChange={(event) => {
            const type = event.target.value;
            if (type === 'none') updateSelected({ interaction: undefined });
            else if (type === 'page') updateSelected({ interaction: { type: 'page', target: pages.find((page) => page.id !== pageId)?.id ?? pageId } });
            else updateSelected({ interaction: { type: 'url', target: 'https://example.com' } });
          }}><option value="none">无交互</option><option value="page">连接到设计面</option><option value="url">打开 URL</option></select></label>
          {selected.interaction?.type === 'page' && <label className="field-label interaction-target">目标画板<select value={selected.interaction.target} onChange={(event) => updateSelected({ interaction: { type: 'page', target: event.target.value } })}>{pages.map((page) => <option key={page.id} value={page.id}>{page.name} · {WORKSPACE_SURFACE_LABELS[page.surfaceKind ?? 'page']}</option>)}</select></label>}
          {selected.interaction?.type === 'url' && <label className="field-label interaction-target">目标 URL<input value={selected.interaction.target} onChange={(event) => updateSelected({ interaction: { type: 'url', target: event.target.value } })} placeholder="https://example.com" /></label>}
          <p className="helper-text inspector-prototype-help">连接普通页面时执行跳转；连接弹窗、抽屉、浮层或菜单时，会在来源页面上叠加预览目标画板。</p>
          </>}
          {inspectorTab === 'design' && <>
          <div className="size-row four">
            <NumberField label={editingSlot ? 'X · 内容' : 'X'} value={inspectedFrame.x} onChange={(x) => updateInspectedFrame({ x })} disabled={selected.locked} />
            <NumberField label={editingSlot ? 'Y · 内容' : 'Y'} value={inspectedFrame.y} onChange={(y) => updateInspectedFrame({ y })} disabled={selected.locked} />
            <NumberField label="W" value={inspectedFrame.width} onChange={(width) => updateInspectedFrame({ width })} disabled={selected.locked} />
            <NumberField label="H" value={inspectedFrame.height} onChange={(height) => updateInspectedFrame({ height })} disabled={selected.locked} />
          </div>
          <div className="panel-title section-title">响应式布局 · {device}</div>
          <label className="field-label responsive-constraint-field">水平约束<select value={selected.constraints?.[device]?.horizontal ?? 'auto'} onChange={(event) => updateSelectedHorizontalConstraint(event.target.value as WebHorizontalConstraint)}>{horizontalConstraintOptions.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}</select></label>
          <p className="helper-text responsive-constraint-help">{horizontalConstraintOptions.find((option) => option.id === (selected.constraints?.[device]?.horizontal ?? 'auto'))?.description}</p>
          <div className="size-constraints-grid"><NumberField label="最小宽" value={selected.constraints?.[device]?.minWidth ?? 16} min={16} onChange={(minWidth) => updateSelectedSizeConstraints({ minWidth: Math.max(16, minWidth) })} /><NumberField label="最大宽" value={selected.constraints?.[device]?.maxWidth ?? 100000} min={16} onChange={(maxWidth) => updateSelectedSizeConstraints({ maxWidth: Math.max(16, maxWidth) })} /><NumberField label="最小高" value={selected.constraints?.[device]?.minHeight ?? 16} min={16} onChange={(minHeight) => updateSelectedSizeConstraints({ minHeight: Math.max(16, minHeight) })} /><NumberField label="最大高" value={selected.constraints?.[device]?.maxHeight ?? 100000} min={16} onChange={(maxHeight) => updateSelectedSizeConstraints({ maxHeight: Math.max(16, maxHeight) })} /></div>
          <label className="ui-library-boolean-prop constraint-toggle"><input type="checkbox" checked={selected.constraints?.[device]?.lockAspectRatio === true} onChange={(event) => updateSelectedSizeConstraints({ lockAspectRatio: event.target.checked })} /><span>调整大小时保持当前宽高比</span></label>
          <div className="panel-title section-title">视觉设计 · {device}</div>
          {inspectorCapabilities?.visualStates && <div className="visual-state-switcher"><button className={inspectorVisualState === 'default' ? 'active' : ''} onClick={() => setInspectorVisualState('default')}>默认</button><button className={inspectorVisualState === 'hover' ? 'active' : ''} onClick={() => setInspectorVisualState('hover')}>悬停</button><button className={inspectorVisualState === 'active' ? 'active' : ''} onClick={() => setInspectorVisualState('active')}>按下</button><button className={inspectorVisualState === 'focus' ? 'active' : ''} onClick={() => setInspectorVisualState('focus')}>聚焦</button></div>}
          {inspectorCapabilities?.visualStates && inspectorVisualState !== 'default' && <div className="visual-state-help"><p className="helper-text">正在设计“{inspectorVisualState === 'hover' ? '悬停' : inspectorVisualState === 'active' ? '按下' : '聚焦'}”状态；画布会立即显示效果，预览时由真实交互触发。</p><button onClick={clearSelectedVisualState} disabled={!selected.states?.[inspectorVisualState]}>清除状态样式</button></div>}
          <section className="design-inspector-group">
            <header><strong>填充</strong><span>颜色、渐变与透明材质</span></header>
            <ColorValueField label="背景" value={inspectedStyle?.background ?? ''} onChange={(background) => updateSelectedStyle({ background })} allowComplex />
            <div className="style-preset-grid fill-presets">{fillPresets.map((fill, index) => <button key={fill} title={fill} aria-label={`填充预设 ${index + 1}`} style={{ background: fill }} onClick={() => updateSelectedStyle({ background: fill })} />)}</div>
            <div className="size-row"><NumberField label="内边距" value={inspectedStyle?.padding ?? 0} onChange={(padding) => updateSelectedStyle({ padding: Math.max(0, padding) })} /><NumberField label="透明度" value={inspectedStyle?.opacity ?? 1} step={0.05} min={0} max={1} onChange={(opacity) => updateSelectedStyle({ opacity: Math.min(1, Math.max(0, opacity)) })} /></div>
          </section>
          <section className="design-inspector-group">
            <header><strong>描边</strong><span>边框与圆角</span></header>
            <ColorValueField label="描边颜色" value={inspectedStyle?.borderColor ?? ''} onChange={(borderColor) => updateSelectedStyle({ borderColor })} />
            <div className="size-row"><NumberField label="粗细" value={inspectedStyle?.borderWidth ?? 0} onChange={(borderWidth) => updateSelectedStyle({ borderWidth: Math.max(0, borderWidth) })} /><NumberField label="圆角" value={inspectedStyle?.borderRadius ?? 0} onChange={(borderRadius) => updateSelectedStyle({ borderRadius: Math.max(0, borderRadius) })} /></div>
            <label className="field-label">线型<select value={inspectedStyle?.borderStyle ?? 'solid'} onChange={(event) => updateSelectedStyle({ borderStyle: event.target.value as NonNullable<WebComponentStyle['borderStyle']> })}><option value="solid">实线</option><option value="dashed">虚线</option><option value="dotted">点线</option><option value="double">双线</option><option value="none">无</option></select></label>
          </section>
          <section className="design-inspector-group">
            <header><strong>效果</strong><span>阴影、模糊与叠加</span></header>
            <label className="field-label">阴影<input value={inspectedStyle?.shadow ?? ''} onChange={(event) => updateSelectedStyle({ shadow: event.target.value })} placeholder="0 18px 48px rgba(0,0,0,.18)" /></label>
            <div className="style-preset-grid shadow-presets">{shadowPresets.map((shadow, index) => <button key={`${shadow}-${index}`} className={shadow ? '' : 'none'} title={shadow || '无阴影'} style={{ boxShadow: shadow || undefined }} onClick={() => updateSelectedStyle({ shadow })}>{shadow ? '' : '×'}</button>)}</div>
            <div className="size-row"><NumberField label="元素模糊" value={inspectedStyle?.blur ?? 0} onChange={(blur) => updateSelectedStyle({ blur: Math.max(0, blur) })} /><NumberField label="背景模糊" value={inspectedStyle?.backdropBlur ?? 0} onChange={(backdropBlur) => updateSelectedStyle({ backdropBlur: Math.max(0, backdropBlur) })} /></div>
            <div className="size-row"><NumberField label="旋转 °" value={inspectedStyle?.rotate ?? 0} onChange={(rotate) => updateSelectedStyle({ rotate })} /><NumberField label="缩放" value={inspectedStyle?.scale ?? 1} step={0.05} min={0.01} onChange={(scale) => updateSelectedStyle({ scale: Math.max(0.01, scale) })} /></div>
            <div className="size-row"><label className="field-label">溢出<select value={inspectedStyle?.overflow ?? 'visible'} onChange={(event) => updateSelectedStyle({ overflow: event.target.value as NonNullable<WebComponentStyle['overflow']> })}><option value="visible">显示</option><option value="hidden">裁切</option><option value="auto">自动滚动</option><option value="scroll">始终滚动</option></select></label><label className="field-label">混合模式<select value={inspectedStyle?.mixBlendMode ?? 'normal'} onChange={(event) => updateSelectedStyle({ mixBlendMode: event.target.value as NonNullable<WebComponentStyle['mixBlendMode']> })}><option value="normal">正常</option><option value="multiply">正片叠底</option><option value="screen">滤色</option><option value="overlay">叠加</option><option value="difference">差值</option></select></label></div>
          </section>
          {inspectorCapabilities?.typography && <section className="design-inspector-group">
            <header><strong>排版</strong><span>所有文字型组件与 UI 内容</span></header>
            <ColorValueField label="文字颜色" value={inspectedStyle?.color ?? ''} onChange={(color) => updateSelectedStyle({ color })} />
            <div className="size-row"><NumberField label="字号" value={inspectedStyle?.fontSize ?? 16} onChange={(fontSize) => updateSelectedStyle({ fontSize: Math.max(6, fontSize) })} /><NumberField label="字重" value={inspectedStyle?.fontWeight ?? 400} step={50} min={100} max={1000} onChange={(fontWeight) => updateSelectedStyle({ fontWeight: Math.min(1000, Math.max(100, fontWeight)) })} /></div>
            <div className="size-row"><NumberField label="行高" value={inspectedStyle?.lineHeight ?? 1.2} step={0.05} min={0.5} max={5} onChange={(lineHeight) => updateSelectedStyle({ lineHeight })} /><NumberField label="字间距" value={inspectedStyle?.letterSpacing ?? 0} step={0.1} onChange={(letterSpacing) => updateSelectedStyle({ letterSpacing })} /></div>
            <div className="size-row"><label className="field-label">对齐<select value={inspectedStyle?.textAlign ?? 'left'} onChange={(event) => updateSelectedStyle({ textAlign: event.target.value as NonNullable<WebComponentStyle['textAlign']> })}><option value="left">左对齐</option><option value="center">居中</option><option value="right">右对齐</option></select></label><label className="field-label">大小写<select value={inspectedStyle?.textTransform ?? 'none'} onChange={(event) => updateSelectedStyle({ textTransform: event.target.value as NonNullable<WebComponentStyle['textTransform']> })}><option value="none">保持</option><option value="uppercase">大写</option><option value="lowercase">小写</option><option value="capitalize">首字母大写</option></select></label></div>
          </section>}
          {inspectorCapabilities?.media && <section className="design-inspector-group"><header><strong>媒体</strong><span>裁切与焦点</span></header><div className="size-row"><label className="field-label">适应方式<select value={inspectedStyle?.objectFit ?? 'cover'} onChange={(event) => updateSelectedStyle({ objectFit: event.target.value as NonNullable<WebComponentStyle['objectFit']> })}><option value="cover">覆盖裁切</option><option value="contain">完整显示</option><option value="fill">拉伸填充</option><option value="none">原始大小</option><option value="scale-down">自动缩小</option></select></label><label className="field-label">焦点<input value={inspectedStyle?.objectPosition ?? '50% 50%'} onChange={(event) => updateSelectedStyle({ objectPosition: event.target.value })} /></label></div></section>}
          <section className="design-inspector-group advanced-css-group">
            <header><strong>高级样式</strong><span>开放式 CSS，不受面板枚举限制</span></header>
            <AdvancedCssEditor value={inspectorVisualState === 'default' ? inspectedFrame?.style.customCss ?? {} : selected.states?.[inspectorVisualState]?.customCss ?? {}} onChange={updateSelectedCustomCss} />
          </section>
          <div className="token-apply-row"><button onClick={() => applyColorToken('background', 'primary')}>主色背景</button><button onClick={() => applyColorToken('background', 'surface')}>表面背景</button><button onClick={() => applyColorToken('color', 'text')}>正文色</button><button onClick={() => applyRadiusToken('medium')}>中圆角</button></div>
          {inspectorCapabilities?.layout && <>
            <div className="panel-title section-title">容器布局 · {directChildCount} 个子组件</div>
            <label className="field-label">布局方式<select value={selected.layout?.mode ?? 'free'} onChange={(event) => updateSelectedLayout({ mode: event.target.value as NonNullable<WebDesignComponent['layout']>['mode'] })}><option value="free">自由布局</option><option value="flex-row">Flex 横向</option><option value="flex-column">Flex 纵向</option><option value="grid">Grid 网格</option></select></label>
            <div className="size-row"><NumberField label="间距" value={selected.layout?.gap ?? 16} onChange={(gap) => updateSelectedLayout({ gap })} /><NumberField label="内边距" value={selected.layout?.padding ?? 16} onChange={(padding) => updateSelectedLayout({ padding })} /></div>
            {selected.layout?.mode === 'grid' && <NumberField label="列数" value={selected.layout.columns ?? 2} onChange={(columns) => updateSelectedLayout({ columns: Math.max(1, Math.round(columns)) })} />}
            <div className="size-row"><label className="field-label layout-align-field">交叉轴<select value={selected.layout?.align ?? 'start'} onChange={(event) => updateSelectedLayout({ align: event.target.value as NonNullable<WebDesignComponent['layout']>['align'] })}><option value="start">起点</option><option value="center">居中</option><option value="end">终点</option><option value="stretch">拉伸</option></select></label><label className="field-label">主轴<select value={selected.layout?.justify ?? 'start'} onChange={(event) => updateSelectedLayout({ justify: event.target.value as NonNullable<WebDesignComponent['layout']>['justify'] })}><option value="start">起点</option><option value="center">居中</option><option value="end">终点</option><option value="space-between">两端分布</option><option value="space-around">环绕分布</option></select></label></div>
            {selected.layout?.mode === 'flex-row' && <label className="ui-library-boolean-prop"><input type="checkbox" checked={selected.layout?.wrap === true} onChange={(event) => updateSelectedLayout({ wrap: event.target.checked })} /><span>空间不足时自动换行</span></label>}
            <button className="secondary-button" disabled={directChildCount === 0 || selected.layout?.mode === 'free'} onClick={applySelectedAutoLayout}>应用自动布局</button>
          </>}
          </>}
          {inspectorTab === 'ai' && <>
          <div className="panel-title section-title">组件批注</div>
          <div className="notes-list">{selected.annotations.length === 0 && <span className="empty-hint">还没有批注</span>}{selected.annotations.map((note) => <div key={note.id} className={`note-card ${note.status}`}><span>{note.text}</span><small>{note.status === 'open' ? '待处理' : '已完成'}</small></div>)}</div>
          <textarea className="composer" rows={3} placeholder="例如：这里的按钮再醒目一些" value={annotationText} onChange={(event) => setAnnotationText(event.target.value)} /><button className="secondary-button" onClick={addLegacyAnnotation}>添加批注</button>
          <div className="panel-title section-title">与 AI 交互</div>
          <textarea className="composer" rows={4} placeholder="告诉 AI 如何修改当前画板里的选中组件…" value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} /><button className="ai-button" onClick={() => void addAiRequest()}>提交给 AI</button>
          </>}
        </> : <div className="empty-inspector"><div className="empty-icon">↖</div><strong>选择一个组件</strong><p>在画布或图层中选择组件，然后编辑、对齐、锁定、批注或提交 AI 请求。</p></div>}
      </aside>}
    </main>
    {variantPickerDefinition && variantPickerLibrary && <div className={`studio-side-surface-host ${variantPickerDrag?.dragging ? 'dragging-library-element' : ''}`}>
      <section className="studio-modal studio-side-surface variant-picker" data-library-portal-host>
        <header><div><span className="eyebrow">{variantPickerLibrary.displayName} · {variantPickerDefinition.category}</span><h2>{variantPickerDefinition.id} · {variantPickerDefinition.label}</h2><p>移动到想要的元素上，点击直接插入，或按住拖到画布中的准确位置。</p></div><button onClick={() => setVariantPickerTarget(undefined)}>×</button></header>
        <div className={`variant-preview-grid ${WIDE_VARIANT_PREVIEWS.has(variantPickerDefinition.id) || variantPickerPresentation?.previewSpan === 'wide' ? 'wide-component-previews' : ''} ${variantPickerVariants.length === 1 ? 'single-component-preview' : ''}`}>{variantPickerVariants.map((variant: any) => {
          const previewComponent = applyUiLibraryVariant(createComponentFromUiLibrary(variantPickerLibrary.id, variantPickerDefinition.id, 0, 0), variant.id);
          previewComponent.id = `library-preview-${variantPickerLibrary.id}-${variantPickerDefinition.id}-${variant.id}`;
          const differences = variantDifferenceLabels(variant);
          const interactiveVariant = variantIsInteractive(variant, variantPickerDefinition.id);
          const openOverlayPreview = OPEN_OVERLAY_PREVIEWS.has(variantPickerDefinition.id);
          const inlinePickerPreview = variantPickerLibrary.id === 'chakra' && ['DatePicker', 'ColorPicker'].includes(variantPickerDefinition.id);
          const previewHeight = variantPickerPresentation?.previewHeight ?? (inlinePickerPreview || interactiveVariant
            ? 440
            : Math.max(openOverlayPreview ? 320 : 118, Math.min(360, previewComponent.height + 24)));
          return <SelectableVariantCard key={variant.id} component={previewComponent} previewHeight={previewHeight} className={`variant-live-preview ${openOverlayPreview ? 'overlay-showcase' : ''} ${inlinePickerPreview ? 'inline-picker-showcase' : ''} ${variant.props.bordered === false || variant.props.variant === 'borderless' ? 'contrast-surface' : ''}`} interactive={interactiveVariant} variantLabel={variant.label} differences={differences} tokens={tokens} onPickItem={(selection) => chooseUiLibraryPreviewElement(variantPickerLibrary.id, variantPickerDefinition.id, variant.id, selection)} onPickPointerEvent={(event) => handleUiLibraryPreviewPointerEvent(variantPickerLibrary.id, variantPickerDefinition.id, variant.id, event)} />;
        })}</div>
      </section>
    </div>}
    {variantPickerDrag?.dragging && <div
      className="variant-picker-pointer-capture"
      onPointerMove={(event) => { event.preventDefault(); event.stopPropagation(); moveUiLibraryPreviewPointerDragAt(event.clientX, event.clientY); }}
      onPointerUp={(event) => { event.preventDefault(); event.stopPropagation(); finishUiLibraryPreviewPointerDragAt(event.clientX, event.clientY); }}
      onPointerCancel={(event) => { event.preventDefault(); event.stopPropagation(); finishUiLibraryPreviewPointerDragAt(event.clientX, event.clientY, true); }}
      onContextMenu={(event) => event.preventDefault()}
    />}
    {variantPickerDrag?.dragging && <div className="variant-picker-drag-ghost" style={{ left: variantPickerDrag.clientX, top: variantPickerDrag.clientY }}><strong>{variantPickerDrag.selection.label}</strong><small>{Math.round(variantPickerDrag.selection.width)} × {Math.round(variantPickerDrag.selection.height)}</small></div>}
    {themePickerOpen && <div className="studio-side-surface-host">
      <section className="studio-modal studio-side-surface theme-picker">
        <header><div><span className="eyebrow">Scene visual system</span><h2>建立视觉变量</h2><p>把颜色、字体和圆角写入真实 Scene Variable Collection，供 AI 与人工设计共同绑定使用。</p></div><button onClick={() => setThemePickerOpen(false)}>×</button></header>
        <div className="theme-preset-grid">{WEB_DESIGN_THEME_PRESETS.map((preset) => <button key={preset.id} onClick={() => void applyDesignTheme(preset)}><div className="theme-preview" style={{ background: preset.canvasBackground }}><i style={{ background: preset.preview[1] }} /><b style={{ background: preset.preview[2] }} /><span style={{ color: preset.tokens.colors.text }}>Aa</span></div><strong>{preset.name}</strong><small>{preset.description}</small><div className="theme-swatches">{preset.preview.map((color) => <i key={color} style={{ background: color }} />)}</div></button>)}</div>
      </section>
    </div>}
    {deleteDesignModal}
    {toast && <div className="toast">{toast}</div>}
  </div>
);
}
