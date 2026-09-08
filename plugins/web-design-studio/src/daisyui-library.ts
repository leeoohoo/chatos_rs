import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';
import type { WebComponentType, WebDesignJsonValue } from './schema.js';
import { DAISYUI_OFFICIAL_COMPONENT_VARIANTS } from './daisyui-registry.generated.js';

export const DAISYUI_VERSION = '5.7.28';
export const DAISYUI_LICENSE = 'MIT';
export const DAISYUI_CATEGORIES = ['操作', '数据录入', '数据展示', '导航', '反馈', '布局', '视觉与模拟器'] as const;
export type DaisyUiCategory = (typeof DAISYUI_CATEGORIES)[number];

type DaisySeed = {
  id: string;
  slug: string;
  label: string;
  category: DaisyUiCategory;
  family: string;
  icon: string;
  baseType?: WebComponentType;
  width?: number;
  height?: number;
  props?: Record<string, WebDesignJsonValue>;
};

const seeds: DaisySeed[] = [
  { id: 'Accordion', slug: 'accordion', label: '手风琴', category: '数据展示', family: 'accordion', icon: '⌄', height: 280 },
  { id: 'Alert', slug: 'alert', label: '提示条', category: '反馈', family: 'alert', icon: '!', height: 150 },
  { id: 'Aura', slug: 'aura', label: '光环边框', category: '视觉与模拟器', family: 'aura', icon: '✦', height: 220 },
  { id: 'Avatar', slug: 'avatar', label: '头像', category: '数据展示', family: 'avatar', icon: '●', width: 300, height: 150 },
  { id: 'Badge', slug: 'badge', label: '徽章', category: '数据展示', family: 'badge', icon: '◆', width: 260, height: 110 },
  { id: 'Breadcrumbs', slug: 'breadcrumbs', label: '面包屑', category: '导航', family: 'breadcrumbs', icon: '›', height: 110 },
  { id: 'Button', slug: 'button', label: '按钮', category: '操作', family: 'button', icon: '▣', baseType: 'button', width: 280, height: 110 },
  { id: 'Calendar', slug: 'calendar', label: '日历', category: '数据录入', family: 'calendar', icon: '▦', height: 340, props: { selectedDay: 18 } },
  { id: 'Card', slug: 'card', label: '卡片', category: '数据展示', family: 'card', icon: '◇', height: 300 },
  { id: 'Carousel', slug: 'carousel', label: '轮播', category: '数据展示', family: 'carousel', icon: '▣', width: 520, height: 300, props: { items: ['Product', 'Design', 'Motion', 'Launch'] } },
  { id: 'Chat', slug: 'chat', label: '聊天气泡', category: '数据展示', family: 'chat', icon: '“', height: 260 },
  { id: 'Checkbox', slug: 'checkbox', label: '复选框', category: '数据录入', family: 'checkbox', icon: '☑', baseType: 'checkbox', width: 300, height: 120 },
  { id: 'Collapse', slug: 'collapse', label: '折叠面板', category: '数据展示', family: 'collapse', icon: '⌄', height: 210 },
  { id: 'Countdown', slug: 'countdown', label: '倒计时', category: '数据展示', family: 'countdown', icon: '#', height: 160 },
  { id: 'Diff', slug: 'diff', label: '前后对比', category: '数据展示', family: 'diff', icon: '↔', width: 520, height: 300 },
  { id: 'Divider', slug: 'divider', label: '分隔线', category: '布局', family: 'divider', icon: '—', height: 120 },
  { id: 'Dock', slug: 'dock', label: '底部程序坞', category: '导航', family: 'dock', icon: '▬', width: 480, height: 130 },
  { id: 'Drawer', slug: 'drawer', label: '抽屉侧栏', category: '布局', family: 'drawer', icon: '▤', width: 520, height: 340 },
  { id: 'Dropdown', slug: 'dropdown', label: '下拉菜单', category: '操作', family: 'dropdown', icon: '⌄', height: 240 },
  { id: 'Fab', slug: 'fab', label: '浮动操作按钮', category: '操作', family: 'fab', icon: '＋', width: 320, height: 260 },
  { id: 'Fieldset', slug: 'fieldset', label: '字段组', category: '数据录入', family: 'fieldset', icon: '▤', height: 300 },
  { id: 'FileInput', slug: 'file-input', label: '文件输入', category: '数据录入', family: 'file-input', icon: '↑', baseType: 'input', height: 150 },
  { id: 'Filter', slug: 'filter', label: '筛选器', category: '操作', family: 'filter', icon: '⌁', height: 150, props: { items: ['全部', '设计', '开发', '已发布'] } },
  { id: 'Footer', slug: 'footer', label: '页脚', category: '布局', family: 'footer', icon: '▥', width: 560, height: 300 },
  { id: 'Hero', slug: 'hero', label: '主视觉', category: '布局', family: 'hero', icon: 'H', width: 580, height: 340 },
  { id: 'Hover3D', slug: 'hover-3d', label: '3D 悬浮卡片', category: '视觉与模拟器', family: 'hover-3d', icon: '◇', height: 280 },
  { id: 'HoverGallery', slug: 'hover-gallery', label: '悬浮画廊', category: '视觉与模拟器', family: 'hover-gallery', icon: '▦', width: 520, height: 310 },
  { id: 'Indicator', slug: 'indicator', label: '角标指示器', category: '数据展示', family: 'indicator', icon: '◉', width: 300, height: 160 },
  { id: 'Input', slug: 'input', label: '文本输入', category: '数据录入', family: 'input', icon: 'I', baseType: 'input', height: 140, props: { placeholder: '输入网站名称' } },
  { id: 'Join', slug: 'join', label: '组合控件', category: '布局', family: 'join', icon: '▥', height: 150 },
  { id: 'Kbd', slug: 'kbd', label: '键盘按键', category: '数据展示', family: 'kbd', icon: '⌘', width: 300, height: 120 },
  { id: 'Label', slug: 'label', label: '表单标签', category: '数据录入', family: 'label', icon: 'L', height: 130 },
  { id: 'Link', slug: 'link', label: '链接', category: '导航', family: 'link', icon: '↗', baseType: 'button', width: 260, height: 100 },
  { id: 'List', slug: 'list', label: '列表', category: '数据展示', family: 'list', icon: '☷', height: 300, props: { items: ['设计系统评审', '移动端适配', '发布前检查'] } },
  { id: 'Loading', slug: 'loading', label: '加载动画', category: '反馈', family: 'loading', icon: '◌', width: 300, height: 150 },
  { id: 'Mask', slug: 'mask', label: '图形蒙版', category: '视觉与模拟器', family: 'mask', icon: '◆', height: 230 },
  { id: 'Megamenu', slug: 'megamenu', label: '大型菜单', category: '导航', family: 'megamenu', icon: '☰', width: 580, height: 300 },
  { id: 'Menu', slug: 'menu', label: '菜单', category: '导航', family: 'menu', icon: '☰', height: 300 },
  { id: 'MockupBrowser', slug: 'mockup-browser', label: '浏览器模型', category: '视觉与模拟器', family: 'mockup-browser', icon: '▯', width: 560, height: 330 },
  { id: 'MockupCode', slug: 'mockup-code', label: '代码模型', category: '视觉与模拟器', family: 'mockup-code', icon: '⌘', width: 520, height: 280 },
  { id: 'MockupPhone', slug: 'mockup-phone', label: '手机模型', category: '视觉与模拟器', family: 'mockup-phone', icon: '▯', width: 320, height: 540 },
  { id: 'MockupWindow', slug: 'mockup-window', label: '窗口模型', category: '视觉与模拟器', family: 'mockup-window', icon: '▣', width: 540, height: 320 },
  { id: 'Modal', slug: 'modal', label: '模态框', category: '反馈', family: 'modal', icon: '▣', height: 320 },
  { id: 'Navbar', slug: 'navbar', label: '导航栏', category: '导航', family: 'navbar', icon: '▬', width: 580, height: 150 },
  { id: 'Otp', slug: 'otp', label: '验证码输入', category: '数据录入', family: 'otp', icon: '#', height: 160 },
  { id: 'Pagination', slug: 'pagination', label: '分页', category: '导航', family: 'pagination', icon: '•••', height: 130 },
  { id: 'Progress', slug: 'progress', label: '进度条', category: '数据展示', family: 'progress', icon: '━', height: 140, props: { value: 64 } },
  { id: 'RadialProgress', slug: 'radial-progress', label: '环形进度', category: '数据展示', family: 'radial-progress', icon: '◔', width: 280, height: 210, props: { value: 72 } },
  { id: 'Radio', slug: 'radio', label: '单选框', category: '数据录入', family: 'radio', icon: '◉', height: 130 },
  { id: 'Range', slug: 'range', label: '范围滑块', category: '数据录入', family: 'range', icon: '━', height: 140, props: { value: 58 } },
  { id: 'Rating', slug: 'rating', label: '评分', category: '数据录入', family: 'rating', icon: '★', height: 140 },
  { id: 'Select', slug: 'select', label: '选择器', category: '数据录入', family: 'select', icon: '⌄', baseType: 'select', height: 150, props: { options: ['Design', 'Engineering', 'Marketing'] } },
  { id: 'Skeleton', slug: 'skeleton', label: '骨架屏', category: '反馈', family: 'skeleton', icon: '▧', height: 220 },
  { id: 'Stack', slug: 'stack', label: '堆叠', category: '布局', family: 'stack', icon: '▤', height: 250 },
  { id: 'Stat', slug: 'stat', label: '统计数值', category: '数据展示', family: 'stat', icon: '#', height: 210 },
  { id: 'Status', slug: 'status', label: '状态点', category: '数据展示', family: 'status', icon: '●', width: 280, height: 120 },
  { id: 'Steps', slug: 'steps', label: '步骤条', category: '导航', family: 'steps', icon: '①', width: 520, height: 180 },
  { id: 'Swap', slug: 'swap', label: '内容切换', category: '操作', family: 'swap', icon: '⇄', width: 280, height: 140 },
  { id: 'Tab', slug: 'tab', label: '标签页', category: '导航', family: 'tabs', icon: '▤', height: 240, props: { items: ['概览', '功能', '设置'] } },
  { id: 'Table', slug: 'table', label: '表格', category: '数据展示', family: 'table', icon: '▦', width: 560, height: 300 },
  { id: 'TextRotate', slug: 'text-rotate', label: '轮换文字', category: '视觉与模拟器', family: 'text-rotate', icon: 'T', height: 160, props: { items: ['更快', '更美', '更智能'] } },
  { id: 'Textarea', slug: 'textarea', label: '多行输入', category: '数据录入', family: 'textarea', icon: '▤', baseType: 'textarea', height: 190 },
  { id: 'ThemeController', slug: 'theme-controller', label: '主题控制器', category: '操作', family: 'theme-controller', icon: '◐', width: 340, height: 150 },
  { id: 'Timeline', slug: 'timeline', label: '时间轴', category: '数据展示', family: 'timeline', icon: '│', width: 540, height: 290 },
  { id: 'Toast', slug: 'toast', label: '浮动通知', category: '反馈', family: 'toast', icon: '▢', height: 240 },
  { id: 'Toggle', slug: 'toggle', label: '切换开关', category: '数据录入', family: 'toggle', icon: '◉', baseType: 'switch', width: 300, height: 130 },
  { id: 'Tooltip', slug: 'tooltip', label: '文字提示', category: '反馈', family: 'tooltip', icon: '?', width: 320, height: 170 },
  { id: 'Validator', slug: 'validator', label: '输入校验', category: '数据录入', family: 'validator', icon: '✓', height: 180 }
];

const orderedSeeds = DAISYUI_CATEGORIES.flatMap((category) => seeds.filter((seed) => seed.category === category));
export const DAISYUI_COMPONENT_SLUGS = orderedSeeds.map((seed) => seed.slug);

export const DAISYUI_COMPONENTS: UiComponentDefinition<DaisyUiCategory>[] = orderedSeeds.map((seed) => ({
  ...defineUiComponent(
    seed.id,
    seed.label,
    seed.category,
    seed.icon,
    seed.baseType ?? 'card',
    seed.width ?? 420,
    seed.height ?? 220,
    seed.label,
    {
      family: seed.family,
      componentSlug: seed.slug,
      title: seed.label,
      items: ['设计', '开发', '发布'],
      ...(seed.props ?? {})
    },
    ['daisyUI', seed.slug, seed.family]
  ),
  docsUrl: `https://daisyui.com/components/${seed.slug}/`
}));

export const DAISYUI_COMPONENT_VARIANTS: Record<string, UiComponentVariant[]> = Object.fromEntries(orderedSeeds.map((seed) => [
  seed.id,
  ((DAISYUI_OFFICIAL_COMPONENT_VARIANTS as Record<string, readonly UiComponentVariant[]>)[seed.id] ?? []).map((variant) => ({ ...variant, props: { ...variant.props } }))
]));

export const DAISYUI_LIBRARY: UiLibraryCatalog<DaisyUiCategory> = {
  id: 'daisyui',
  displayName: 'daisyUI',
  shortName: 'daisyUI',
  version: DAISYUI_VERSION,
  brandMark: 'D',
  categories: DAISYUI_CATEGORIES,
  components: DAISYUI_COMPONENTS,
  variants: DAISYUI_COMPONENT_VARIANTS,
  license: DAISYUI_LICENSE,
  sourceUrl: 'https://github.com/saadeghi/daisyui',
  licenseUrl: 'https://github.com/saadeghi/daisyui/blob/master/LICENSE'
};

export function createDaisyUiComponent(definitionId: string, x: number, y: number) { return createUiLibraryComponent(DAISYUI_LIBRARY, definitionId, x, y); }
export function variantsForDaisyUiComponent(definitionId: string) { return variantsForUiComponent(DAISYUI_LIBRARY, definitionId); }
export function applyDaisyUiComponentVariant(component: Parameters<typeof applyUiComponentVariant>[1], variantId: string) { return applyUiComponentVariant(DAISYUI_LIBRARY, component, variantId); }
