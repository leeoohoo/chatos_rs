import {
  cloneComponentSubtrees,
  componentsForPage,
  updateComponentFrame
} from '../../src/editor-model';
import { exportPageHtml } from '../../src/html-exporter';
import { exportReactComponent } from '../../src/react-exporter';
import { exportVueComponent } from '../../src/vue-exporter';
import { componentDefaults } from '../../src/templates';
import {
  pagesForDocument,
  type WebDesignAsset,
  type WebDesignComponent
} from '../../src/schema';
import type {
  WorkspaceArtboardPlacement,
  WorkspaceSurfaceKind
} from '../../src/v2/workspace-placement-store';
import type { ScenePrototypeLink } from '../../src/v2/scene-schema';
import type { WebDesignDocumentActionContext } from './WebDesignActionContracts';

export function createWebDesignDocumentActions(context: WebDesignDocumentActionContext) {
  const {
    repository,
    document,
    setSelectedId,
    setSelectedIds,
    pageId,
    setPageId,
    setPreviewOverlayPageId,
    device,
    workspacePlacement,
    setWorkspacePlacement,
    setActiveArtboardId,
    annotationText,
    setAnnotationText,
    aiInstruction,
    setAiInstruction,
    sceneAiContext,
    setSceneAiContext,
    setSceneAnnotationPreparingId,
    setEditingSlot,
    interaction,
    documentRef,
    sceneDocumentRef,
    assetInput,
    selected,
    selectedSceneNode,
    activeScenePage,
    breakpoint,
    previewViewportHeight,
    pages,
    currentPage,
    editingContainer,
    editingSlotDefinition,
    showToast,
    commit,
    commitSceneCommand,
    updateComponent,
    save,
    resetSlotEditorCamera,
    activateWorkspaceArtboard,
    addWorkspaceSurface
  } = context;

  function switchPage(nextPageId: string) {
    setPreviewOverlayPageId(undefined);
    const artboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === nextPageId);
    if (artboard) activateWorkspaceArtboard(artboard);
    else setPageId(nextPageId);
    resetSlotEditorCamera();
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
  }

  function addPage() {
    void addWorkspaceSurface('page');
  }

  async function duplicateScenePage() {
    const scene = sceneDocumentRef.current;
    const sourcePage = scene?.pages.find((page) => page.id === pageId);
    if (!scene || !sourcePage) return;
    const newPageId = `page:${crypto.randomUUID()}`;
    const name = `${sourcePage.name} 副本`;
    try {
      await commitSceneCommand({ type: 'duplicate-page', pageId: sourcePage.id, newPageId, name }, '用户复制完整 Scene 画板。');
      const sourceArtboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === sourcePage.id);
      const artboard: WorkspaceArtboardPlacement = {
        artboardId: `artboard-${crypto.randomUUID().slice(0, 8)}`,
        pageId: newPageId,
        surfaceKind: sourceArtboard?.surfaceKind ?? 'page',
        viewportWidth: sourceArtboard?.viewportWidth ?? breakpoint.width,
        viewportHeight: sourceArtboard?.viewportHeight ?? previewViewportHeight,
        x: 0,
        y: sourceArtboard?.y ?? 0
      };
      setWorkspacePlacement((current) => current ? { ...current, artboards: [...current.artboards, artboard] } : current);
      activateWorkspaceArtboard(artboard);
      setSelectedId(undefined);
      setSelectedIds([]);
      showToast(`已复制画板“${sourcePage.name}”，响应式规则保持一致`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function duplicatePage() {
    if (sceneDocumentRef.current) {
      void duplicateScenePage();
      return;
    }
    const current = documentRef.current;
    if (!current || !currentPage) return;
    const id = `page-${crypto.randomUUID().slice(0, 8)}`;
    const page = { id, name: `${currentPage.name} 副本`, slug: `/page-${pagesForDocument(current).length + 1}`, surfaceKind: currentPage.surfaceKind ?? 'page' };
    const sourceIds = componentsForPage(current, currentPage.id).map((component) => component.id);
    const cloned = cloneComponentSubtrees(current, sourceIds, id, 0, current);
    commit((active) => ({ ...active, pages: [...pagesForDocument(active), page], components: [...active.components, ...cloned.components] }));
    const sourceArtboard = workspacePlacement?.artboards.find((candidate) => candidate.pageId === currentPage.id);
    if (workspacePlacement) {
      const duplicateArtboard: WorkspaceArtboardPlacement = {
        artboardId: `artboard-${crypto.randomUUID().slice(0, 8)}`,
        pageId: id,
        surfaceKind: sourceArtboard?.surfaceKind ?? 'page',
        viewportWidth: sourceArtboard?.viewportWidth ?? breakpoint.width,
        viewportHeight: sourceArtboard?.viewportHeight ?? previewViewportHeight,
        x: 0,
        y: sourceArtboard?.y ?? 0
      };
      setWorkspacePlacement({ ...workspacePlacement, artboards: [...workspacePlacement.artboards, duplicateArtboard] });
      setActiveArtboardId(duplicateArtboard.artboardId);
    }
    setPageId(id);
  }

  function deleteCurrentPage() {
    if (sceneDocumentRef.current) {
      const scene = sceneDocumentRef.current;
      const page = scene.pages.find((candidate) => candidate.id === pageId);
      if (!page || scene.pages.length <= 1) return;
      if (!window.confirm(`确定删除画板“${page.name}”及其全部 Scene 图层吗？此操作可撤销。`)) return;
      const nextPage = scene.pages.find((candidate) => candidate.id !== page.id);
      void commitSceneCommand({ type: 'delete-page', pageId: page.id }, '用户删除完整 Scene 画板。').then(() => {
        const remaining = workspacePlacement?.artboards.filter((artboard) => artboard.pageId !== page.id) ?? [];
        setWorkspacePlacement((current) => current ? { ...current, artboards: current.artboards.filter((artboard) => artboard.pageId !== page.id) } : current);
        const nextArtboard = remaining.find((artboard) => artboard.pageId === nextPage?.id) ?? remaining[0];
        if (nextArtboard) activateWorkspaceArtboard(nextArtboard);
        else if (nextPage) setPageId(nextPage.id);
        setSelectedId(undefined);
        setSelectedIds([]);
        showToast(`已删除画板“${page.name}”`);
      }).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      return;
    }
    const current = documentRef.current;
    if (!current || !currentPage || pagesForDocument(current).length <= 1) return;
    if (!window.confirm(`确定删除页面“${currentPage.name}”及其全部组件吗？`)) return;
    const removedIds = new Set(componentsForPage(current, currentPage.id).map((component) => component.id));
    const remainingPages = pagesForDocument(current).filter((page) => page.id !== currentPage.id);
    commit((active) => ({
      ...active,
      pages: remainingPages,
      components: active.components.filter((component) => !removedIds.has(component.id)).map((component) => component.interaction?.type === 'page' && component.interaction.target === currentPage.id
        ? { ...component, interaction: undefined }
        : component),
      symbols: active.symbols?.map((symbol) => ({
        ...symbol,
        components: symbol.components.map((component) => component.interaction?.type === 'page' && component.interaction.target === currentPage.id
          ? { ...component, interaction: undefined }
          : component)
      })),
      requests: active.requests.filter((request) => !request.componentId || !removedIds.has(request.componentId))
    }));
    const remainingArtboards = workspacePlacement?.artboards.filter((artboard) => artboard.pageId !== currentPage.id) ?? [];
    if (workspacePlacement) setWorkspacePlacement({ ...workspacePlacement, artboards: remainingArtboards });
    const nextArtboard = remainingArtboards.find((artboard) => artboard.pageId === remainingPages[0].id) ?? remainingArtboards[0];
    if (nextArtboard) activateWorkspaceArtboard(nextArtboard);
    else setPageId(remainingPages[0].id);
  }

  function updateCurrentPage(changes: Partial<{ name: string; slug: string; surfaceKind: WorkspaceSurfaceKind }>) {
    if (!currentPage) return;
    if (sceneDocumentRef.current) {
      if (changes.name?.trim() && changes.name.trim() !== currentPage.name) {
        void commitSceneCommand({ type: 'rename-page', pageId: currentPage.id, name: changes.name.trim() }, '用户重命名 Scene 画板。')
          .catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      }
      if (changes.surfaceKind) {
        setWorkspacePlacement((current) => current ? {
          ...current,
          artboards: current.artboards.map((artboard) => artboard.pageId === currentPage.id ? { ...artboard, surfaceKind: changes.surfaceKind! } : artboard)
        } : current);
      }
      return;
    }
    commit((current) => ({
      ...current,
      pages: pagesForDocument(current).map((page) => page.id === currentPage.id ? { ...page, ...changes } : page)
    }));
    if (changes.surfaceKind) {
      setWorkspacePlacement((current) => current ? {
        ...current,
        artboards: current.artboards.map((artboard) => artboard.pageId === currentPage.id ? { ...artboard, surfaceKind: changes.surfaceKind! } : artboard)
      } : current);
    }
  }

  function useAsset(asset: WebDesignAsset) {
    const current = documentRef.current;
    if (!current) return;
    if (selected?.type === 'image') {
      updateComponent(selected.id, (component) => ({ ...component, content: asset.dataUrl, name: asset.name }));
      return;
    }
    let component = componentDefaults('image', 70, 70);
    component.pageId = pageId;
    component.name = asset.name;
    component.content = asset.dataUrl;
    component.zIndex = Math.max(1, ...componentsForPage(current, pageId).map((item) => item.zIndex)) + 1;
    if (device !== 'desktop') component = updateComponentFrame(component, device, { x: 32, y: 80, width: 326, height: 220 });
    commit((active) => ({ ...active, components: [...active.components, component] }));
    setSelectedId(component.id);
    setSelectedIds([component.id]);
  }

  async function importAssets(files: FileList | null) {
    if (!files?.length) return;
    for (const file of Array.from(files)) {
      if (!file.type.startsWith('image/')) {
        showToast(`${file.name} 不是图片文件`);
        continue;
      }
      if (file.size > 8_000_000) {
        showToast(`${file.name} 超过 8MB`);
        continue;
      }
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error ?? new Error('读取图片失败'));
        reader.readAsDataURL(file);
      });
      const asset: WebDesignAsset = {
        id: `asset-${crypto.randomUUID().slice(0, 8)}`,
        name: file.name,
        mimeType: file.type,
        dataUrl,
        createdAt: new Date().toISOString()
      };
      commit((current) => ({ ...current, assets: [...(current.assets ?? []), asset] }));
      useAsset(asset);
    }
    if (assetInput.current) assetInput.current.value = '';
  }

  function downloadTextFile(filename: string, content: string, mimeType: string) {
    const blob = new Blob([content], { type: `${mimeType};charset=utf-8` });
    const url = URL.createObjectURL(blob);
    const link = window.document.createElement('a');
    link.href = url;
    link.download = filename;
    link.style.display = 'none';
    window.document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    showToast(`已导出 ${filename}`);
  }

  function exportCurrentPage() {
    const current = documentRef.current;
    if (!current || !currentPage) return;
    const filename = currentPage.slug === '/' ? 'index.html' : `${currentPage.slug.replace(/^\/+|\/+$/g, '') || currentPage.id}.html`;
    downloadTextFile(filename, exportPageHtml(current, currentPage.id, device), 'text/html');
  }

  function exportReact() {
    const current = documentRef.current;
    if (!current) return;
    const file = exportReactComponent(current, device);
    downloadTextFile(file.filename, file.content, 'text/javascript');
  }

  function exportVue() {
    const current = documentRef.current;
    if (!current) return;
    const file = exportVueComponent(current, device);
    downloadTextFile(file.filename, file.content, 'text/plain');
  }

  function activatePreviewInteraction(component: WebDesignComponent) {
    if (!component.interaction) return;
    if (component.interaction.type === 'page') {
      const target = pages.find((page) => page.id === component.interaction!.target);
      if (!target) return;
      if ((target.surfaceKind ?? 'page') === 'page') switchPage(target.id);
      else setPreviewOverlayPageId(target.id);
      return;
    }
    window.open(component.interaction.target, '_blank', 'noopener,noreferrer');
  }

  function activateScenePrototype(link: ScenePrototypeLink) {
    const target = pages.find((page) => page.id === link.targetPageId);
    if (!target) {
      showToast('原型目标画板已经不存在');
      return;
    }
    if (link.action === 'navigate') switchPage(target.id);
    else setPreviewOverlayPageId(target.id);
  }

  function addLegacyAnnotation() {
    if (!selected || !annotationText.trim()) return;
    const annotation = { id: `note-${crypto.randomUUID().slice(0, 8)}`, text: annotationText.trim(), status: 'open' as const, createdAt: new Date().toISOString() };
    updateComponent(selected.id, (component) => ({ ...component, annotations: [...component.annotations, annotation] }));
    setAnnotationText('');
  }

  async function prepareSceneAnnotation(nodeId: string, annotationId: string) {
    const scene = sceneDocumentRef.current;
    if (!repository || !scene) return;
    setSceneAnnotationPreparingId(annotationId);
    try {
      const context = await repository.prepareSceneAnnotationTask(scene.documentId, {
        nodeId,
        annotationId,
        viewportWidth: breakpoint.width
      });
      setSceneAiContext(context);
      showToast('视觉批注已准备，AI 可按稳定节点和截图继续修改');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setSceneAnnotationPreparingId(undefined);
    }
  }

  async function addSceneAnnotation(body = annotationText, prepareForAi = false) {
    const node = selectedSceneNode ?? activeScenePage?.children[0];
    const trimmed = body.trim();
    if (!node || !trimmed) return;
    const annotationId = `annotation:${crypto.randomUUID()}`;
    try {
      await commitSceneCommand({ type: 'add-annotation', nodeId: node.id, annotationId, body: trimmed }, '用户为 Scene 图层添加视觉批注。');
      setAnnotationText('');
      if (prepareForAi) await prepareSceneAnnotation(node.id, annotationId);
      else showToast('已添加批注，AI 会把它视为待处理设计任务');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function changeSceneAnnotationStatus(annotationId: string, status: 'open' | 'resolved') {
    if (!selectedSceneNode) return;
    try {
      await commitSceneCommand({
        type: status === 'open' ? 'reopen-annotation' : 'resolve-annotation',
        nodeId: selectedSceneNode.id,
        annotationId
      }, status === 'open' ? '用户重新打开 Scene 批注。' : '用户确认 Scene 批注已完成。');
      if (sceneAiContext?.task.annotationId === annotationId) setSceneAiContext(undefined);
      showToast(status === 'open' ? '批注已重新打开' : '批注已标记完成');
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  async function submitSceneAiInstruction() {
    const instruction = aiInstruction.trim();
    if (!instruction) return;
    setAiInstruction('');
    await addSceneAnnotation(instruction, true);
  }

  async function addAiRequest(instruction = aiInstruction) {
    const current = documentRef.current;
    if (!current || !instruction.trim()) return;
    const target = selected ?? editingContainer;
    const request = {
      id: `request-${crypto.randomUUID().slice(0, 8)}`,
      componentId: target?.id,
      instruction: `[${device}][${editingSlotDefinition ? `内容区域:${editingSlotDefinition.label}` : target ? `组件:${target.name}` : `页面:${currentPage?.name ?? pageId}`}] ${instruction.trim()}`,
      status: 'pending' as const,
      createdAt: new Date().toISOString()
    };
    commit((active) => ({ ...active, requests: [...active.requests, request] }));
    setAiInstruction('');
    await save(true, true);
    showToast(target ? '已提交组件修改任务' : '已提交整页设计任务');
  }


  return {
    switchPage,
    addPage,
    duplicateScenePage,
    duplicatePage,
    deleteCurrentPage,
    updateCurrentPage,
    useAsset,
    importAssets,
    downloadTextFile,
    exportCurrentPage,
    exportReact,
    exportVue,
    activatePreviewInteraction,
    activateScenePrototype,
    addLegacyAnnotation,
    prepareSceneAnnotation,
    addSceneAnnotation,
    changeSceneAnnotationStatus,
    submitSceneAiInstruction,
    addAiRequest
  };
}
