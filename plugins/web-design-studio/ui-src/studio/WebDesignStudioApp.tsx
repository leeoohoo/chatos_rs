import { lazy, Suspense, useEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import {
  autoLayoutContainer,
  breakpointFor,
  cloneComponentSubtrees,
  componentsForPage,
  createSymbolFromSelection,
  detachSymbolInstance,
  descendantIds,
  flattenComponentTree,
  moveComponentsWithDescendants,
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
import { createBlockPreset, createPageTemplate, WEB_DESIGN_BLOCK_PRESETS, WEB_DESIGN_PAGE_TEMPLATES, type WebDesignBlockPresetId, type WebDesignPageTemplateId } from '../../src/component-library';
import { ANTD_CATEGORIES, ANTD_COMPONENTS, ANTD_COMPONENT_VARIANTS, ANTD_VERSION, applyAntdComponentVariant, createAntdComponent, variantsForAntdComponent } from '../../src/antd-library';
import { editableSlotsForAntdComponent, isAntdContentContainer, type AntdEditableSlot } from '../../src/antd-slots';
import { WEB_DESIGN_THEME_PRESETS, type WebDesignThemePreset } from '../../src/design-themes';
import { componentDefaults } from '../../src/templates';
import {
  pagesForDocument,
  tokensForDocument,
  type WebComponentStyle,
  type WebComponentType,
  type WebDesignAsset,
  type WebDesignComponent,
  type WebDesignDevice,
  type WebDesignDocument,
  type WebDesignJsonValue,
  type WebDesignSymbol,
  type WebDesignTokens,
  type WebSymbolOverride
} from '../../src/schema';
import { createRepository, type DesignRepository, type DesignSummary } from './repository';

const AntdCanvasComponent = lazy(() => import('./AntdCanvasComponent').then((module) => ({ default: module.AntdCanvasComponent })));

type BasicShapeId = 'rectangle' | 'ellipse' | 'line';

const palette: Array<{ id: BasicShapeId; type: WebComponentType; label: string; icon: string; keywords: string[] }> = [
  { id: 'rectangle', type: 'section', label: '矩形', icon: '▭', keywords: ['rectangle', '矩形', '容器'] },
  { id: 'ellipse', type: 'section', label: '圆形', icon: '○', keywords: ['ellipse', 'circle', '圆形'] },
  { id: 'line', type: 'divider', label: '直线', icon: '—', keywords: ['line', 'divider', '直线'] }
];

function basicShapeDefaults(shapeId: BasicShapeId, x: number, y: number): WebDesignComponent {
  if (shapeId === 'line') {
    const line = componentDefaults('divider', x, y);
    line.id = `shape-line-${crypto.randomUUID().slice(0, 8)}`;
    line.name = '直线';
    line.width = 320;
    line.style = { borderColor: '#8E8E93', borderWidth: 1 };
    return line;
  }
  const shape = componentDefaults('section', x, y);
  shape.id = `shape-${shapeId}-${crypto.randomUUID().slice(0, 8)}`;
  shape.name = shapeId === 'ellipse' ? '圆形' : '矩形';
  shape.width = shapeId === 'ellipse' ? 160 : 260;
  shape.height = shapeId === 'ellipse' ? 160 : 160;
  shape.style = { background: '#EAF3FF', borderColor: '#A8CCFF', borderWidth: 1, borderRadius: shapeId === 'ellipse' ? 999 : 16 };
  return shape;
}

function contentContainerAncestor(document: WebDesignDocument, component: WebDesignComponent): WebDesignComponent | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let parent = component.parentId ? byId.get(component.parentId) : undefined;
  while (parent) {
    if (isAntdContentContainer(parent)) return parent;
    parent = parent.parentId ? byId.get(parent.parentId) : undefined;
  }
  return undefined;
}

function slotIdForDescendant(document: WebDesignDocument, component: WebDesignComponent, containerId: string): string | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let current = component;
  while (current.parentId && current.parentId !== containerId) {
    const parent = byId.get(current.parentId);
    if (!parent) return undefined;
    current = parent;
  }
  return current.parentId === containerId ? (current.slot ?? 'content') : undefined;
}

function componentsInSlot(document: WebDesignDocument, containerId: string, slotId: string): WebDesignComponent[] {
  return document.components.filter((component) => slotIdForDescendant(document, component, containerId) === slotId);
}

function visibleComponentsInSlot(document: WebDesignDocument, containerId: string, slotId: string): WebDesignComponent[] {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  return componentsInSlot(document, containerId, slotId).filter((component) => {
    let parent = component.parentId ? byId.get(component.parentId) : undefined;
    while (parent && parent.id !== containerId) {
      if (isAntdContentContainer(parent)) return false;
      parent = parent.parentId ? byId.get(parent.parentId) : undefined;
    }
    return true;
  });
}

function createSlotStarterComponents(
  container: WebDesignComponent,
  slot: AntdEditableSlot,
  template: 'form' | 'details',
  pageId: string,
  device: WebDesignDevice
): WebDesignComponent[] {
  const containerFrame = resolveComponent(container, device);
  const availableWidth = Math.max(220, slot.width - 24);
  const rows = template === 'form'
    ? [
      { definition: 'Typography', variant: 'title', name: '表单标题', content: '完善信息', x: 12, y: 12, width: availableWidth, height: 44 },
      { definition: 'Input', variant: 'outlined', name: '姓名输入', content: '请输入姓名', x: 12, y: 76, width: availableWidth, height: 40 },
      { definition: 'Select', variant: 'outlined', name: '类型选择', content: '请选择类型', x: 12, y: 132, width: availableWidth, height: 40 },
      { definition: 'Input', variant: 'textarea', name: '详细说明', content: '请输入详细说明', x: 12, y: 188, width: availableWidth, height: 96 },
      { definition: 'Button', variant: 'primary', name: '提交按钮', content: '保存修改', x: 12, y: 304, width: 120, height: 40 }
    ]
    : [
      { definition: 'Typography', variant: 'title', name: '详情标题', content: '产品详情', x: 12, y: 12, width: availableWidth, height: 44 },
      { definition: 'Descriptions', variant: 'bordered', name: '详情信息', content: '', x: 12, y: 72, width: availableWidth, height: 170 },
      { definition: 'Divider', variant: 'plain', name: '内容分隔线', content: '', x: 12, y: 258, width: availableWidth, height: 24 },
      { definition: 'Button', variant: 'primary', name: '确认按钮', content: '确认', x: 12, y: 300, width: 100, height: 40 }
    ];
  return rows.map((row, index) => {
    let child = createAntdComponent(row.definition, container.x + row.x, container.y + row.y);
    child = applyAntdComponentVariant(child, row.variant);
    child.name = row.name;
    child.content = row.content;
    child.pageId = pageId;
    child.parentId = container.id;
    child.slot = slot.id;
    child.zIndex = index + 1;
    child.width = row.width;
    child.height = row.height;
    if (device !== 'desktop') child = updateComponentFrame(child, device, {
      x: containerFrame.x + row.x,
      y: containerFrame.y + row.y,
      width: row.width,
      height: row.height
    });
    return child;
  });
}

const deviceOptions: Array<{ device: WebDesignDevice; label: string; icon: string }> = [
  { device: 'desktop', label: '桌面', icon: '▰' },
  { device: 'tablet', label: '平板', icon: '▯' },
  { device: 'mobile', label: '手机', icon: '▯' }
];

type Interaction = {
  kind: 'move' | 'resize';
  componentId: string;
  pointerX: number;
  pointerY: number;
  frame: ResolvedWebDesignComponent;
  selectedIds: string[];
  snapshot: WebDesignDocument;
  scale: number;
  scoped: boolean;
};

type LayerAction = 'front' | 'forward' | 'backward' | 'back';
type AlignAction = 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom';
type LibraryTab = 'components' | 'antd' | 'blocks' | 'layers';
type EditingSlot = { componentId: string; slotId: string };

export function WebDesignStudioApp() {
  const [repository, setRepository] = useState<DesignRepository>();
  const [documents, setDocuments] = useState<DesignSummary[]>([]);
  const [document, setDocument] = useState<WebDesignDocument>();
  const [persistedRevision, setPersistedRevision] = useState(0);
  const [selectedId, setSelectedId] = useState<string>();
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [pageId, setPageId] = useState('home');
  const [clipboard, setClipboard] = useState<{ document: WebDesignDocument; componentIds: string[] }>();
  const [snapGuides, setSnapGuides] = useState<SnapGuides>({});
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [preview, setPreview] = useState(false);
  const [interactionMode, setInteractionMode] = useState(false);
  const [device, setDevice] = useState<WebDesignDevice>('desktop');
  const [zoom, setZoom] = useState(0.82);
  const [past, setPast] = useState<WebDesignDocument[]>([]);
  const [future, setFuture] = useState<WebDesignDocument[]>([]);
  const [toast, setToast] = useState<string>();
  const [annotationText, setAnnotationText] = useState('');
  const [aiInstruction, setAiInstruction] = useState('');
  const [paletteQuery, setPaletteQuery] = useState('');
  const [libraryTab, setLibraryTab] = useState<LibraryTab>('antd');
  const [variantPickerId, setVariantPickerId] = useState<string>();
  const [themePickerOpen, setThemePickerOpen] = useState(false);
  const [aiPanelOpen, setAiPanelOpen] = useState(false);
  const [editingSlot, setEditingSlot] = useState<EditingSlot>();
  const interaction = useRef<Interaction | undefined>(undefined);
  const documentRef = useRef<WebDesignDocument | undefined>(undefined);
  const assetInput = useRef<HTMLInputElement | null>(null);
  const canvasScroll = useRef<HTMLDivElement | null>(null);
  const previewZoom = useRef(zoom);
  const interactionZoom = useRef(zoom);

  useEffect(() => { documentRef.current = document; }, [document]);

  const selected = useMemo(
    () => document?.components.find((component) => component.id === selectedId),
    [document, selectedId]
  );
  const selectedFrame = useMemo(
    () => selected ? resolveComponent(selected, device) : undefined,
    [selected, device]
  );
  const selectedIdSet = useMemo(() => new Set(selectedIds), [selectedIds]);
  const breakpoint = useMemo(
    () => document ? breakpointFor(document, device) : { width: 1200, height: 940 },
    [document, device]
  );
  const pages = useMemo(() => document ? pagesForDocument(document) : [], [document]);
  const tokens = useMemo(() => document ? tokensForDocument(document) : undefined, [document]);
  const currentPage = useMemo(() => pages.find((page) => page.id === pageId) ?? pages[0], [pages, pageId]);
  const pageComponents = useMemo(() => document && currentPage ? componentsForPage(document, currentPage.id) : [], [document, currentPage]);
  const editingContainer = useMemo(() => editingSlot ? document?.components.find((component) => component.id === editingSlot.componentId) : undefined, [document, editingSlot]);
  const editingSlotDefinition = useMemo(() => editingContainer && editingSlot
    ? editableSlotsForAntdComponent(editingContainer).find((slot) => slot.id === editingSlot.slotId)
    : undefined, [editingContainer, editingSlot]);
  const editingSlotComponents = useMemo(() => document && editingSlot
    ? componentsInSlot(document, editingSlot.componentId, editingSlot.slotId)
    : [], [document, editingSlot]);
  const editingVisibleComponents = useMemo(() => document && editingSlot
    ? visibleComponentsInSlot(document, editingSlot.componentId, editingSlot.slotId)
    : [], [document, editingSlot]);
  const inspectedFrame = useMemo(() => {
    if (!selectedFrame || !editingContainer || !editingSlot || !selected || slotIdForDescendant(document!, selected, editingContainer.id) !== editingSlot.slotId) return selectedFrame;
    const containerFrame = resolveComponent(editingContainer, device);
    return { ...selectedFrame, x: selectedFrame.x - containerFrame.x, y: selectedFrame.y - containerFrame.y };
  }, [selectedFrame, editingContainer, editingSlot, selected, document, device]);

  useEffect(() => {
    void (async () => {
      const repo = await createRepository();
      setRepository(repo);
      let items = await repo.list();
      let first: WebDesignDocument;
      if (items.length === 0) {
        first = await repo.create('AI 产品落地页');
        items = await repo.list();
      } else {
        first = await repo.read(items[0].documentId);
      }
      setDocuments(items);
      openDocument(first);
    })().catch((error) => showToast(error instanceof Error ? error.message : String(error)));
  }, []);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const active = interaction.current;
      if (!active) return;
      const dx = (event.clientX - active.pointerX) / active.scale;
      const dy = (event.clientY - active.pointerY) / active.scale;
      if (active.kind === 'move') {
        const movingIds = selectedRootIds(active.snapshot, active.selectedIds).flatMap((id) => [id, ...descendantIds(active.snapshot, id)]);
        const candidate = { ...active.frame, x: active.frame.x + dx, y: active.frame.y + dy };
        const snapped = active.scoped
          ? { frame: candidate, guides: {} as SnapGuides }
          : snapComponentFrame(active.snapshot, active.componentId, device, candidate, movingIds);
        setSnapGuides(snapped.guides);
        changeLive(() => {
          const moved = moveComponentsWithDescendants(
            active.snapshot,
            active.selectedIds,
            device,
            snapped.frame.x - active.frame.x,
            snapped.frame.y - active.frame.y
          );
          const moving = new Set(movingIds);
          return { ...moved, components: moved.components.map((component) => moving.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
        });
      } else {
        changeLive(() => ({
          ...active.snapshot,
          components: active.snapshot.components.map((component) => component.id === active.componentId
            ? setSymbolOverride(updateComponentFrame(component, device, {
              width: Math.max(24, Math.round(active.frame.width + dx)),
              height: Math.max(24, Math.round(active.frame.height + dy))
            }), 'frame', true)
            : component)
        }));
      }
    };
    const onUp = () => {
      const active = interaction.current;
      if (!active) return;
      setPast((items) => [...items.slice(-59), active.snapshot]);
      setFuture([]);
      interaction.current = undefined;
      setSnapGuides({});
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
  }, [device, zoom]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select, [contenteditable="true"]')) return;
      const command = event.metaKey || event.ctrlKey;
      if (event.key === 'Escape' && preview) {
        event.preventDefault();
        toggleFullPreview();
      } else if (event.key === 'Escape' && interactionMode) {
        event.preventDefault();
        toggleInteractionMode();
      } else if (command && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void save();
      } else if (command && !event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        undo();
      } else if (command && event.shiftKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        redo();
      } else if (command && event.key.toLowerCase() === 'd') {
        event.preventDefault();
        duplicateSelected();
      } else if (command && event.key.toLowerCase() === 'c' && selectedIds.length > 0) {
        event.preventDefault();
        copySelected();
      } else if (command && event.key.toLowerCase() === 'v' && clipboard) {
        event.preventDefault();
        pasteClipboard();
      } else if ((event.key === 'Backspace' || event.key === 'Delete') && selectedIds.length > 0) {
        event.preventDefault();
        deleteSelected();
      } else if (selectedIds.length > 0 && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
        event.preventDefault();
        const amount = event.shiftKey ? 10 : 1;
        const dx = event.key === 'ArrowLeft' ? -amount : event.key === 'ArrowRight' ? amount : 0;
        const dy = event.key === 'ArrowUp' ? -amount : event.key === 'ArrowDown' ? amount : 0;
        nudgeSelected(dx, dy);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [selectedIds, past, future, dirty, saving, repository, persistedRevision, device, clipboard, pageId, preview, interactionMode, zoom, breakpoint.width]);

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast((current) => current === message ? undefined : current), 2600);
  }

  function setCurrent(next: WebDesignDocument) {
    documentRef.current = next;
    setDocument(next);
  }

  function openDocument(next: WebDesignDocument) {
    setCurrent(next);
    setPersistedRevision(next.revision);
    setSelectedId(undefined);
    setSelectedIds([]);
    setDirty(false);
    setPast([]);
    setFuture([]);
    setDevice('desktop');
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

  function changeLive(updater: (current: WebDesignDocument) => WebDesignDocument) {
    const current = documentRef.current;
    if (!current) return;
    setCurrent(updater(current));
    setDirty(true);
  }

  function historyDocument(snapshot: WebDesignDocument, current: WebDesignDocument): WebDesignDocument {
    return { ...structuredClone(snapshot), revision: current.revision, createdAt: current.createdAt, updatedAt: current.updatedAt };
  }

  function undo() {
    const current = documentRef.current;
    const previous = past[past.length - 1];
    if (!current || !previous) return;
    setPast((items) => items.slice(0, -1));
    setFuture((items) => [structuredClone(current), ...items].slice(0, 60));
    setCurrent(historyDocument(previous, current));
    setDirty(true);
  }

  function redo() {
    const current = documentRef.current;
    const next = future[0];
    if (!current || !next) return;
    setFuture((items) => items.slice(1));
    setPast((items) => [...items.slice(-59), structuredClone(current)]);
    setCurrent(historyDocument(next, current));
    setDirty(true);
  }

  function updateComponent(componentId: string, updater: (component: WebDesignComponent) => WebDesignComponent) {
    commit((current) => ({ ...current, components: current.components.map((component) => component.id === componentId ? updater(component) : component) }));
  }

  async function save(force = false, silent = false) {
    const current = documentRef.current;
    if (!repository || !current || saving || (!dirty && !force)) return;
    setSaving(true);
    try {
      const saved = await repository.save(current, persistedRevision);
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
    if (!repository) return;
    try {
      const created = await repository.create(`网站设计 ${documents.length + 1}`);
      setDocuments(await repository.list());
      openDocument(created);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  }

  function onPaletteDrag(event: DragEvent, shapeId: BasicShapeId) {
    event.dataTransfer.setData('application/x-web-design-shape', shapeId);
    event.dataTransfer.effectAllowed = 'copy';
  }

  function onAntdDrag(event: DragEvent, definitionId: string) {
    event.dataTransfer.setData('application/x-web-design-antd', definitionId);
    event.dataTransfer.effectAllowed = 'copy';
  }

  function addAntdComponent(definitionId: string, x: number, y: number, variantId?: string, targetSlot = editingSlot) {
    const current = documentRef.current;
    if (!current) return;
    const container = targetSlot ? current.components.find((candidate) => candidate.id === targetSlot.componentId) : undefined;
    const containerFrame = container ? resolveComponent(container, device) : undefined;
    const componentX = containerFrame ? containerFrame.x + x : x;
    const componentY = containerFrame ? containerFrame.y + y : y;
    let component = createAntdComponent(definitionId, componentX, componentY);
    if (variantId) component = applyAntdComponentVariant(component, variantId);
    component.pageId = pageId;
    if (container && targetSlot) {
      component.parentId = container.id;
      component.slot = targetSlot.slotId;
      component.zIndex = Math.max(0, ...current.components.filter((item) => item.parentId === container.id && item.slot === targetSlot.slotId).map((item) => item.zIndex)) + 1;
    } else {
      component.zIndex = Math.max(1, ...componentsForPage(current, pageId).filter((item) => !contentContainerAncestor(current, item)).map((item) => item.zIndex)) + 1;
    }
    if (device !== 'desktop') component = updateComponentFrame(component, device, { x: componentX, y: componentY });
    const starterSlot = !container && (definitionId === 'Drawer' || definitionId === 'Modal') ? editableSlotsForAntdComponent(component)[0] : undefined;
    const starter = starterSlot ? createSlotStarterComponents(component, starterSlot, 'form', pageId, device) : [];
    commit((active) => ({ ...active, components: [...active.components, component, ...starter] }));
    setSelectedId(component.id);
    setSelectedIds([component.id]);
    showToast(container ? `已添加到${editableSlotsForAntdComponent(container).find((slot) => slot.id === targetSlot?.slotId)?.label ?? '组件内容'}` : `已插入 Ant Design ${component.library?.component}`);
  }

  function insertAntdComponent(definitionId: string, variantId?: string) {
    const definition = ANTD_COMPONENTS.find((candidate) => candidate.id === definitionId);
    if (!definition) return;
    if (editingSlotDefinition) {
      addAntdComponent(definitionId, Math.max(12, Math.round((editingSlotDefinition.width - definition.width) / 2)), 28, variantId);
    } else {
      addAntdComponent(definitionId, Math.max(24, Math.round((breakpoint.width - definition.width) / 2)), 80, variantId);
    }
    setVariantPickerId(undefined);
  }

  function editComponentSlot(component: WebDesignComponent, slotId: string) {
    if (interactionMode) setInteractionMode(false);
    setEditingSlot({ componentId: component.id, slotId });
    const first = documentRef.current ? componentsInSlot(documentRef.current, component.id, slotId)[0] : undefined;
    setSelectedId(first?.id);
    setSelectedIds(first ? [first.id] : []);
  }

  function exitSlotEditor() {
    const containerId = editingSlot?.componentId;
    setEditingSlot(undefined);
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
    const containerFrame = resolveComponent(editingContainer, device);
    const contentBottom = Math.max(...adjusted.map((component) => {
      const frame = resolveComponent(component, device);
      return frame.y - containerFrame.y + frame.height;
    }));
    const shouldGrowContainer = !['Drawer', 'Modal', 'Popover'].includes(editingContainer.library?.component ?? '');
    const grownContainer = shouldGrowContainer
      ? updateComponentFrame(editingContainer, device, { height: Math.max(containerFrame.height, Math.ceil(contentBottom + 68)) })
      : editingContainer;
    commit((active) => ({
      ...active,
      components: [...active.components.map((component) => component.id === grownContainer.id ? grownContainer : component), ...adjusted]
    }));
    setSelectedId(adjusted[0]?.id);
    setSelectedIds(adjusted[0] ? [adjusted[0].id] : []);
    showToast(template === 'form' ? '已插入可编辑表单' : '已插入可编辑详情内容');
  }

  function onCanvasDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    const current = documentRef.current;
    if (!current || preview || interactionMode) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const activeScale = editingSlot ? 1 : zoom;
    const x = Math.round((event.clientX - bounds.left) / activeScale);
    const y = Math.round((event.clientY - bounds.top) / activeScale);
    const antdDefinitionId = event.dataTransfer.getData('application/x-web-design-antd');
    if (antdDefinitionId && ANTD_COMPONENTS.some((item) => item.id === antdDefinitionId)) {
      addAntdComponent(antdDefinitionId, x, y);
      return;
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
    commit((active) => ({ ...active, components: [...active.components, component] }));
    setSelectedId(component.id);
    setSelectedIds([component.id]);
  }

  function insertBlockPreset(presetId: WebDesignBlockPresetId) {
    const current = documentRef.current;
    if (!current) return;
    const block = createBlockPreset(current, pageId, presetId);
    commit((active) => {
      const breakpoints = structuredClone(active.breakpoints ?? {
        desktop: { width: active.viewport.width, height: active.viewport.height },
        tablet: { width: 768, height: 1100 },
        mobile: { width: 390, height: 844 }
      });
      for (const target of ['desktop', 'tablet', 'mobile'] as const) {
        const bottom = Math.max(...block.components.map((component) => {
          const frame = resolveComponent(component, target);
          return frame.y + frame.height;
        }));
        breakpoints[target].height = Math.max(breakpoints[target].height, Math.ceil(bottom + 40));
      }
      return {
        ...active,
        breakpoints,
        viewport: { ...active.viewport, height: breakpoints.desktop.height },
        components: [...active.components, ...block.components]
      };
    });
    setSelectedId(block.rootIds[0]);
    setSelectedIds(block.rootIds);
    showToast(`已插入${WEB_DESIGN_BLOCK_PRESETS.find((preset) => preset.id === presetId)?.name ?? '成品区块'}`);
  }

  function applyPageTemplate(templateId: WebDesignPageTemplateId) {
    const current = documentRef.current;
    if (!current) return;
    const existingIds = new Set(componentsForPage(current, pageId).map((component) => component.id));
    if (existingIds.size > 0 && !window.confirm('应用页面模板会替换当前页面的全部组件，是否继续？')) return;
    const template = createPageTemplate(current, pageId, templateId);
    commit((active) => {
      const nextComponents = [...active.components.filter((component) => !existingIds.has(component.id)), ...template.components];
      const breakpoints = structuredClone(active.breakpoints ?? {
        desktop: { width: active.viewport.width, height: active.viewport.height },
        tablet: { width: 768, height: 1100 },
        mobile: { width: 390, height: 844 }
      });
      for (const target of ['desktop', 'tablet', 'mobile'] as const) {
        const bottom = Math.max(...template.components.map((component) => {
          const frame = resolveComponent(component, target);
          return frame.y + frame.height;
        }));
        breakpoints[target].height = Math.max(640, Math.ceil(bottom + 60));
      }
      return {
        ...active,
        breakpoints,
        viewport: { ...active.viewport, height: breakpoints.desktop.height },
        components: nextComponents,
        requests: active.requests.filter((request) => !request.componentId || !existingIds.has(request.componentId))
      };
    });
    setSelectedId(undefined);
    setSelectedIds([]);
    showToast(`已应用${WEB_DESIGN_PAGE_TEMPLATES.find((template) => template.id === templateId)?.name ?? '页面模板'}`);
  }

  function beginInteraction(event: ReactPointerEvent, component: WebDesignComponent, kind: Interaction['kind']) {
    if (preview || interactionMode) return;
    event.preventDefault();
    event.stopPropagation();
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
      snapshot: structuredClone(current)
      , scale: editingSlot ? 1 : zoom
      , scoped: Boolean(editingSlot)
    };
  }

  function updateSelected(changes: Partial<WebDesignComponent>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({ ...component, ...changes }, 'content', true));
  }

  function updateSelectedFrame(changes: Partial<Pick<ResolvedWebDesignComponent, 'x' | 'y' | 'width' | 'height' | 'hidden'>>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride(updateComponentFrame(component, device, changes), 'frame', true));
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
    updateComponent(selected.id, (component) => setSymbolOverride(updateComponentStyle(component, device, changes), 'style', true));
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
    if (editingSlot && removed.has(editingSlot.componentId)) setEditingSlot(undefined);
    setSelectedIds([]);
  }

  function duplicateSelected() {
    const current = documentRef.current;
    if (selectedIds.length === 0 || !current) return;
    const cloned = cloneComponentSubtrees(current, selectedIds, pageId, 20, current);
    commit((active) => ({ ...active, components: [...active.components, ...cloned.components] }));
    setSelectedId(cloned.rootIds[0]);
    setSelectedIds(cloned.rootIds);
  }

  function copySelected() {
    const current = documentRef.current;
    if (!current || selectedIds.length === 0) return;
    setClipboard({ document: structuredClone(current), componentIds: [...selectedIds] });
    showToast(`已复制 ${selectedIds.length} 个组件`);
  }

  function pasteClipboard() {
    const current = documentRef.current;
    if (!current || !clipboard) return;
    const cloned = cloneComponentSubtrees(clipboard.document, clipboard.componentIds, pageId, 20, current);
    commit((active) => ({ ...active, components: [...active.components, ...cloned.components] }));
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
    const targetWidth = editingSlotDefinition?.width ?? breakpoint.width;
    const targetHeight = editingSlotDefinition?.height ?? breakpoint.height;
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
    commit((current) => {
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

  function updateBreakpoint(width: number, height: number) {
    commit((current) => {
      const breakpoints = {
        desktop: current.breakpoints?.desktop ?? { width: current.viewport.width, height: current.viewport.height },
        tablet: current.breakpoints?.tablet ?? { width: 768, height: 1100 },
        mobile: current.breakpoints?.mobile ?? { width: 390, height: 844 }
      };
      breakpoints[device] = { width, height };
      return { ...current, breakpoints, viewport: device === 'desktop' ? { ...current.viewport, width, height } : current.viewport };
    });
  }

  function switchDevice(next: WebDesignDevice) {
    setDevice(next);
    setSelectedId(undefined);
    setSelectedIds([]);
    setZoom(next === 'desktop' ? .82 : next === 'tablet' ? .72 : .9);
  }

  function fitCanvasWidth() {
    const scroller = canvasScroll.current;
    if (!scroller) return;
    const nextZoom = Math.max(.25, Math.min(1, (scroller.clientWidth - 120) / breakpoint.width));
    setZoom(nextZoom);
    window.setTimeout(() => scroller.scrollTo({ left: 0, top: 0, behavior: 'smooth' }), 0);
  }

  function toggleFullPreview() {
    if (preview) {
      setPreview(false);
      setZoom(previewZoom.current);
      return;
    }
    previewZoom.current = interactionMode ? interactionZoom.current : zoom;
    if (interactionMode) setInteractionMode(false);
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
    setPreview(true);
    window.setTimeout(() => {
      const scroller = canvasScroll.current;
      if (!scroller) return;
      setZoom(Math.max(.25, Math.min(1.5, scroller.clientWidth / breakpoint.width)));
      scroller.scrollTo({ left: 0, top: 0 });
    }, 0);
  }

  function toggleInteractionMode() {
    if (interactionMode) {
      setInteractionMode(false);
      setZoom(interactionZoom.current);
      return;
    }
    interactionZoom.current = zoom;
    setSelectedId(undefined);
    setSelectedIds([]);
    setInteractionMode(true);
    window.setTimeout(() => fitCanvasWidth(), 0);
  }

  function selectComponent(componentId: string, additive = false) {
    const current = documentRef.current;
    const component = current?.components.find((candidate) => candidate.id === componentId);
    const container = current && component ? contentContainerAncestor(current, component) : undefined;
    const slotId = current && component && container ? slotIdForDescendant(current, component, container.id) : undefined;
    if (container && slotId && (editingSlot?.componentId !== container.id || editingSlot.slotId !== slotId)) {
      setEditingSlot({ componentId: container.id, slotId });
    }
    if (!additive) {
      setSelectedId(componentId);
      setSelectedIds([componentId]);
      return;
    }
    const next = selectedIds.includes(componentId) ? selectedIds.filter((id) => id !== componentId) : [...selectedIds, componentId];
    setSelectedIds(next);
    setSelectedId(next.includes(componentId) ? componentId : next[0]);
  }

  function groupSelected() {
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
    group.name = '组件组合';
    group.pageId = pageId;
    group.width = desktopBounds.width;
    group.height = desktopBounds.height;
    group.parentId = parentIds.size === 1 ? components[0].parentId : undefined;
    group.slot = slotIds.size === 1 ? components[0].slot : undefined;
    group.zIndex = Math.max(0, Math.min(...components.map((component) => component.zIndex)) - 1);
    group.layout = { mode: 'free', gap: 16, padding, align: 'start' };
    group.responsive = { tablet: tabletBounds, mobile: mobileBounds };
    commit((active) => ({
      ...active,
      components: [...active.components.map((component) => roots.includes(component.id) ? { ...component, parentId: group.id } : component), group]
    }));
    setSelectedId(group.id);
    setSelectedIds([group.id]);
  }

  function ungroupSelected() {
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
  }

  function updateSelectedLayout(changes: Partial<NonNullable<WebDesignComponent['layout']>>) {
    if (!selected) return;
    updateComponent(selected.id, (component) => setSymbolOverride({
      ...component,
      layout: { mode: 'free', gap: 16, padding: 16, align: 'start', ...component.layout, ...changes }
    }, 'frame', true));
  }

  function applySelectedAutoLayout() {
    if (!selected) return;
    commit((current) => {
      const changedIds = new Set(current.components.filter((component) => component.parentId === selected.id)
        .flatMap((component) => [component.id, ...descendantIds(current, component.id)]));
      const laidOut = autoLayoutContainer(current, selected.id, device);
      return { ...laidOut, components: laidOut.components.map((component) => changedIds.has(component.id) ? setSymbolOverride(component, 'frame', true) : component) };
    });
  }

  function saveSelectionAsSymbol() {
    const current = documentRef.current;
    if (!current || selectedIds.length === 0) return;
    const symbol = createSymbolFromSelection(current, selectedIds, `${selected?.name ?? '组合'} 组件`);
    commit((active) => ({ ...active, symbols: [...(active.symbols ?? []), symbol] }));
    showToast(`已保存到组件库：${symbol.name}`);
  }

  function insertSymbol(symbol: WebDesignSymbol) {
    const current = documentRef.current;
    if (!current) return;
    const instance = instantiateSymbol(current, symbol, pageId);
    commit((active) => {
      const breakpoints = structuredClone(active.breakpoints ?? {
        desktop: { width: active.viewport.width, height: active.viewport.height },
        tablet: { width: 768, height: 1100 },
        mobile: { width: 390, height: 844 }
      });
      for (const target of ['desktop', 'tablet', 'mobile'] as const) {
        const bottom = Math.max(...instance.components.map((component) => {
          const frame = resolveComponent(component, target);
          return frame.y + frame.height;
        }));
        breakpoints[target].height = Math.max(breakpoints[target].height, Math.ceil(bottom + 40));
      }
      return {
        ...active,
        breakpoints,
        viewport: { ...active.viewport, height: breakpoints.desktop.height },
        components: [...active.components, ...instance.components]
      };
    });
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

  function removeSymbol(symbolId: string) {
    commit((current) => ({
      ...current,
      symbols: (current.symbols ?? []).filter((symbol) => symbol.id !== symbolId),
      components: current.components.map((component) => component.symbolId === symbolId ? {
        ...component,
        symbolId: undefined,
        symbolInstanceId: undefined,
        symbolComponentId: undefined,
        symbolOverrides: undefined
      } : component)
    }));
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

  function applySelectedAntdVariant(variantId: string) {
    if (!selected?.library || selected.library.name !== 'antd') return;
    updateComponent(selected.id, (component) => applyAntdComponentVariant(component, variantId));
  }

  function detachSelectedSymbol() {
    if (!selected?.symbolInstanceId) return;
    commit((current) => detachSymbolInstance(current, selected.id));
    showToast('当前实例已脱离组件库');
  }

  function updateTokens(updater: (tokens: WebDesignTokens) => WebDesignTokens) {
    commit((current) => ({ ...current, tokens: updater(structuredClone(tokensForDocument(current))) }));
  }

  function applyDesignTheme(preset: WebDesignThemePreset) {
    commit((current) => ({
      ...current,
      viewport: { ...current.viewport, background: preset.canvasBackground },
      tokens: structuredClone(preset.tokens)
    }));
    setThemePickerOpen(false);
    showToast(`已应用 ${preset.name} 视觉风格`);
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

  function switchPage(nextPageId: string) {
    setPageId(nextPageId);
    setEditingSlot(undefined);
    setSelectedId(undefined);
    setSelectedIds([]);
  }

  function addPage() {
    const current = documentRef.current;
    if (!current) return;
    const currentPages = pagesForDocument(current);
    const index = currentPages.length + 1;
    const id = `page-${crypto.randomUUID().slice(0, 8)}`;
    const page = { id, name: `页面 ${index}`, slug: `/page-${index}` };
    commit((active) => ({ ...active, pages: [...pagesForDocument(active), page] }));
    switchPage(id);
  }

  function duplicatePage() {
    const current = documentRef.current;
    if (!current || !currentPage) return;
    const id = `page-${crypto.randomUUID().slice(0, 8)}`;
    const page = { id, name: `${currentPage.name} 副本`, slug: `/page-${pagesForDocument(current).length + 1}` };
    const sourceIds = componentsForPage(current, currentPage.id).map((component) => component.id);
    const cloned = cloneComponentSubtrees(current, sourceIds, id, 0, current);
    commit((active) => ({ ...active, pages: [...pagesForDocument(active), page], components: [...active.components, ...cloned.components] }));
    switchPage(id);
  }

  function deleteCurrentPage() {
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
    switchPage(remainingPages[0].id);
  }

  function updateCurrentPage(changes: Partial<{ name: string; slug: string }>) {
    if (!currentPage) return;
    commit((current) => ({
      ...current,
      pages: pagesForDocument(current).map((page) => page.id === currentPage.id ? { ...page, ...changes } : page)
    }));
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
      if (target) switchPage(target.id);
      return;
    }
    window.open(component.interaction.target, '_blank', 'noopener,noreferrer');
  }

  function addAnnotation() {
    if (!selected || !annotationText.trim()) return;
    const annotation = { id: `note-${crypto.randomUUID().slice(0, 8)}`, text: annotationText.trim(), status: 'open' as const, createdAt: new Date().toISOString() };
    updateComponent(selected.id, (component) => ({ ...component, annotations: [...component.annotations, annotation] }));
    setAnnotationText('');
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
    setAiPanelOpen(false);
    await save(true, true);
    showToast(target ? '已提交组件修改任务' : '已提交整页设计任务');
  }

  if (!document) return <div className="loading-screen"><div className="loading-dot" />正在打开 Web Design Studio…</div>;

  const layerComponents = flattenComponentTree(document, pageId);
  const directChildCount = selected ? document.components.filter((component) => component.parentId === selected.id).length : 0;
  const canUngroup = Boolean(selected?.id.startsWith('group-') && directChildCount > 0);
  const selectedSymbol = selected?.symbolId ? document.symbols?.find((symbol) => symbol.id === selected.symbolId) : undefined;
  const selectedAntdVariants = selected?.library?.name === 'antd' ? variantsForAntdComponent(selected.library.component) : [];
  const selectedEditableSlots = selected ? editableSlotsForAntdComponent(selected) : [];
  const aiTarget = selected ?? editingContainer;
  const normalizedPaletteQuery = paletteQuery.trim().toLowerCase();
  const filteredPalette = palette.filter((item) => !normalizedPaletteQuery
    || `${item.label} ${item.id} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredPresets = WEB_DESIGN_BLOCK_PRESETS.filter((preset) => !normalizedPaletteQuery
    || `${preset.name} ${preset.description} ${preset.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const filteredAntdComponents = ANTD_COMPONENTS.filter((item) => !normalizedPaletteQuery
    || `${item.id} ${item.label} ${item.keywords.join(' ')}`.toLowerCase().includes(normalizedPaletteQuery));
  const variantPickerDefinition = variantPickerId ? ANTD_COMPONENTS.find((item) => item.id === variantPickerId) : undefined;
  const variantPickerVariants = variantPickerDefinition ? variantsForAntdComponent(variantPickerDefinition.id) : [];
  const aiQuickPrompts = aiTarget
    ? ['让这个组件更精致、更有层次', '优化尺寸、间距和对齐', '给我 3 个更好看的视觉方案']
    : ['设计一个像 Apple 官网一样克制高级的页面', '统一整页的字号、间距、圆角和色彩', '检查并修复页面中不协调的视觉细节'];

  return (
    <div className={`studio-shell ${preview ? 'preview-active' : ''}`}>
      <header className="topbar">
        <div className="brand"><span className="brand-mark">W</span><span>Web Design Studio</span></div>
        <select className="document-select" value={document.documentId} onChange={(event) => {
          const id = event.target.value;
          if (!repository || (dirty && !window.confirm('切换设计会丢弃未保存修改，确定继续吗？'))) return;
          void repository.read(id).then(openDocument).catch((error) => showToast(String(error)));
        }}>
          {documents.map((item) => <option key={item.documentId} value={item.documentId}>{item.title}</option>)}
        </select>
        <button className="quiet-button" onClick={() => void createNew()}>新建设计</button>
        <button className="quiet-button style-trigger" onClick={() => setThemePickerOpen(true)}>设计风格</button>
        <div className="history-tools">
          <button title="撤销 ⌘Z" disabled={past.length === 0} onClick={undo}>↶</button>
          <button title="重做 ⇧⌘Z" disabled={future.length === 0} onClick={redo}>↷</button>
        </div>
        <div className="topbar-spacer" />
        <span className={`service-pill ${repository?.mode === 'server' ? 'online' : ''}`}>{repository?.mode === 'server' ? '本地服务' : '浏览器存储'}</span>
        <details className="delivery-menu"><summary>交付</summary><div><button onClick={exportCurrentPage}>HTML</button><button onClick={exportReact}>React</button><button onClick={exportVue}>Vue</button></div></details>
        <button className="quiet-button" onClick={() => void refresh()}>刷新</button>
        <button className={`quiet-button ${preview ? 'active' : ''}`} onClick={toggleFullPreview}>{preview ? '退出预览' : '全屏预览'}</button>
        <button className="ai-design-trigger" onClick={() => setAiPanelOpen(true)}>✦ AI 设计</button>
        <button className="primary-button" disabled={!dirty || saving} onClick={() => void save()}>{saving ? '保存中…' : dirty ? '保存' : '已保存'}</button>
      </header>

      <main className={`workspace ${preview ? 'preview-mode' : ''}`}>
        {!preview && <aside className="palette-panel">
          <div className="library-tabs">
            <button className={libraryTab === 'antd' ? 'active' : ''} onClick={() => setLibraryTab('antd')}>AntD</button>
            <button className={libraryTab === 'components' ? 'active' : ''} onClick={() => setLibraryTab('components')}>图形</button>
            <button className={libraryTab === 'blocks' ? 'active' : ''} onClick={() => setLibraryTab('blocks')}>区块</button>
            <button className={libraryTab === 'layers' ? 'active' : ''} onClick={() => setLibraryTab('layers')}>图层</button>
          </div>
          <div className="palette-panel-content">
            {libraryTab !== 'layers' && <input className="component-search" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} placeholder={libraryTab === 'components' ? '搜索基础图形…' : libraryTab === 'antd' ? '搜索 Ant Design 组件…' : '搜索成品区块…'} />}

            {libraryTab === 'components' && <>
              <div className="panel-intro"><strong>基础图形</strong><span>只保留矩形、圆形和直线；产品组件统一使用成熟 UI 库</span></div>
              <div className="palette-grid shapes-grid">{filteredPalette.map((item) => <div key={item.id} className="palette-item" draggable onDragStart={(event) => onPaletteDrag(event, item.id)}><span className="palette-icon">{item.icon}</span><span>{item.label}</span></div>)}</div>
            </>}

            {libraryTab === 'antd' && <>
              <div className="antd-library-heading"><div className="antd-logo-mark">A</div><div><strong>Ant Design</strong><span>实际组件库 · v{ANTD_VERSION}</span></div></div>
              <div className="panel-intro"><strong>Ant Design 组件总览</strong><span>点击先预览不同款式，拖拽则直接插入默认款</span></div>
              {ANTD_CATEGORIES.map((category) => {
                const items = filteredAntdComponents.filter((item) => item.category === category);
                return items.length > 0 && <div key={category} className="antd-category"><div className="antd-category-title">{category}</div><div className="antd-component-list">
                  {items.map((item) => <button key={item.id} draggable onDragStart={(event) => onAntdDrag(event, item.id)} onClick={() => setVariantPickerId(item.id)}><span className="antd-list-icon">{item.icon}</span><strong>{item.id}</strong><small>{item.label}</small><em>{(ANTD_COMPONENT_VARIANTS[item.id]?.length ?? 1) > 1 ? `${ANTD_COMPONENT_VARIANTS[item.id].length} 款` : '预览'}</em><b>›</b></button>)}
                </div></div>;
              })}
            </>}

            {libraryTab === 'blocks' && <>
              <div className="panel-intro"><strong>页面模板</strong><span>一键替换当前页，得到可继续编辑的完整网站结构</span></div>
              <div className="page-template-list">{WEB_DESIGN_PAGE_TEMPLATES.map((template) => <button key={template.id} onClick={() => applyPageTemplate(template.id)}><span>{template.icon}</span><div><strong>{template.name}</strong><small>{template.description}</small></div></button>)}</div>
              <div className="panel-title section-title">成品区块</div>
              <div className="block-preset-list">{filteredPresets.map((preset) => <button key={preset.id} onClick={() => insertBlockPreset(preset.id)}><span>{preset.icon}</span><div><strong>{preset.name}</strong><small>{preset.description}</small></div><b>＋</b></button>)}</div>
              <div className="panel-title section-title layer-title"><span>我的组件</span><small>{document.symbols?.length ?? 0}</small></div>
              {selectedIds.length > 0 && <button className="secondary-button" onClick={saveSelectionAsSymbol}>将选中项保存为组件</button>}
              <div className="symbol-list">{(document.symbols ?? []).map((symbol) => <div key={symbol.id} className="symbol-row"><button className="symbol-insert" onClick={() => insertSymbol(symbol)}><span>◇</span><strong>{symbol.name}</strong><small>{symbol.components.length} 层</small></button><button className="symbol-remove" title="移出组件库" onClick={() => removeSymbol(symbol.id)}>×</button></div>)}</div>
            </>}

            {libraryTab === 'layers' && <>
              <div className="panel-title layer-title"><span>页面图层</span><small>{pageComponents.length}</small></div>
              <div className="layers-list expanded">
                {layerComponents.map(({ component, depth }) => {
                  const resolved = resolveComponent(component, device);
                  return <div key={component.id} className={`layer-row ${selectedIdSet.has(component.id) ? 'selected' : ''} ${resolved.hidden ? 'hidden' : ''}`} style={{ paddingLeft: 4 + depth * 14 }} onClick={(event) => selectComponent(component.id, event.shiftKey)}>
                    <button title={resolved.hidden ? '显示' : '隐藏'} onClick={(event) => { event.stopPropagation(); toggleHidden(component); }}>{resolved.hidden ? '○' : '●'}</button>
                    <span className="layer-type">{component.library?.name === 'antd' ? ANTD_COMPONENTS.find((item) => item.id === component.library?.component)?.icon : palette.find((item) => item.type === component.type)?.icon}</span>
                    <span className="layer-name">{depth > 0 ? '└ ' : ''}{component.name}</span>
                    <button title={component.locked ? '解锁' : '锁定'} onClick={(event) => { event.stopPropagation(); toggleLocked(component); }}>{component.locked ? '🔒' : '⌁'}</button>
                  </div>;
                })}
              </div>

              <div className="panel-title section-title">页面设置</div>
              <label className="field-label">当前页面<select value={currentPage?.id} onChange={(event) => switchPage(event.target.value)}>{pages.map((page) => <option key={page.id} value={page.id}>{page.name}</option>)}</select></label>
              <div className="page-actions"><button onClick={addPage}>＋ 新建</button><button onClick={duplicatePage}>复制页</button><button disabled={pages.length <= 1} onClick={deleteCurrentPage}>删除</button></div>
              {currentPage && <><label className="field-label">页面名称<input value={currentPage.name} onChange={(event) => updateCurrentPage({ name: event.target.value })} /></label><label className="field-label page-slug">路径<input value={currentPage.slug} onChange={(event) => updateCurrentPage({ slug: event.target.value })} /></label></>}
              <label className="field-label">网站标题<input value={document.title} onChange={(event) => commit((current) => ({ ...current, title: event.target.value }))} /></label>
              <div className="size-row"><NumberField label={`${device} 宽`} value={breakpoint.width} onChange={(width) => updateBreakpoint(width, breakpoint.height)} /><NumberField label="高" value={breakpoint.height} onChange={(height) => updateBreakpoint(breakpoint.width, height)} /></div>
              <label className="field-label">背景<input value={document.viewport.background} onChange={(event) => commit((current) => ({ ...current, viewport: { ...current.viewport, background: event.target.value } }))} /></label>

              <div className="panel-title section-title layer-title"><span>图片资源</span><small>{document.assets?.length ?? 0}</small></div>
              <input ref={assetInput} className="asset-input" type="file" accept="image/*" multiple onChange={(event) => void importAssets(event.target.files).catch((error) => showToast(String(error)))} />
              <button className="secondary-button" onClick={() => assetInput.current?.click()}>导入图片</button>
              <div className="asset-grid">{(document.assets ?? []).map((asset) => <button key={asset.id} title={`使用 ${asset.name}`} onClick={() => useAsset(asset)}><img src={asset.dataUrl} alt={asset.name} /><span>{asset.name}</span></button>)}</div>
              {tokens && <><div className="panel-title section-title">设计 Token</div><div className="token-colors">
                {(['primary', 'accent', 'surface', 'text', 'muted'] as const).map((key) => <label key={key} className="token-color"><span>{key}</span><input type="color" value={tokens.colors[key]} onChange={(event) => updateTokenColor(key, event.target.value)} /><input value={tokens.colors[key]} onChange={(event) => updateTokenColor(key, event.target.value)} /></label>)}
              </div><div className="size-row"><NumberField label="基础字号" value={tokens.typography.baseFontSize} onChange={(baseFontSize) => updateTokens((current) => ({ ...current, typography: { ...current.typography, baseFontSize } }))} /><NumberField label="中圆角" value={tokens.radii.medium} onChange={(medium) => updateTokens((current) => ({ ...current, radii: { ...current.radii, medium } }))} /></div></>}
              <div className="panel-title section-title">AI 待办</div>
              <div className="request-summary">{document.requests.filter((request) => request.status === 'pending').length} 个待处理请求</div>
              <p className="helper-text">保存后，在对话中让 AI“处理 Web Design Studio 的待办请求”。</p>
            </>}
          </div>
        </aside>}

        <section className="canvas-stage">
          {!preview && <div className="canvas-toolbar device-toolbar">
            {editingSlot && editingContainer && editingSlotDefinition ? <>
              <button className="slot-editor-back" onClick={exitSlotEditor}>‹ 返回页面</button>
              <span className="slot-editor-path"><b>{editingContainer.library?.component}</b><i>/</i>{editingSlotDefinition.label}</span>
              <span className="toolbar-divider" />
              <button className="fit-button" onClick={() => insertSlotTemplate('form')}>＋ 表单模板</button>
              <button className="fit-button" onClick={() => insertSlotTemplate('details')}>＋ 详情模板</button>
            </> : <>
              <div className="device-switcher">
                {deviceOptions.map((item) => <button key={item.device} className={device === item.device ? 'active' : ''} title={item.label} onClick={() => switchDevice(item.device)}>{item.icon}<span>{item.label}</span></button>)}
              </div>
              <span className="toolbar-divider" />
              <button title="缩小" onClick={() => setZoom(Math.max(.25, zoom - .1))}>−</button><span className="zoom-value">{Math.round(zoom * 100)}%</span><button title="放大" onClick={() => setZoom(Math.min(1.5, zoom + .1))}>＋</button>
              <span className="toolbar-divider" />
              <button className="fit-button" title="适应画布宽度" onClick={fitCanvasWidth}>适应</button><button className="fit-button" title="恢复 100%" onClick={() => setZoom(1)}>100%</button>
              <span className="toolbar-divider" /><button className={`fit-button interaction-mode-button ${interactionMode ? 'active' : ''}`} title="操作输入框、选择器、抽屉、标签页等真实组件" onClick={toggleInteractionMode}>{interactionMode ? '退出交互' : '交互'}</button>
            </>}
          </div>}
          {preview && <button className="exit-fullscreen-preview" onClick={toggleFullPreview}>退出预览 <span>Esc</span></button>}
          {interactionMode && !preview && <div className="interaction-mode-banner"><span>●</span> 交互模式：可以输入、选择、展开和打开弹层；退出后继续拖动编辑</div>}
          {preview && pages.length > 1 && <nav className="route-preview-bar">{pages.map((page) => <button key={page.id} className={page.id === pageId ? 'active' : ''} onClick={() => switchPage(page.id)}><span>{page.name}</span><small>{page.slug}</small></button>)}</nav>}
          {selected && !preview && <div className="selection-toolbar">
            <button title="左对齐" onClick={() => alignSelected('left')}>⇤</button><button title="水平居中" onClick={() => alignSelected('center')}>↔</button><button title="右对齐" onClick={() => alignSelected('right')}>⇥</button>
            <button title="顶部对齐" onClick={() => alignSelected('top')}>↥</button><button title="垂直居中" onClick={() => alignSelected('middle')}>↕</button><button title="底部对齐" onClick={() => alignSelected('bottom')}>↧</button>
            <span /><button title="置于顶层" onClick={() => reorderSelected('front')}>⤒</button><button title="上移一层" onClick={() => reorderSelected('forward')}>↑</button><button title="下移一层" onClick={() => reorderSelected('backward')}>↓</button><button title="置于底层" onClick={() => reorderSelected('back')}>⤓</button>
            <span /><button title="复制 ⌘C" onClick={copySelected}>⧉</button><button title="粘贴 ⌘V" disabled={!clipboard} onClick={pasteClipboard}>▣</button>
            {selectedIds.length > 1 && <><span /><button className="wide-tool" title="将选中组件组合" onClick={groupSelected}>组合</button></>}
            {canUngroup && <button className="wide-tool" title="取消当前容器组合" onClick={ungroupSelected}>取消组合</button>}
          </div>}
          <div ref={canvasScroll} className={`canvas-scroll ${preview ? 'preview-canvas-scroll' : ''} ${editingSlot ? 'slot-editor-scroll' : ''}`}>
            {editingSlot && editingContainer && editingSlotDefinition ? <div className="slot-editor-frame">
              <div className="slot-editor-heading"><div><span>可编辑内容区域</span><strong>{editingSlotDefinition.label}</strong><small>{editingSlotDefinition.description}</small></div><em>{Math.round(editingSlotDefinition.width)} × {Math.round(editingSlotDefinition.height)}</em></div>
              <div className="slot-design-canvas design-canvas" style={{ width: editingSlotDefinition.width, height: editingSlotDefinition.height }} onDragOver={(event) => event.preventDefault()} onDrop={onCanvasDrop} onPointerDown={() => { setSelectedId(undefined); setSelectedIds([]); }}>
                {editingSlotComponents.length === 0 && <div className="slot-empty-state"><span>＋</span><strong>从左侧拖入组件</strong><p>也可以先插入表单或详情模板，再逐项调整。</p><div><button onPointerDown={(event) => event.stopPropagation()} onClick={() => insertSlotTemplate('form')}>插入表单</button><button onPointerDown={(event) => event.stopPropagation()} onClick={() => insertSlotTemplate('details')}>插入详情</button></div></div>}
                {editingVisibleComponents.sort((left, right) => left.zIndex - right.zIndex).map((component) => {
                  const frame = resolveComponent(component, device);
                  const containerFrame = resolveComponent(editingContainer, device);
                  const resolved = { ...frame, x: frame.x - containerFrame.x, y: frame.y - containerFrame.y };
                  if (resolved.hidden) return null;
                  return <CanvasComponent key={component.id} component={component} resolved={resolved} selected={selectedIdSet.has(component.id)} primary={component.id === selectedId} interactive={false} tokens={tokens} slotContent={runtimeSlotContentMap(document, component, device, false, tokens, activatePreviewInteraction)} onPointerDown={(event) => beginInteraction(event, component, 'move')} onResizePointerDown={(event) => beginInteraction(event, component, 'resize')} onPreviewActivate={() => activatePreviewInteraction(component)} />;
                })}
              </div>
            </div> : <div className="canvas-scale" style={{ width: breakpoint.width * zoom, height: breakpoint.height * zoom }}>
              <div className={`design-canvas device-${device}`} style={{
                width: breakpoint.width,
                height: breakpoint.height,
                background: document.viewport.background,
                transform: `scale(${zoom})`,
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
              } as CSSProperties} onDragOver={(event) => event.preventDefault()} onDrop={onCanvasDrop} onPointerDown={() => { if (!interactionMode) { setSelectedId(undefined); setSelectedIds([]); } }}>
                {snapGuides.x !== undefined && <div className="snap-guide vertical" style={{ left: snapGuides.x }} />}
                {snapGuides.y !== undefined && <div className="snap-guide horizontal" style={{ top: snapGuides.y }} />}
                {[...pageComponents].filter((component) => !contentContainerAncestor(document, component)).sort((left, right) => left.zIndex - right.zIndex).map((component) => {
                  const resolved = resolveComponent(component, device);
                  if (resolved.hidden) return null;
                  return <CanvasComponent key={component.id} component={component} resolved={resolved} selected={selectedIdSet.has(component.id)} primary={component.id === selectedId} interactive={preview || interactionMode} tokens={tokens} slotContent={runtimeSlotContentMap(document, component, device, preview || interactionMode, tokens, activatePreviewInteraction)} onPointerDown={(event) => beginInteraction(event, component, 'move')} onResizePointerDown={(event) => beginInteraction(event, component, 'resize')} onPreviewActivate={() => activatePreviewInteraction(component)} />;
                })}
              </div>
            </div>}
          </div>
        </section>

        {!preview && <aside className="inspector-panel">
          {selected && inspectedFrame ? <>
            <div className="inspector-heading"><div><span className="eyebrow">已选择 {selectedIds.length > 1 ? `${selectedIds.length} 项` : ''} · {device}</span><strong>{selected.name}</strong></div><button className="danger-link" onClick={deleteSelected}>删除</button></div>
            <div className="inspector-actions"><button onClick={duplicateSelected}>复制 ⌘D</button><button className={selected.locked ? 'active' : ''} onClick={() => toggleLocked(selected)}>{selected.locked ? '解锁' : '锁定'}</button><button className={inspectedFrame.hidden ? 'active' : ''} onClick={() => toggleHidden(selected)}>{inspectedFrame.hidden ? '显示' : '隐藏'}</button></div>
            <button className="secondary-button save-symbol-button" onClick={saveSelectionAsSymbol}>保存到可复用组件库</button>
            {selectedSymbol && selected.symbolInstanceId && <div className="symbol-instance-panel">
              <div><span>实例来源</span><strong>{selectedSymbol.name}</strong></div>
              <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('content')} onChange={() => toggleSelectedSymbolOverride('content')} />保留内容</label>
              <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('style')} onChange={() => toggleSelectedSymbolOverride('style')} />保留样式</label>
              <label><input type="checkbox" checked={(selected.symbolOverrides ?? []).includes('frame')} onChange={() => toggleSelectedSymbolOverride('frame')} />保留位置尺寸</label>
              <div className="symbol-instance-actions"><button onClick={updateSelectedSymbolDefinition}>用当前实例更新定义</button><button onClick={synchronizeSelectedSymbol}>同步全部实例</button><button className="danger" onClick={detachSelectedSymbol}>脱离组件库</button></div>
            </div>}
            <label className="field-label">组件名称<input value={selected.name} onChange={(event) => updateSelected({ name: event.target.value })} /></label>
            <label className="field-label">内容<textarea rows={3} value={selected.content} onChange={(event) => updateSelected({ content: event.target.value })} /></label>
            {selected.library?.name === 'antd' && <div className="antd-inspector">
              <div className="panel-title section-title">Ant Design 组件</div>
              <div className="antd-binding-summary"><span>组件</span><strong>{selected.library.component}</strong><small>v{selected.library.version}</small></div>
              {selectedEditableSlots.length > 0 && <div className="content-slots-panel">
                <div className="content-slots-heading"><div><strong>内部内容</strong><span>像页面一样继续设计</span></div><em>{selectedEditableSlots.length} 个区域</em></div>
                {selectedEditableSlots.map((slot) => {
                  const count = componentsInSlot(document, selected.id, slot.id).length;
                  return <button key={slot.id} className={editingSlot?.componentId === selected.id && editingSlot.slotId === slot.id ? 'active' : ''} onClick={() => editComponentSlot(selected, slot.id)}><span><strong>{slot.label}</strong><small>{slot.description}</small></span><em>{count > 0 ? `${count} 个组件` : '空白'}</em><b>编辑 ›</b></button>;
                })}
              </div>}
              <label className="field-label antd-variant-field">展现款式<select value={selected.library.variant ?? selectedAntdVariants[0]?.id} onChange={(event) => applySelectedAntdVariant(event.target.value)}>{selectedAntdVariants.map((variant) => <option key={variant.id} value={variant.id}>{variant.label}</option>)}</select></label>
              {Object.entries(selected.library.props).filter(([, value]) => ['string', 'number', 'boolean'].includes(typeof value)).map(([key, value]) => typeof value === 'boolean'
                ? <label key={key} className="antd-boolean-prop"><input type="checkbox" checked={value} onChange={(event) => updateSelectedLibraryProp(key, event.target.checked)} /><span>{key}</span></label>
                : typeof value === 'number'
                  ? <NumberField key={key} label={key} value={value} onChange={(next) => updateSelectedLibraryProp(key, next)} />
                  : <label key={key} className="field-label">{key}<input value={String(value)} onChange={(event) => updateSelectedLibraryProp(key, event.target.value)} /></label>)}
              {Object.entries(selected.library.props).some(([, value]) => value !== null && typeof value === 'object') && <div className="antd-data-editors"><div className="panel-title section-title">示例数据</div>{Object.entries(selected.library.props).filter(([, value]) => value !== null && typeof value === 'object').map(([key, value]) => <JsonPropertyEditor key={key} label={key} value={value} onChange={(next) => updateSelectedLibraryProp(key, next)} />)}</div>}
            </div>}
            <div className="panel-title section-title">预览交互</div>
            <label className="field-label">点击行为<select value={selected.interaction?.type ?? 'none'} onChange={(event) => {
              const type = event.target.value;
              if (type === 'none') updateSelected({ interaction: undefined });
              else if (type === 'page') updateSelected({ interaction: { type: 'page', target: pages.find((page) => page.id !== pageId)?.id ?? pageId } });
              else updateSelected({ interaction: { type: 'url', target: 'https://example.com' } });
            }}><option value="none">无交互</option><option value="page">跳转页面</option><option value="url">打开 URL</option></select></label>
            {selected.interaction?.type === 'page' && <label className="field-label interaction-target">目标页面<select value={selected.interaction.target} onChange={(event) => updateSelected({ interaction: { type: 'page', target: event.target.value } })}>{pages.map((page) => <option key={page.id} value={page.id}>{page.name} · {page.slug}</option>)}</select></label>}
            {selected.interaction?.type === 'url' && <label className="field-label interaction-target">目标 URL<input value={selected.interaction.target} onChange={(event) => updateSelected({ interaction: { type: 'url', target: event.target.value } })} placeholder="https://example.com" /></label>}
            <div className="size-row four">
              <NumberField label={editingSlot ? 'X · 内容' : 'X'} value={inspectedFrame.x} onChange={(x) => updateInspectedFrame({ x })} disabled={selected.locked} />
              <NumberField label={editingSlot ? 'Y · 内容' : 'Y'} value={inspectedFrame.y} onChange={(y) => updateInspectedFrame({ y })} disabled={selected.locked} />
              <NumberField label="W" value={inspectedFrame.width} onChange={(width) => updateInspectedFrame({ width })} disabled={selected.locked} />
              <NumberField label="H" value={inspectedFrame.height} onChange={(height) => updateInspectedFrame({ height })} disabled={selected.locked} />
            </div>
            <div className="panel-title section-title">样式 · {device}</div>
            <label className="field-label">背景<input value={inspectedFrame.style.background ?? ''} onChange={(event) => updateSelectedStyle({ background: event.target.value })} /></label>
            <label className="field-label">文字颜色<input value={inspectedFrame.style.color ?? ''} onChange={(event) => updateSelectedStyle({ color: event.target.value })} /></label>
            <div className="size-row"><NumberField label="字号" value={inspectedFrame.style.fontSize ?? 16} onChange={(fontSize) => updateSelectedStyle({ fontSize })} /><NumberField label="圆角" value={inspectedFrame.style.borderRadius ?? 0} onChange={(borderRadius) => updateSelectedStyle({ borderRadius })} /></div>
            <div className="token-apply-row"><button onClick={() => applyColorToken('background', 'primary')}>主色背景</button><button onClick={() => applyColorToken('background', 'surface')}>表面背景</button><button onClick={() => applyColorToken('color', 'text')}>正文色</button><button onClick={() => applyRadiusToken('medium')}>中圆角</button></div>
            {(selected.type === 'section' || directChildCount > 0) && selectedEditableSlots.length === 0 && <>
              <div className="panel-title section-title">容器布局 · {directChildCount} 个子组件</div>
              <label className="field-label">布局方式<select value={selected.layout?.mode ?? 'free'} onChange={(event) => updateSelectedLayout({ mode: event.target.value as NonNullable<WebDesignComponent['layout']>['mode'] })}><option value="free">自由布局</option><option value="flex-row">Flex 横向</option><option value="flex-column">Flex 纵向</option><option value="grid">Grid 网格</option></select></label>
              <div className="size-row"><NumberField label="间距" value={selected.layout?.gap ?? 16} onChange={(gap) => updateSelectedLayout({ gap })} /><NumberField label="内边距" value={selected.layout?.padding ?? 16} onChange={(padding) => updateSelectedLayout({ padding })} /></div>
              {selected.layout?.mode === 'grid' && <NumberField label="列数" value={selected.layout.columns ?? 2} onChange={(columns) => updateSelectedLayout({ columns: Math.max(1, Math.round(columns)) })} />}
              <label className="field-label layout-align-field">对齐<select value={selected.layout?.align ?? 'start'} onChange={(event) => updateSelectedLayout({ align: event.target.value as NonNullable<WebDesignComponent['layout']>['align'] })}><option value="start">起点</option><option value="center">居中</option><option value="end">终点</option><option value="stretch">拉伸</option></select></label>
              <button className="secondary-button" disabled={directChildCount === 0 || selected.layout?.mode === 'free'} onClick={applySelectedAutoLayout}>应用自动布局</button>
            </>}
            <div className="panel-title section-title">组件批注</div>
            <div className="notes-list">{selected.annotations.length === 0 && <span className="empty-hint">还没有批注</span>}{selected.annotations.map((note) => <div key={note.id} className={`note-card ${note.status}`}><span>{note.text}</span><small>{note.status === 'open' ? '待处理' : '已完成'}</small></div>)}</div>
            <textarea className="composer" rows={3} placeholder="例如：这里的按钮再醒目一些" value={annotationText} onChange={(event) => setAnnotationText(event.target.value)} /><button className="secondary-button" onClick={addAnnotation}>添加批注</button>
            <div className="panel-title section-title">与 AI 交互</div>
            <textarea className="composer" rows={4} placeholder={`告诉 AI 如何修改当前${device === 'desktop' ? '桌面' : device === 'tablet' ? '平板' : '手机'}组件…`} value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} /><button className="ai-button" onClick={() => void addAiRequest()}>提交给 AI</button>
          </> : <div className="empty-inspector"><div className="empty-icon">↖</div><strong>选择一个组件</strong><p>在画布或图层中选择组件，然后编辑、对齐、锁定、批注或提交 AI 请求。</p></div>}
        </aside>}
      </main>
      {variantPickerDefinition && <div className="studio-modal-backdrop" onPointerDown={() => setVariantPickerId(undefined)}>
        <section className="studio-modal variant-picker" onPointerDown={(event) => event.stopPropagation()}>
          <header><div><span className="eyebrow">Ant Design · {variantPickerDefinition.category}</span><h2>{variantPickerDefinition.id} · {variantPickerDefinition.label}</h2><p>先看实际效果，再选择最适合当前页面的款式。</p></div><button onClick={() => setVariantPickerId(undefined)}>×</button></header>
          <div className="variant-preview-grid">{variantPickerVariants.map((variant) => {
            const previewComponent = applyAntdComponentVariant(createAntdComponent(variantPickerDefinition.id, 0, 0), variant.id);
            return <button key={variant.id} className="variant-preview-card" onClick={() => insertAntdComponent(variantPickerDefinition.id, variant.id)}><div className="variant-live-preview"><Suspense fallback={<span className="antd-loading-placeholder">加载组件…</span>}><AntdCanvasComponent component={previewComponent} preview={false} tokens={tokens} /></Suspense></div><span><strong>{variant.label}</strong><small>插入此款式</small></span></button>;
          })}</div>
        </section>
      </div>}
      {themePickerOpen && <div className="studio-modal-backdrop" onPointerDown={() => setThemePickerOpen(false)}>
        <section className="studio-modal theme-picker" onPointerDown={(event) => event.stopPropagation()}>
          <header><div><span className="eyebrow">Visual system</span><h2>选择整站设计风格</h2><p>一次统一颜色、字体、圆角、画布背景和 Ant Design 主题。</p></div><button onClick={() => setThemePickerOpen(false)}>×</button></header>
          <div className="theme-preset-grid">{WEB_DESIGN_THEME_PRESETS.map((preset) => <button key={preset.id} onClick={() => applyDesignTheme(preset)}><div className="theme-preview" style={{ background: preset.canvasBackground }}><i style={{ background: preset.preview[1] }} /><b style={{ background: preset.preview[2] }} /><span style={{ color: preset.tokens.colors.text }}>Aa</span></div><strong>{preset.name}</strong><small>{preset.description}</small><div className="theme-swatches">{preset.preview.map((color) => <i key={color} style={{ background: color }} />)}</div></button>)}</div>
        </section>
      </div>}
      {aiPanelOpen && <div className="ai-command-panel">
        <div className="ai-command-heading"><div><span>✦ AI 设计助手</span><strong>{editingSlotDefinition ? `正在设计：${editingSlotDefinition.label}` : aiTarget ? `正在修改：${aiTarget.name}` : `正在设计：${currentPage?.name ?? '当前页面'}`}</strong></div><button onClick={() => setAiPanelOpen(false)}>×</button></div>
        <div className="ai-quick-prompts">{aiQuickPrompts.map((prompt) => <button key={prompt} onClick={() => setAiInstruction(prompt)}>{prompt}</button>)}</div>
        <textarea autoFocus rows={4} value={aiInstruction} onChange={(event) => setAiInstruction(event.target.value)} onKeyDown={(event) => { if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') void addAiRequest(); }} placeholder={aiTarget ? '描述你希望这个组件或内容区域如何变化，也可以要求 AI 直接添加表单、详情或操作组件…' : '描述网站目标、受众、内容和你喜欢的视觉感觉…'} />
        <div className="ai-command-footer"><span>{editingSlotDefinition ? `仅修改 ${editingSlotDefinition.label}` : aiTarget ? '仅修改当前组件' : `作用于 ${device} · 当前页面`}</span><button disabled={!aiInstruction.trim()} onClick={() => void addAiRequest()}>提交设计任务 <b>⌘↵</b></button></div>
      </div>}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function NumberField({ label, value, onChange, disabled = false }: { label: string; value: number; onChange: (value: number) => void; disabled?: boolean }) {
  return <label className="field-label">{label}<input type="number" disabled={disabled} value={Number.isFinite(value) ? value : 0} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function JsonPropertyEditor({ label, value, onChange }: { label: string; value: WebDesignJsonValue; onChange: (value: WebDesignJsonValue) => void }) {
  const serialized = JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState('');
  useEffect(() => { setDraft(serialized); setError(''); }, [serialized]);
  function apply() {
    try {
      const parsed = JSON.parse(draft) as WebDesignJsonValue;
      onChange(parsed);
      setError('');
    } catch {
      setError('JSON 格式不正确');
    }
  }
  return <div className="json-prop-editor"><div><strong>{label}</strong><button onClick={apply}>应用数据</button></div><textarea rows={Math.min(10, Math.max(4, draft.split('\n').length))} value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} />{error && <small>{error}</small>}</div>;
}

function CanvasComponentContent({ component, interactive, tokens, slotContent }: { component: WebDesignComponent; interactive: boolean; tokens?: WebDesignTokens; slotContent?: Record<string, ReactNode> }) {
  if (component.library?.name === 'antd') return <div className={`antd-canvas-content ${interactive ? 'preview' : ''}`}><Suspense fallback={<span className="antd-loading-placeholder">加载组件…</span>}><AntdCanvasComponent component={component} preview={interactive} tokens={tokens} slotContent={slotContent} /></Suspense></div>;
  if (component.type === 'image') return component.content ? <img src={component.content} alt={component.name} draggable={false} /> : <span className="image-placeholder">图片</span>;
  if (component.type === 'video') return component.content ? <video src={component.content} controls={interactive} muted /> : <span className="media-placeholder">▶<small>视频</small></span>;
  if (component.type === 'input') return <span className="input-placeholder">{component.content}</span>;
  if (component.type === 'textarea') return <span className="input-placeholder textarea-placeholder">{component.content}</span>;
  if (component.type === 'select') return <span className="select-placeholder"><span>{component.content.split('\n')[0]}</span><b>⌄</b></span>;
  if (component.type === 'checkbox') return <span className="choice-control"><i>✓</i>{component.content}</span>;
  if (component.type === 'switch') return <span className="choice-control"><i className="switch-track">●</i>{component.content}</span>;
  if (component.type === 'divider') return null;
  if (component.type === 'list') return <ul className="component-list">{component.content.split('\n').filter(Boolean).map((item, index) => <li key={index}>{item}</li>)}</ul>;
  if (component.type === 'table') return <table className="component-table"><tbody>{component.content.split('\n').filter(Boolean).map((row, rowIndex) => <tr key={rowIndex}>{row.split('|').map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody></table>;
  if (component.type === 'avatar' && /^(data:image\/|https?:\/\/)/.test(component.content)) return <img src={component.content} alt={component.name} draggable={false} />;
  if (component.type === 'section') return null;
  return <span className="component-copy">{component.content}</span>;
}

function CanvasComponent({ component, resolved, selected, primary, interactive, tokens, slotContent, onPointerDown, onResizePointerDown, onPreviewActivate }: {
  component: WebDesignComponent;
  resolved: ResolvedWebDesignComponent;
  selected: boolean;
  primary: boolean;
  interactive: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
  onPointerDown: (event: ReactPointerEvent) => void;
  onResizePointerDown: (event: ReactPointerEvent) => void;
  onPreviewActivate: () => void;
}) {
  const style: CSSProperties = {
    left: resolved.x, top: resolved.y, width: resolved.width, height: resolved.height, zIndex: component.zIndex,
    background: resolved.style.background, color: resolved.style.color, borderColor: resolved.style.borderColor,
    borderWidth: resolved.style.borderWidth, borderStyle: resolved.style.borderWidth ? 'solid' : undefined,
    borderRadius: resolved.style.borderRadius, fontSize: resolved.style.fontSize, fontWeight: resolved.style.fontWeight,
    textAlign: resolved.style.textAlign, opacity: resolved.style.opacity, boxShadow: resolved.style.shadow
  };
  return (
    <div className={`canvas-component type-${component.type} ${component.library?.name === 'antd' ? 'library-antd' : ''} ${selected ? 'selected' : ''} ${component.locked ? 'locked' : ''} ${interactive ? 'interactive' : ''}`} style={style} onPointerDown={onPointerDown} onClick={(event) => { if (interactive && component.interaction) { event.stopPropagation(); onPreviewActivate(); } }}>
      <CanvasComponentContent component={component} interactive={interactive} tokens={tokens} slotContent={slotContent} />
      {!interactive && component.annotations.some((annotation) => annotation.status === 'open') && <span className="annotation-badge">{component.annotations.filter((annotation) => annotation.status === 'open').length}</span>}
      {primary && !interactive && <><span className="selection-label">{component.locked ? '🔒 ' : ''}{component.name}</span>{!component.locked && <span className="resize-handle" onPointerDown={onResizePointerDown} />}</>}
    </div>
  );
}

function runtimeSlotContentMap(
  document: WebDesignDocument,
  component: WebDesignComponent,
  device: WebDesignDevice,
  interactive: boolean,
  tokens: WebDesignTokens | undefined,
  onPreviewActivate: (component: WebDesignComponent) => void
): Record<string, ReactNode> {
  return Object.fromEntries(editableSlotsForAntdComponent(component)
    .filter((slot) => componentsInSlot(document, component.id, slot.id).length > 0)
    .map((slot) => [slot.id,
      <RuntimeSlotContent key={slot.id} document={document} container={component} slot={slot} device={device} interactive={interactive} tokens={tokens} onPreviewActivate={onPreviewActivate} />
    ]));
}

function RuntimeSlotContent({ document, container, slot, device, interactive, tokens, onPreviewActivate }: {
  document: WebDesignDocument;
  container: WebDesignComponent;
  slot: AntdEditableSlot;
  device: WebDesignDevice;
  interactive: boolean;
  tokens?: WebDesignTokens;
  onPreviewActivate: (component: WebDesignComponent) => void;
}) {
  const containerFrame = resolveComponent(container, device);
  const children = visibleComponentsInSlot(document, container.id, slot.id).sort((left, right) => left.zIndex - right.zIndex);
  if (children.length === 0) return null;
  const requiredHeight = Math.max(slot.height, ...children.map((child) => {
    const frame = resolveComponent(child, device);
    return frame.y - containerFrame.y + frame.height + 12;
  }));
  return <div className="runtime-slot-canvas" style={{ minHeight: requiredHeight }}>
    {children.map((child) => {
      const frame = resolveComponent(child, device);
      if (frame.hidden) return null;
      const localFrame = { ...frame, x: frame.x - containerFrame.x, y: frame.y - containerFrame.y };
      const style: CSSProperties = {
        position: 'absolute', left: localFrame.x, top: localFrame.y, width: localFrame.width, height: localFrame.height, zIndex: child.zIndex,
        background: localFrame.style.background, color: localFrame.style.color, borderColor: localFrame.style.borderColor,
        borderWidth: localFrame.style.borderWidth, borderStyle: localFrame.style.borderWidth ? 'solid' : undefined,
        borderRadius: localFrame.style.borderRadius, fontSize: localFrame.style.fontSize, fontWeight: localFrame.style.fontWeight,
        textAlign: localFrame.style.textAlign, opacity: localFrame.style.opacity, boxShadow: localFrame.style.shadow
      };
      return <div key={child.id} className={`runtime-slot-component type-${child.type}`} style={style} onClick={(event) => {
        if (interactive && child.interaction) {
          event.stopPropagation();
          onPreviewActivate(child);
        }
      }}><CanvasComponentContent component={child} interactive={interactive} tokens={tokens} slotContent={runtimeSlotContentMap(document, child, device, interactive, tokens, onPreviewActivate)} /></div>;
    })}
  </div>;
}
