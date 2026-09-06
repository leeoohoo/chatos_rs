import { editableSlotsForAntdComponent } from './antd-slots.js';
import { fitContentCanvasToComponents, resolveComponent, updateComponentFrame } from './editor-model.js';
import { pageIdForComponent, type WebDesignComponent, type WebDesignDevice, type WebDesignDocument } from './schema.js';
import { contentSlot, namedItemSlots, splitPanelSlots, type UiEditableSlot } from './ui-library.js';

const OVERLAY_CONTENT_COMPONENTS = new Set(['Drawer', 'Modal', 'Dialog', 'Sheet', 'AlertDialog', 'Popover', 'HoverCard', 'ToggleTip', 'FloatingPanel', 'OverlayManager']);
const FIXED_CONTENT_VIEWPORTS = new Set(['ScrollArea', 'Carousel', 'AspectRatio']);

const CHAKRA_CONTENT_SLOTS: Record<string, [string, string]> = {
  AspectRatio: ['比例内容', '保持固定宽高比的媒体或自由组件'],
  Bleed: ['溢出内容', '突破父容器内边距的内容区域'],
  AbsoluteCenter: ['居中内容', '通过绝对定位居中的自由内容'],
  Center: ['居中内容', '在容器中央排列的自由内容'],
  Float: ['主体内容', '浮标所依附的卡片或内容区域'],
  Wrap: ['换行内容', '会根据可用宽度自动换行的组件'],
  Box: ['盒子内容', '盒子内部的自由内容'],
  Container: ['容器内容', '限制宽度的页面内容'],
  Flex: ['Flex 内容', '使用弹性布局组织组件'],
  Grid: ['Grid 内容', '使用栅格组织组件'],
  SimpleGrid: ['栅格内容', '自动等分的栅格内容'],
  Stack: ['Stack 内容', '垂直或水平堆叠的组件'],
  Group: ['组合内容', '组合在一起的操作组件'],
  ScrollArea: ['滚动内容', '可向下滚动的内容区域'],
  LinkOverlay: ['卡片内容', '整块可点击区域中的标题、说明和媒体'],
  ActionBar: ['操作栏内容', '批量操作按钮、状态和辅助信息'],
  FloatingPanel: ['面板内容', '可拖动浮动面板中的自由内容'],
  OverlayManager: ['浮层内容', '由浮层管理器程序化打开的内容'],
  Fieldset: ['字段组内容', '表单字段、说明和操作'],
  Card: ['卡片内容', '卡片主体、操作和媒体内容'],
  Collapsible: ['折叠内容', '展开后展示的组件']
};

const SHADCN_CONTENT_SLOTS: Record<string, [string, string]> = {
  AspectRatio: ['比例容器内容', '保持固定宽高比的媒体或组件'],
  ButtonGroup: ['按钮组内容', '组合排列的操作按钮'],
  ScrollArea: ['滚动内容', '可向下滚动的内容区域'],
  Sidebar: ['侧边栏内容', '导航、账号和辅助操作'],
  Field: ['字段内容', '标签、输入控件、说明和错误信息'],
  Card: ['卡片内容', '卡片标题、正文、媒体和操作'],
  Collapsible: ['折叠内容', '展开后展示的组件']
};

function overlaySlot(component: WebDesignComponent, label: string, description: string): UiEditableSlot[] {
  const side = component.library?.props.side ?? component.library?.props.placement;
  const horizontal = side === 'top' || side === 'bottom';
  return [contentSlot(component, 'content', label, description, {
    width: horizontal ? 760 : 380,
    height: horizontal ? 260 : 560
  })];
}

export function editableSlotsForUiComponent(component: WebDesignComponent): UiEditableSlot[] {
  const library = component.library?.name;
  const name = component.library?.component;
  if (!library || !name) return [];
  if (library === 'antd') return editableSlotsForAntdComponent(component);

  if (library === 'magicui' || library === 'spell' || library === 'inspira' || library === 'daisyui') {
    const family = component.library?.props.family;
    if (family === 'bento' || family === 'card' || family === 'device' || family === 'media' || family === 'social' || family === 'tree' || family === 'terminal' || family === 'list') {
      return [contentSlot(component, 'content', '内容区域', '在保留组件视觉与交互的同时，放入自由编辑的内容', {
        width: Math.max(280, component.width - 32),
        height: Math.max(160, component.height - 48)
      })];
    }
    if (family === 'tabs') return namedItemSlots(component, 'tab', '标签页');
    if (family === 'modal') return [contentSlot(component, 'content', '弹窗内容', '在弹窗中设计表单、详情和操作区域', { width: 480, height: 400 })];
    if (family === 'gallery') return [contentSlot(component, 'content', '画廊内容', '在保留画廊导航的同时设计媒体和说明', { width: Math.max(320, component.width - 40), height: Math.max(220, component.height - 72) })];
    if (family === 'tooltip') return [contentSlot(component, 'popup', '提示内容', '悬停或点击时展示的自由内容', { width: 300, height: 180 })];
    if (family === 'testimonial') return [contentSlot(component, 'content', '评价内容', '设计头像、引用、身份和辅助媒体', { width: Math.max(300, component.width - 40), height: Math.max(180, component.height - 64) })];
  }

  if (library === 'daisyui') {
    if (name === 'Accordion') return namedItemSlots(component, 'panel', '手风琴面板');
    if (name === 'Tab') return namedItemSlots(component, 'tab', '标签页');
    if (name === 'Carousel') return namedItemSlots(component, 'slide', '轮播项');
    if (name === 'Drawer') return overlaySlot(component, '抽屉内容', '导航、表单、详情和操作区域');
    if (name === 'Modal') return [contentSlot(component, 'content', '模态框内容', '正文、表单和操作区域', { width: 480, height: 400 })];
    if (['Card', 'Collapse', 'Fieldset', 'Footer', 'Hero', 'Navbar'].includes(name)) {
      return [contentSlot(component, 'content', `${component.library?.component ?? '组件'}内容`, '保留 daisyUI 结构并放入自由编辑的内容', {
        width: Math.max(280, component.width - 32),
        height: Math.max(160, component.height - 56)
      })];
    }
  }

  if (library === 'chakra') {
    const single = CHAKRA_CONTENT_SLOTS[name];
    if (single) return [contentSlot(component, 'content', single[0], single[1])];
    if (name === 'Splitter') return splitPanelSlots(component);
    if (name === 'Carousel') return Array.from({ length: Math.max(1, Number(component.library?.props.slideCount ?? 3)) }, (_, index) => contentSlot(component, `slide-${index + 1}`, `轮播项 ${index + 1}`, `编辑第 ${index + 1} 个轮播页面`, { width: Math.max(280, component.width - 48), height: Math.max(180, component.height - 80) }));
    if (name === 'Tabs') return namedItemSlots(component, 'tab', '标签页');
    if (name === 'Accordion') return namedItemSlots(component, 'panel', '手风琴面板');
    if (name === 'Dialog') return [contentSlot(component, 'content', '对话框内容', '正文、表单和操作区域', { width: 480, height: 400 })];
    if (name === 'Drawer') return overlaySlot(component, '抽屉内容', '表单、详情、导航和操作区域');
    if (name === 'Popover') return [contentSlot(component, 'popup', '气泡内容', '点击后展示的自由内容', { width: 320, height: 220 })];
    if (name === 'HoverCard') return [contentSlot(component, 'popup', '悬浮卡片内容', '鼠标悬停后展示的自由内容', { width: 340, height: 220 })];
    if (name === 'ToggleTip') return [contentSlot(component, 'popup', '点击提示内容', '点击触发后展示的辅助内容', { width: 280, height: 160 })];
  }

  if (library === 'shadcn') {
    const single = SHADCN_CONTENT_SLOTS[name];
    if (single) return [contentSlot(component, 'content', single[0], single[1])];
    if (name === 'Resizable') return splitPanelSlots(component);
    if (name === 'Carousel') return Array.from({ length: Math.max(1, Number(component.library?.props.slideCount ?? 3)) }, (_, index) => contentSlot(component, `slide-${index + 1}`, `轮播项 ${index + 1}`, `编辑第 ${index + 1} 个轮播页面`, { width: Math.max(280, component.width - 48), height: Math.max(180, component.height - 80) }));
    if (name === 'Tabs') return namedItemSlots(component, 'tab', '标签页');
    if (name === 'Accordion') return namedItemSlots(component, 'panel', '手风琴面板');
    if (name === 'Dialog' || name === 'AlertDialog') return [contentSlot(component, 'content', name === 'AlertDialog' ? '确认内容' : '对话框内容', '正文、表单和操作区域', { width: 480, height: 400 })];
    if (name === 'Drawer' || name === 'Sheet') return overlaySlot(component, name === 'Sheet' ? '面板内容' : '抽屉内容', '表单、详情、导航和操作区域');
    if (name === 'Popover' || name === 'HoverCard') return [contentSlot(component, 'popup', name === 'Popover' ? '气泡内容' : '悬浮卡片内容', '浮层中的自由内容', { width: 320, height: 220 })];
  }

  return [];
}

export function isUiContentContainer(component: WebDesignComponent): boolean {
  return editableSlotsForUiComponent(component).length > 0;
}

export function isOverlayUiContentContainer(component: WebDesignComponent): boolean {
  return OVERLAY_CONTENT_COMPONENTS.has(component.library?.component ?? '')
    || component.library?.props.family === 'modal'
    || component.library?.name === 'daisyui' && ['Drawer', 'Modal'].includes(component.library.component);
}

export function slotIdForDescendant(document: WebDesignDocument, component: WebDesignComponent, containerId: string): string | undefined {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let current = component;
  while (current.parentId && current.parentId !== containerId) {
    const parent = byId.get(current.parentId);
    if (!parent) return undefined;
    current = parent;
  }
  return current.parentId === containerId ? (current.slot ?? 'content') : undefined;
}

export function componentsInSlot(document: WebDesignDocument, containerId: string, slotId: string): WebDesignComponent[] {
  return document.components.filter((component) => slotIdForDescendant(document, component, containerId) === slotId);
}

export function visibleComponentsInSlot(document: WebDesignDocument, containerId: string, slotId: string): WebDesignComponent[] {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  return componentsInSlot(document, containerId, slotId).filter((component) => {
    let parent = component.parentId ? byId.get(component.parentId) : undefined;
    while (parent && parent.id !== containerId) {
      if (isUiContentContainer(parent)) return false;
      parent = parent.parentId ? byId.get(parent.parentId) : undefined;
    }
    return true;
  });
}

function componentDepth(document: WebDesignDocument, component: WebDesignComponent): number {
  const byId = new Map(document.components.map((candidate) => [candidate.id, candidate]));
  let depth = 0;
  let parent = component.parentId ? byId.get(component.parentId) : undefined;
  while (parent) {
    depth += 1;
    parent = parent.parentId ? byId.get(parent.parentId) : undefined;
  }
  return depth;
}

export function growUiContentContainersToFit(
  document: WebDesignDocument,
  pageId: string,
  device: WebDesignDevice
): WebDesignDocument {
  const candidates = document.components
    .filter((component) => pageIdForComponent(document, component) === pageId
      && isUiContentContainer(component)
      && !isOverlayUiContentContainer(component)
      && !FIXED_CONTENT_VIEWPORTS.has(component.library?.component ?? ''))
    .sort((left, right) => componentDepth(document, right) - componentDepth(document, left));

  return candidates.reduce((current, candidate) => {
    const container = current.components.find((component) => component.id === candidate.id);
    if (!container) return current;
    const containerFrame = resolveComponent(container, device);
    let requiredWidth = containerFrame.width;
    let requiredHeight = containerFrame.height;
    let hasVisibleSlotContent = false;

    for (const slot of editableSlotsForUiComponent(container)) {
      const visible = visibleComponentsInSlot(current, container.id, slot.id);
      if (visible.length === 0) continue;
      hasVisibleSlotContent = true;
      const size = fitContentCanvasToComponents(visible, device, {
        minimumWidth: slot.width,
        minimumHeight: slot.height,
        originX: containerFrame.x,
        originY: containerFrame.y
      });
      requiredWidth = Math.max(requiredWidth, size.width);
      requiredHeight = Math.max(requiredHeight, size.height);
    }

    if (!hasVisibleSlotContent || (requiredWidth === containerFrame.width && requiredHeight === containerFrame.height)) return current;
    return {
      ...current,
      components: current.components.map((component) => component.id === container.id
        ? updateComponentFrame(component, device, { width: requiredWidth, height: requiredHeight })
        : component)
    };
  }, document);
}
