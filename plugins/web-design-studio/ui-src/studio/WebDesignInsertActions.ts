import { type DragEvent } from 'react';
import { flushSync } from 'react-dom';
import { componentsForPage, resolveComponent, updateComponentFrame } from '../../src/editor-model';
import { editableSlotsForUiComponent } from '../../src/library-slots';
import { applyUiLibraryVariant, createComponentFromUiLibrary, uiLibraryByName } from '../../src/ui-libraries';
import { type WebDesignComponent, type WebDesignLibraryName } from '../../src/schema';
import { type LibraryPreviewPointerEvent, type LibraryPreviewSelection } from '../library-runtime/element-selection';
import type { WorkspaceArtboardPlacement } from '../../src/v2/workspace-placement-store';
import { indexSceneDocument, type SceneDocument, type SceneNode } from '../../src/v2/scene-schema';
import { createSceneBasicShape, createSceneLibraryInstance } from './scene-node-factory';
import { resolveSceneInsertionTarget, type SceneInsertionTarget } from './scene-insertion-target';
import { BasicShapeId, palette, contentContainerAncestor, createSlotStarterComponents, VariantPickerTarget, VariantPickerPointerDrag, bindLibraryPreviewElement } from './WebDesignStudioSupport';
import type { WebDesignDeferredActions } from './WebDesignActionContracts';

type WebDesignActionContext =
  ReturnType<typeof import('./useWebDesignStudioState').useWebDesignStudioState> &
  ReturnType<typeof import('./WebDesignCoreActions').createWebDesignCoreActions> &
  Pick<WebDesignDeferredActions,
    'editComponentSlot' | 'activateWorkspaceArtboard' | 'withGeneratedResponsiveLayouts'>;

export function createWebDesignInsertActions(context: WebDesignActionContext) {
  const {
    setSelectedId,
    setSelectedIds,
    pageId,
    interactionMode,
    device,
    workspacePlacement,
    activeArtboardId,
    variantPickerTarget,
    setVariantPickerTarget,
    sceneContentFocus,
    setVariantPickerDrag,
    editingSlot,
    variantPickerDragRef,
    documentRef,
    sceneDocumentRef,
    breakpoint,
    editingSlotCanvasSize,
    showToast,
    commitWithCanvasGrowth,
    commitSceneCommand,
    updateComponent,
    editComponentSlot,
    activateWorkspaceArtboard,
    withGeneratedResponsiveLayouts
  } = context;

  function onPaletteDrag(event: DragEvent, shapeId: BasicShapeId) {
    event.dataTransfer.setData('application/x-web-design-shape', shapeId);
    event.dataTransfer.effectAllowed = 'copy';
  }

  function addUiLibraryComponent(libraryName: WebDesignLibraryName, definitionId: string, x: number, y: number, variantId?: string, targetSlot = editingSlot, registryElement?: LibraryPreviewSelection): WebDesignComponent | undefined {
    const current = documentRef.current;
    if (!current) return;
    const library = uiLibraryByName(libraryName);
    if (!library) return;
    const container = targetSlot ? current.components.find((candidate) => candidate.id === targetSlot.componentId) : undefined;
    const containerFrame = container ? resolveComponent(container, device) : undefined;
    const componentX = containerFrame ? containerFrame.x + x : x;
    const componentY = containerFrame ? containerFrame.y + y : y;
    let component = createComponentFromUiLibrary(libraryName, definitionId, componentX, componentY);
    if (variantId) component = applyUiLibraryVariant(component, variantId);
    if (registryElement) component = bindLibraryPreviewElement(component, library.displayName, registryElement);
    component.pageId = pageId;
    if (container && targetSlot) {
      component.parentId = container.id;
      component.slot = targetSlot.slotId;
      component.zIndex = Math.max(0, ...current.components.filter((item) => item.parentId === container.id && item.slot === targetSlot.slotId).map((item) => item.zIndex)) + 1;
    } else {
      component.zIndex = Math.max(1, ...componentsForPage(current, pageId).filter((item) => !contentContainerAncestor(current, item)).map((item) => item.zIndex)) + 1;
    }
    if (device !== 'desktop') component = updateComponentFrame(component, device, { x: componentX, y: componentY });
    const starterSlot = !container && ['Drawer', 'Modal', 'Dialog', 'Sheet', 'AlertDialog'].includes(definitionId) ? editableSlotsForUiComponent(component)[0] : undefined;
    const starter = starterSlot ? createSlotStarterComponents(component, starterSlot, 'form', pageId, device) : [];
    commitWithCanvasGrowth((active) => withGeneratedResponsiveLayouts({ ...active, components: [...active.components, component, ...starter] }, pageId), ['desktop', 'tablet', 'mobile']);
    setSelectedId(component.id);
    setSelectedIds([component.id]);
    showToast(container ? `已添加到${editableSlotsForUiComponent(container).find((slot) => slot.id === targetSlot?.slotId)?.label ?? '组件内容'}` : `已插入 ${library.displayName} ${component.library?.component}`);
    return component;
  }

  function scenePageRoot(scene: SceneDocument, targetPageId: string) {
    const page = scene.pages.find((candidate) => candidate.id === targetPageId);
    const root = page?.children[0];
    if (!page || !root || !('children' in root) || !Array.isArray(root.children)) {
      throw new Error('当前画板没有可插入内容的 Scene 根节点。');
    }
    return { page, root };
  }

  async function insertSceneLibraryComponent(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    x: number,
    y: number,
    variantId?: string,
    registryElement?: LibraryPreviewSelection,
    targetPageId = pageId,
    insertionTarget?: SceneInsertionTarget
  ): Promise<SceneNode | undefined> {
    const scene = sceneDocumentRef.current;
    if (!scene) return undefined;
    const { root } = scenePageRoot(scene, targetPageId);
    const node = createSceneLibraryInstance({
      nodeId: `library:${crypto.randomUUID()}`,
      libraryName,
      definitionId,
      variantId,
      x: Math.max(0, Math.round(x)),
      y: Math.max(0, Math.round(y)),
      registryElement
    });
    await commitSceneCommand({
      type: 'insert-node',
      parentId: insertionTarget?.nodeId ?? root.id,
      slot: insertionTarget?.slot,
      index: insertionTarget?.index ?? root.children.length,
      node
    }, `用户从 ${libraryName} 组件库插入 ${node.name}。`);
    setSelectedId(node.id);
    setSelectedIds([node.id]);
    const targetName = insertionTarget ? indexSceneDocument(scene).get(insertionTarget.nodeId)?.node.name : undefined;
    showToast(targetName ? `已插入 ${node.name} 到 ${targetName}` : `已插入 ${node.name}`);
    return node;
  }

  async function insertSceneBasicShape(
    shape: BasicShapeId,
    x: number,
    y: number,
    targetPageId: string,
    insertionTarget?: SceneInsertionTarget
  ): Promise<SceneNode | undefined> {
    const scene = sceneDocumentRef.current;
    if (!scene) return undefined;
    const { root } = scenePageRoot(scene, targetPageId);
    const node = createSceneBasicShape({
      nodeId: `shape:${crypto.randomUUID()}`,
      shape,
      x: Math.max(0, Math.round(x)),
      y: Math.max(0, Math.round(y))
    });
    await commitSceneCommand({
      type: 'insert-node',
      parentId: insertionTarget?.nodeId ?? root.id,
      slot: insertionTarget?.slot,
      index: insertionTarget?.index ?? root.children.length,
      node
    }, `用户插入基本图形 ${node.name}。`);
    setSelectedId(node.id);
    setSelectedIds([node.id]);
    showToast(`已插入${node.name}`);
    return node;
  }

  async function onSceneCanvasDrop(event: DragEvent<HTMLDivElement>, artboard: WorkspaceArtboardPlacement) {
    event.preventDefault();
    event.stopPropagation();
    if (interactionMode || !sceneDocumentRef.current) return;
    activateWorkspaceArtboard(artboard);
    const bounds = event.currentTarget.getBoundingClientRect();
    const scaleX = bounds.width / Math.max(1, artboard.viewportWidth);
    const scaleY = bounds.height / Math.max(1, event.currentTarget.offsetHeight);
    const x = (event.clientX - bounds.left) / Math.max(scaleX, .0001);
    const y = (event.clientY - bounds.top) / Math.max(scaleY, .0001);
    const libraryPayload = event.dataTransfer.getData('application/x-web-design-library');
    try {
      const scene = sceneDocumentRef.current;
      const insertionTarget = resolveSceneInsertionTarget({
        document: scene,
        pageId: artboard.pageId,
        viewportWidth: artboard.viewportWidth,
        point: { x, y },
        preferred: sceneContentFocus?.pageId === artboard.pageId ? sceneContentFocus : undefined
      });
      if (libraryPayload) {
        const parsed = JSON.parse(libraryPayload) as VariantPickerTarget & {
          definitionId?: string;
          variantId?: string;
          registryElement?: LibraryPreviewSelection;
        };
        const definitionId = parsed.definitionId ?? parsed.componentId;
        if (uiLibraryByName(parsed.library)?.components.some((item) => item.id === definitionId)) {
          await insertSceneLibraryComponent(parsed.library, definitionId, insertionTarget.x, insertionTarget.y, parsed.variantId, parsed.registryElement, artboard.pageId, insertionTarget);
          setVariantPickerDrag(undefined);
          if (parsed.registryElement) setVariantPickerTarget(undefined);
          return;
        }
      }
      const shapeId = event.dataTransfer.getData('application/x-web-design-shape') as BasicShapeId;
      if (palette.some((item) => item.id === shapeId)) await insertSceneBasicShape(shapeId, insertionTarget.x, insertionTarget.y, artboard.pageId, insertionTarget);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function insertUiLibraryComponent(libraryName: WebDesignLibraryName, definitionId: string, variantId?: string, registryElement?: LibraryPreviewSelection) {
    const definition = uiLibraryByName(libraryName)?.components.find((candidate) => candidate.id === definitionId);
    if (!definition) return;
    if (sceneDocumentRef.current) {
      const targetArtboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === activeArtboardId)
        ?? workspacePlacement?.artboards.find((candidate) => candidate.pageId === pageId);
      const targetPageId = targetArtboard?.pageId ?? pageId;
      const targetWidth = targetArtboard?.viewportWidth ?? breakpoint.width;
      const width = registryElement?.width ?? definition.width;
      const focusedTarget = sceneContentFocus?.pageId === targetPageId
        ? resolveSceneInsertionTarget({
          document: sceneDocumentRef.current,
          pageId: targetPageId,
          viewportWidth: targetWidth,
          point: { x: 0, y: 0 },
          preferred: sceneContentFocus
        })
        : undefined;
      setVariantPickerTarget(undefined);
      void insertSceneLibraryComponent(
        libraryName,
        definitionId,
        focusedTarget ? 24 : Math.max(24, Math.round((targetWidth - width) / 2)),
        focusedTarget ? 24 : 80,
        variantId,
        registryElement,
        targetPageId,
        focusedTarget ? { ...focusedTarget, x: 24, y: 24 } : undefined
      ).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      return;
    }
    let inserted: WebDesignComponent | undefined;
    if (editingSlotCanvasSize) {
      const width = registryElement?.width ?? definition.width;
      inserted = addUiLibraryComponent(libraryName, definitionId, Math.max(12, Math.round((editingSlotCanvasSize.width - width) / 2)), 28, variantId, editingSlot, registryElement);
    } else {
      const width = registryElement?.width ?? definition.width;
      inserted = addUiLibraryComponent(libraryName, definitionId, Math.max(24, Math.round((breakpoint.width - width) / 2)), 80, variantId, editingSlot, registryElement);
    }
    setVariantPickerTarget(undefined);
    const compoundSlot = inserted?.library?.props.registryDemo && !inserted.library.props.registryElement
      ? editableSlotsForUiComponent(inserted).find((slot) => slot.id === 'content')
      : undefined;
    if (inserted && compoundSlot) {
      window.setTimeout(() => { void editComponentSlot(inserted!, compoundSlot.id, { compoundOnly: true }); }, 120);
    }
  }

  function chooseUiLibraryPreviewElement(libraryName: WebDesignLibraryName, definitionId: string, variantId: string, registryElement: LibraryPreviewSelection) {
    const replaceComponentId = variantPickerTarget?.replaceComponentId;
    if (!replaceComponentId) {
      insertUiLibraryComponent(libraryName, definitionId, variantId, registryElement);
      return;
    }
    const library = uiLibraryByName(libraryName);
    if (!library) return;
    const scene = sceneDocumentRef.current;
    const sceneNode = scene && replaceComponentId ? indexSceneDocument(scene).get(replaceComponentId)?.node : undefined;
    if (scene && sceneNode) {
      if (sceneNode.type !== 'library-instance') {
        showToast('当前 Scene 图层不是组件库实例，不能直接替换变体。');
        return;
      }
      const replacement = createSceneLibraryInstance({
        nodeId: sceneNode.id,
        libraryName,
        definitionId,
        variantId,
        x: sceneNode.frame.x,
        y: sceneNode.frame.y,
        registryElement
      });
      setVariantPickerTarget(undefined);
      void commitSceneCommand({
        type: 'update-node',
        nodeId: sceneNode.id,
        patches: [
          { path: ['name'], value: replacement.name },
          { path: ['library'], value: replacement.library },
          { path: ['component'], value: replacement.component },
          { path: ['variant'], value: replacement.variant ?? '' },
          { path: ['properties'], value: replacement.properties },
          { path: ['content'], value: replacement.content ?? '' },
          { path: ['frame', 'width'], value: replacement.frame.width },
          { path: ['frame', 'height'], value: replacement.frame.height }
        ]
      }, `用户把 Scene 组件替换为 ${registryElement.label}。`).then(() => {
        setSelectedId(sceneNode.id);
        setSelectedIds([sceneNode.id]);
        showToast(`已改为 ${registryElement.label}`);
      }).catch((error) => showToast(error instanceof Error ? error.message : String(error)));
      return;
    }
    if (scene) {
      setVariantPickerTarget(undefined);
      showToast('要替换的 Scene 组件已经不存在，请重新选择。');
      return;
    }
    updateComponent(replaceComponentId, (component) => {
      let next = bindLibraryPreviewElement(applyUiLibraryVariant(component, variantId), library.displayName, registryElement);
      if (device !== 'desktop') next = updateComponentFrame(next, device, { width: next.width, height: next.height });
      return next;
    });
    setVariantPickerTarget(undefined);
    setSelectedId(replaceComponentId);
    setSelectedIds([replaceComponentId]);
    showToast(`已改为 ${registryElement.label}`);
  }

  function beginUiLibraryPreviewPointerDrag(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    variantId: string,
    state: LibraryPreviewPointerEvent
  ) {
    const next: VariantPickerPointerDrag = {
      selection: state.selection,
      pointerId: state.pointerId,
      clientX: state.clientX,
      clientY: state.clientY,
      libraryName,
      definitionId,
      variantId,
      startClientX: state.clientX,
      startClientY: state.clientY,
      dragging: false
    };
    variantPickerDragRef.current = next;
    flushSync(() => setVariantPickerDrag(next));
  }

  function moveUiLibraryPreviewPointerDragAt(clientX: number, clientY: number) {
    const active = variantPickerDragRef.current;
    if (!active) return;
    const dragging = active.dragging
      || Math.hypot(clientX - active.startClientX, clientY - active.startClientY) >= 5;
    if (!dragging && clientX === active.clientX && clientY === active.clientY) return;
    const next = { ...active, clientX, clientY, dragging };
    variantPickerDragRef.current = next;
    setVariantPickerDrag(next);
  }

  function finishUiLibraryPreviewPointerDragAt(clientX: number, clientY: number, cancelled = false) {
    const active = variantPickerDragRef.current;
    if (!active) return;
    variantPickerDragRef.current = undefined;
    setVariantPickerDrag(undefined);
    if (cancelled) return;
    if (!active.dragging) {
      chooseUiLibraryPreviewElement(active.libraryName, active.definitionId, active.variantId, active.selection);
      return;
    }
    dropUiLibraryPreviewElement(active.libraryName, active.definitionId, active.variantId, active.selection, {
      clientX,
      clientY
    });
  }

  function handleUiLibraryPreviewPointerEvent(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    variantId: string,
    event: LibraryPreviewPointerEvent
  ) {
    if (event.phase === 'start') beginUiLibraryPreviewPointerDrag(libraryName, definitionId, variantId, event);
    else if (event.phase === 'move') moveUiLibraryPreviewPointerDragAt(event.clientX, event.clientY);
    else finishUiLibraryPreviewPointerDragAt(event.clientX, event.clientY, event.phase === 'cancel');
  }

  function dropUiLibraryPreviewElement(
    libraryName: WebDesignLibraryName,
    definitionId: string,
    variantId: string,
    registryElement: LibraryPreviewSelection,
    point: { clientX: number; clientY: number }
  ) {
    variantPickerDragRef.current = undefined;
    setVariantPickerDrag(undefined);
    if (variantPickerTarget?.replaceComponentId) {
      chooseUiLibraryPreviewElement(libraryName, definitionId, variantId, registryElement);
      return;
    }
    const canvasSelector = editingSlot ? '.slot-design-canvas' : '.design-canvas:not(.slot-design-canvas)';
    const canvas = [...window.document.querySelectorAll<HTMLElement>(canvasSelector)].find((candidate) => {
      const bounds = candidate.getBoundingClientRect();
      return point.clientX >= bounds.left && point.clientX <= bounds.right
        && point.clientY >= bounds.top && point.clientY <= bounds.bottom;
    });
    if (!canvas) {
      showToast('请把元素拖到中间画布区域');
      return;
    }
    const bounds = canvas.getBoundingClientRect();
    const scaleX = bounds.width / Math.max(1, canvas.offsetWidth);
    const scaleY = bounds.height / Math.max(1, canvas.offsetHeight);
    const x = Math.max(0, Math.round((point.clientX - bounds.left) / Math.max(scaleX, .0001) - registryElement.width / 2));
    const y = Math.max(0, Math.round((point.clientY - bounds.top) / Math.max(scaleY, .0001) - registryElement.height / 2));
    if (sceneDocumentRef.current && !editingSlot) {
      const artboard = workspacePlacement?.artboards.find((candidate) => candidate.artboardId === canvas.dataset.artboardId);
      if (!artboard) {
        showToast('没有找到目标 Scene 画板，请重新拖入。');
        return;
      }
      activateWorkspaceArtboard(artboard);
      const insertionTarget = resolveSceneInsertionTarget({
        document: sceneDocumentRef.current,
        pageId: artboard.pageId,
        viewportWidth: artboard.viewportWidth,
        point: { x, y },
        preferred: sceneContentFocus?.pageId === artboard.pageId ? sceneContentFocus : undefined
      });
      void insertSceneLibraryComponent(libraryName, definitionId, insertionTarget.x, insertionTarget.y, variantId, registryElement, artboard.pageId, insertionTarget)
        .catch((error) => showToast(error instanceof Error ? error.message : String(error)));
    } else {
      addUiLibraryComponent(libraryName, definitionId, x, y, variantId, editingSlot, registryElement);
    }
    setVariantPickerTarget(undefined);
  }


  return {
    onPaletteDrag,
    addUiLibraryComponent,
    scenePageRoot,
    insertSceneLibraryComponent,
    insertSceneBasicShape,
    onSceneCanvasDrop,
    insertUiLibraryComponent,
    chooseUiLibraryPreviewElement,
    beginUiLibraryPreviewPointerDrag,
    moveUiLibraryPreviewPointerDragAt,
    finishUiLibraryPreviewPointerDragAt,
    handleUiLibraryPreviewPointerEvent,
    dropUiLibraryPreviewElement
  };
}
