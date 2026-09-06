import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';
import type { WebComponentType, WebDesignJsonValue } from './schema.js';

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

function modeVariants(...items: Array<[string, string, string, string?]>): UiComponentVariant[] {
  return items.map(([id, label, mode, className = '']) => ({ id, label, props: { mode, className } }));
}

const commonTone = () => modeVariants(['primary', '主色', 'primary', 'primary'], ['outline', '描边', 'outline', 'outline'], ['neutral', '中性色', 'neutral', 'neutral']);
const defaultExample = () => modeVariants(['standard', '标准示例', 'standard']);

const variantOverrides: Record<string, UiComponentVariant[]> = {
  Accordion: modeVariants(['radio', '单项展开', 'radio', 'arrow'], ['arrow', '箭头折叠', 'arrow', 'arrow'], ['plus', '加号折叠', 'plus', 'plus']),
  Alert: modeVariants(['info', '信息提示', 'info', 'alert-info'], ['success', '成功提示', 'success', 'alert-success'], ['warning', '警告提示', 'warning', 'alert-warning'], ['error', '错误提示', 'error', 'alert-error']),
  Avatar: modeVariants(['single', '单头像', 'single'], ['ring', '带状态环', 'ring'], ['group', '头像组', 'group'], ['placeholder', '文字占位', 'placeholder']),
  Badge: modeVariants(['primary', '主色徽章', 'primary', 'badge-primary'], ['outline', '描边徽章', 'outline', 'badge-outline'], ['soft', '柔和徽章', 'soft', 'badge-soft'], ['dash', '虚线徽章', 'dash', 'badge-dash']),
  Button: modeVariants(['primary', '主按钮', 'primary', 'btn-primary'], ['secondary', '次按钮', 'secondary', 'btn-secondary'], ['outline', '描边按钮', 'outline', 'btn-outline'], ['soft', '柔和按钮', 'soft', 'btn-soft'], ['dash', '虚线按钮', 'dash', 'btn-dash'], ['wide', '宽按钮', 'wide', 'btn-wide']),
  Card: modeVariants(['body', '基础卡片', 'body'], ['image', '图片卡片', 'image', 'image-full'], ['side', '横向卡片', 'side', 'card-side'], ['compact', '紧凑卡片', 'compact', 'card-sm']),
  Carousel: modeVariants(['snap', '横向吸附', 'snap'], ['center', '居中轮播', 'center'], ['full', '全宽轮播', 'full']),
  Checkbox: modeVariants(['primary', '主色复选', 'primary', 'checkbox-primary'], ['success', '成功复选', 'success', 'checkbox-success'], ['indeterminate', '半选状态', 'indeterminate'], ['list', '多选清单', 'list']),
  Diff: modeVariants(['slider', '拖动对比', 'slider'], ['split', '左右分栏', 'split'], ['stacked', '上下审阅', 'stacked']),
  Divider: modeVariants(['horizontal', '水平分隔', 'horizontal'], ['labeled', '带文字', 'labeled'], ['vertical', '垂直分隔', 'vertical']),
  Drawer: modeVariants(['left', '左侧抽屉', 'left'], ['right', '右侧抽屉', 'right', 'drawer-end'], ['navigation', '导航抽屉', 'navigation']),
  Dropdown: modeVariants(['menu', '基础菜单', 'menu'], ['hover', '悬停菜单', 'hover', 'dropdown-hover'], ['end', '右对齐菜单', 'end', 'dropdown-end']),
  Fab: modeVariants(['single', '单按钮', 'single'], ['speed', '纵向 Speed Dial', 'speed'], ['flower', '花瓣操作组', 'flower']),
  FileInput: modeVariants(['bordered', '标准上传', 'bordered', 'file-input-bordered'], ['primary', '主色上传', 'primary', 'file-input-primary'], ['ghost', '透明上传', 'ghost', 'file-input-ghost']),
  Filter: modeVariants(['chips', '胶囊筛选', 'chips'], ['toolbar', '工具栏筛选', 'toolbar'], ['vertical', '纵向筛选', 'vertical']),
  Hover3D: modeVariants(['product', '产品卡片', 'product'], ['poster', '海报卡片', 'poster'], ['metric', '指标卡片', 'metric']),
  HoverGallery: modeVariants(['product', '商品画廊', 'product'], ['portfolio', '作品集画廊', 'portfolio'], ['editorial', '杂志画廊', 'editorial']),
  Input: modeVariants(['bordered', '标准输入', 'bordered', 'input-bordered'], ['ghost', '透明输入', 'ghost', 'input-ghost'], ['error', '错误状态', 'error', 'input-error'], ['search', '搜索输入', 'search']),
  Loading: modeVariants(['spinner', '旋转加载', 'spinner', 'loading-spinner'], ['dots', '点状加载', 'dots', 'loading-dots'], ['bars', '条形加载', 'bars', 'loading-bars'], ['ring', '环形加载', 'ring', 'loading-ring']),
  Mask: modeVariants(['squircle', '圆方形', 'squircle', 'mask-squircle'], ['hexagon', '六边形', 'hexagon', 'mask-hexagon'], ['heart', '心形', 'heart', 'mask-heart'], ['star', '星形', 'star', 'mask-star']),
  Modal: modeVariants(['dialog', '居中对话框', 'dialog'], ['bottom', '底部弹窗', 'bottom', 'modal-bottom'], ['sheet', '全宽面板', 'sheet']),
  MockupBrowser: modeVariants(['website', '网站预览', 'website'], ['dashboard', '仪表盘预览', 'dashboard'], ['code', '开发预览', 'code']),
  MockupCode: modeVariants(['terminal', '终端输出', 'terminal'], ['diff', '代码差异', 'diff'], ['install', '安装命令', 'install']),
  MockupPhone: modeVariants(['app', '移动应用', 'app'], ['commerce', '电商应用', 'commerce'], ['social', '社交应用', 'social']),
  MockupWindow: modeVariants(['canvas', '设计画布', 'canvas'], ['analytics', '数据窗口', 'analytics'], ['editor', '编辑器窗口', 'editor']),
  Otp: modeVariants(['six', '六位验证码', 'six'], ['four', '四位验证码', 'four'], ['masked', '安全验证码', 'masked']),
  Progress: modeVariants(['primary', '主色进度', 'primary', 'progress-primary'], ['success', '成功进度', 'success', 'progress-success'], ['steps', '分段进度', 'steps']),
  RadialProgress: modeVariants(['primary', '主色环形', 'primary'], ['thick', '粗环形', 'thick'], ['metric', '指标环形', 'metric']),
  Range: modeVariants(['primary', '主色滑块', 'primary', 'range-primary'], ['steps', '带刻度滑块', 'steps'], ['dual', '区间展示', 'dual']),
  Rating: modeVariants(['stars', '星级评分', 'stars'], ['hearts', '心形评分', 'hearts', 'mask-heart'], ['half', '半星评分', 'half']),
  Select: modeVariants(['bordered', '标准选择', 'bordered', 'select-bordered'], ['ghost', '透明选择', 'ghost', 'select-ghost'], ['error', '错误状态', 'error', 'select-error'], ['grouped', '分组选择', 'grouped']),
  Steps: modeVariants(['horizontal', '横向步骤', 'horizontal'], ['vertical', '纵向步骤', 'vertical', 'steps-vertical'], ['progress', '状态步骤', 'progress']),
  Tab: modeVariants(['border', '边框标签', 'border', 'tabs-border'], ['lift', '凸起标签', 'lift', 'tabs-lift'], ['box', '盒式标签', 'box', 'tabs-box']),
  Table: modeVariants(['zebra', '斑马纹表格', 'zebra', 'table-zebra'], ['pinned', '固定表头', 'pinned', 'table-pin-rows'], ['compact', '紧凑表格', 'compact', 'table-sm']),
  TextRotate: modeVariants(['inline', '行内轮换', 'inline'], ['hero', '主视觉轮换', 'hero'], ['badge', '徽章轮换', 'badge']),
  ThemeController: modeVariants(['toggle', '开关主题', 'toggle'], ['buttons', '按钮主题', 'buttons'], ['cards', '主题卡片', 'cards']),
  Timeline: modeVariants(['horizontal', '横向时间轴', 'horizontal'], ['vertical', '纵向时间轴', 'vertical', 'timeline-vertical'], ['compact', '紧凑记录', 'compact']),
  Toast: modeVariants(['end', '右下通知', 'end', 'toast-end'], ['center', '顶部居中', 'center', 'toast-center toast-top'], ['stack', '通知堆叠', 'stack']),
  Tooltip: modeVariants(['top', '顶部提示', 'top', 'tooltip-top'], ['right', '右侧提示', 'right', 'tooltip-right'], ['open', '常显提示', 'open', 'tooltip-open']),
  Validator: modeVariants(['error', '错误校验', 'error'], ['success', '成功校验', 'success'], ['password', '密码规则', 'password'])
};

function variantsForSeed(seed: DaisySeed): UiComponentVariant[] {
  if (variantOverrides[seed.id]) return variantOverrides[seed.id];
  if (['button', 'badge', 'link', 'status', 'toggle', 'radio'].includes(seed.family)) return commonTone();
  return defaultExample();
}

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

export const DAISYUI_COMPONENT_VARIANTS: Record<string, UiComponentVariant[]> = Object.fromEntries(orderedSeeds.map((seed) => [seed.id, variantsForSeed(seed)]));

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
