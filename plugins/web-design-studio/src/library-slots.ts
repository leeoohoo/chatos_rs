import { editableSlotsForAntdComponent } from './antd-slots.js';
import type { WebDesignComponent } from './schema.js';
import { contentSlot, namedItemSlots, splitPanelSlots, type UiEditableSlot } from './ui-library.js';

const CHAKRA_CONTENT_SLOTS: Record<string, [string, string]> = {
  Box: ['盒子内容', '盒子内部的自由内容'],
  Container: ['容器内容', '限制宽度的页面内容'],
  Flex: ['Flex 内容', '使用弹性布局组织组件'],
  Grid: ['Grid 内容', '使用栅格组织组件'],
  SimpleGrid: ['栅格内容', '自动等分的栅格内容'],
  Stack: ['Stack 内容', '垂直或水平堆叠的组件'],
  Group: ['组合内容', '组合在一起的操作组件'],
  ScrollArea: ['滚动内容', '可向下滚动的内容区域'],
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

  if (library === 'chakra') {
    const single = CHAKRA_CONTENT_SLOTS[name];
    if (single) return [contentSlot(component, 'content', single[0], single[1])];
    if (name === 'Splitter') return splitPanelSlots(component);
    if (name === 'Tabs') return namedItemSlots(component, 'tab', '标签页');
    if (name === 'Accordion') return namedItemSlots(component, 'panel', '手风琴面板');
    if (name === 'Dialog') return [contentSlot(component, 'content', '对话框内容', '正文、表单和操作区域', { width: 480, height: 400 })];
    if (name === 'Drawer') return overlaySlot(component, '抽屉内容', '表单、详情、导航和操作区域');
    if (name === 'Popover') return [contentSlot(component, 'popup', '气泡内容', '点击后展示的自由内容', { width: 320, height: 220 })];
  }

  if (library === 'shadcn') {
    const single = SHADCN_CONTENT_SLOTS[name];
    if (single) return [contentSlot(component, 'content', single[0], single[1])];
    if (name === 'Resizable') return splitPanelSlots(component);
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
