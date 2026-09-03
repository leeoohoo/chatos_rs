import { contentSlot, namedItemSlots, numericLibraryProp, splitPanelSlots, type UiEditableSlot } from './ui-library';
import type { WebDesignComponent } from './schema';

export type AntdEditableSlot = UiEditableSlot;

export function editableSlotsForAntdComponent(component: WebDesignComponent): AntdEditableSlot[] {
  if (component.library?.name !== 'antd') return [];
  const name = component.library.component;
  const props = component.library.props;
  const contentWidth = Math.max(280, component.width - 32);
  const contentHeight = Math.max(180, component.height - 56);

  if (name === 'Drawer') {
    const horizontal = props.placement === 'top' || props.placement === 'bottom';
    return [{
      id: 'content', label: '抽屉内容', description: '表单、详情、导航和操作区域',
      width: horizontal ? 760 : Math.max(280, numericLibraryProp(props.width, 420) - 48),
      height: horizontal ? Math.max(180, numericLibraryProp(props.height, 300) - 88) : 620
    }];
  }
  if (name === 'Modal') return [contentSlot(component, 'content', '对话框内容', '正文、表单和操作内容', { width: Math.max(320, numericLibraryProp(props.width, 520) - 48), height: 420 })];
  if (name === 'Card') return [contentSlot(component, 'content', '卡片内容', '卡片主体区域')];
  if (name === 'Form') return [contentSlot(component, 'content', '表单内容', '输入项、说明和提交操作', { height: Math.max(260, component.height) })];
  if (name === 'App') return [contentSlot(component, 'content', '应用内容', '继承 App 上下文的页面或功能组件')];
  if (name === 'ConfigProvider') return [contentSlot(component, 'content', '全局配置内容', '应用统一尺寸、方向、禁用状态与主题的组件')];
  if (name === 'BorderBeam') return [contentSlot(component, 'content', '流光容器内容', '需要突出显示的卡片、AI 模块或关键行动区域')];
  if (name === 'Popover') return [contentSlot(component, 'popup', '气泡内容', '点击或悬停后展示的内容', { width: 320, height: 220 })];
  if (name === 'Watermark') return [contentSlot(component, 'content', '水印内容', '水印覆盖下的页面内容')];
  if (name === 'Result') return [contentSlot(component, 'extra', '结果操作区', '结果说明下方的按钮和附加内容', { height: 160 })];
  if (name === 'Flex' || name === 'Space' || name === 'Grid') return [contentSlot(component, 'content', `${name} 内容`, '容器内排列的组件')];
  if (name === 'Splitter') return splitPanelSlots(component);
  if (name === 'Layout') {
    const slots: AntdEditableSlot[] = [
      { id: 'header', label: 'Header', description: '页面顶部区域', width: contentWidth, height: 88 },
      { id: 'content', label: 'Content', description: '页面主体区域', width: Math.max(320, contentWidth * .7), height: contentHeight }
    ];
    if (component.library.variant !== 'top') slots.push({ id: 'sider', label: 'Sider', description: '侧边栏区域', width: Math.max(220, contentWidth * .28), height: contentHeight });
    return slots;
  }
  if (name === 'Tabs') return namedItemSlots(component, 'tab', '标签页');
  if (name === 'Collapse') return namedItemSlots(component, 'panel', '折叠面板');
  if (name === 'Carousel') {
    return ['产品设计', 'AI 协作', '代码交付'].map((label, index) => ({
      id: `slide-${index + 1}`, label, description: `第 ${index + 1} 张轮播内容`, width: contentWidth, height: contentHeight
    }));
  }
  return [];
}

export function isAntdContentContainer(component: WebDesignComponent): boolean {
  return editableSlotsForAntdComponent(component).length > 0;
}
