import type { WebDesignComponent } from './schema.js';
import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';
import { CHAKRA_OFFICIAL_COMPONENT_VARIANTS } from './chakra-registry.generated.js';

export type ChakraCategory = '布局' | '排版' | '按钮' | '数据录入' | '导航' | '数据展示' | '反馈' | '浮层' | '国际化' | '工具';
export type ChakraComponentDefinition = UiComponentDefinition<ChakraCategory>;
export type ChakraComponentVariant = UiComponentVariant;

export const CHAKRA_VERSION = '3.37.0';
export const CHAKRA_CATEGORIES: ChakraCategory[] = ['布局', '排版', '按钮', '数据录入', '导航', '数据展示', '反馈', '浮层', '国际化', '工具'];
const item = defineUiComponent<ChakraCategory>;
const listItems = [
  { key: 'composition', label: '组合式组件' },
  { key: 'tokens', label: '主题令牌' },
  { key: 'accessibility', label: '无障碍交互' }
];
const navItems = [{ key: 'overview', label: '概览' }, { key: 'features', label: '功能' }, { key: 'settings', label: '设置' }];
const accordionItems = [{ key: 'design', label: '设计系统' }, { key: 'collaboration', label: 'AI 协作' }, { key: 'delivery', label: '交付能力' }];

export const CHAKRA_COMPONENTS: ChakraComponentDefinition[] = [
  item('AspectRatio', '比例容器', '布局', '▰', 'section', 480, 270, '', { ratio: 16 / 9, kind: 'video' }, ['媒体比例']),
  item('Bleed', '溢出布局', '布局', '↔', 'section', 420, 180, '', { inline: 6, block: 0, colorPalette: 'blue' }, ['负边距']),
  item('AbsoluteCenter', '绝对居中', '布局', '⊙', 'section', 320, 180, '居中内容', { axis: 'both', colorPalette: 'blue' }),
  item('Center', '居中容器', '布局', '◎', 'section', 320, 160, '居中内容', { inline: false, colorPalette: 'blue' }),
  item('Float', '浮动定位', '布局', '◉', 'section', 340, 180, '新', { placement: 'top-end', offset: 3, colorPalette: 'red' }),
  item('Wrap', '自动换行', '布局', '↵', 'section', 440, 180, '', { justify: 'start', align: 'center', direction: 'row', gap: 3, itemCount: 8 }),
  item('Box', '盒子', '布局', '□', 'section', 360, 140, '', {}, ['容器']),
  item('Container', '内容容器', '布局', '▭', 'section', 480, 180, '', { maxWidth: 'lg', centerContent: false }),
  item('Flex', '弹性布局', '布局', '⇥', 'section', 400, 120, '', { direction: 'row', gap: 4, align: 'center', justify: 'start', wrap: 'nowrap' }),
  item('Grid', '栅格布局', '布局', '▦', 'section', 440, 160, '', { columns: 3, gap: 4 }),
  item('SimpleGrid', '简易栅格', '布局', '▦', 'section', 440, 160, '', { columns: 3, gap: 4 }),
  item('Stack', '堆叠布局', '布局', '☰', 'section', 360, 180, '', { direction: 'column', gap: 4, align: 'stretch' }),
  item('Group', '组件组合', '布局', '▣', 'section', 360, 80, '', { attached: false, orientation: 'horizontal' }),
  item('Separator', '分隔线', '布局', '—', 'divider', 360, 28, '', { orientation: 'horizontal', variant: 'solid' }),
  item('ScrollArea', '滚动区域', '布局', '↕', 'section', 360, 220, '', { maxHeight: 220 }),
  item('Splitter', '分隔面板', '布局', '⋮', 'section', 460, 220, '', { orientation: 'horizontal', defaultSizes: [45, 55] }),

  item('Heading', '标题', '排版', 'H', 'heading', 360, 64, '构建漂亮的网站', { size: '2xl', level: 2 }),
  item('Text', '正文', '排版', 'T', 'text', 360, 72, 'Chakra UI 提供可组合、可访问并且适合主题化的界面基础。', { textStyle: 'md' }),
  item('Code', '代码', '排版', '</>', 'text', 260, 44, 'npm install @chakra-ui/react', { variant: 'subtle', colorPalette: 'gray' }),
  item('CodeBlock', '代码块', '排版', '{ }', 'card', 500, 220, 'export function App() {\n  return <Button>开始设计</Button>\n}', { language: 'tsx', title: 'app.tsx', showHeader: false, showLineNumbers: false }),
  item('Em', '强调文本', '排版', 'I', 'text', 300, 48, '这是一段需要强调的内容。', { color: 'fg' }),
  item('Highlight', '文本高亮', '排版', '▱', 'text', 440, 72, 'AI 与设计师协作，可以更快设计出漂亮的网站。', { query: ['AI'], colorPalette: 'yellow', ignoreCase: true }),
  item('LinkOverlay', '链接覆盖层', '排版', '↗', 'card', 380, 150, '查看产品设计系统', { href: '#product', external: false, variant: 'card' }),
  item('Mark', '文本标记', '排版', '▰', 'text', 320, 48, '重要设计决策', { colorPalette: 'yellow', variant: 'subtle' }),
  item('Prose', '文章排版', '排版', '¶', 'section', 560, 360, '', { size: 'md', maxWidth: '65ch', showTable: false }, ['官方 snippet']),
  item('RichTextEditor', '富文本编辑器', '排版', '✎', 'textarea', 620, 300, '<h2>欢迎使用网站设计工作台</h2><p>选中文字后，可以使用工具栏调整格式。</p>', { toolbar: ['bold', 'italic', 'strike', 'code'], editable: true, showFooter: false, placeholder: '开始输入内容…' }, ['Tiptap', '官方 snippet']),
  item('Blockquote', '引用', '排版', '❝', 'card', 420, 110, '优秀的设计系统让产品团队更专注于用户价值。', { cite: 'Web Design Studio' }),
  item('Kbd', '键盘按键', '排版', '⌘', 'badge', 120, 40, '⌘ K', {}),
  item('Link', '链接', '排版', '↗', 'link', 180, 42, '查看完整文档', { colorPalette: 'blue', variant: 'underline' }),
  item('List', '列表', '排版', '☷', 'list', 360, 150, '', { items: listItems, ordered: false, variant: 'marker', align: 'start', indicator: 'none', gap: 2, unstyled: false }),

  item('Button', '按钮', '按钮', '▣', 'button', 150, 44, '主要操作', { variant: 'solid', colorPalette: 'blue', size: 'md' }),
  item('IconButton', '图标按钮', '按钮', '✦', 'button', 48, 44, '＋', { variant: 'solid', colorPalette: 'blue', size: 'md' }),
  item('CloseButton', '关闭按钮', '按钮', '×', 'button', 44, 44, '', { size: 'md', variant: 'ghost', colorPalette: 'gray' }),
  item('DownloadTrigger', '下载触发器', '按钮', '⇩', 'button', 170, 44, '下载文件', { fileName: 'design-notes.txt', mimeType: 'text/plain', data: '由 Web Design Studio 生成的设计说明。', variant: 'solid', colorPalette: 'blue' }),

  item('DateInput', '日期输入', '数据录入', '◷', 'input', 320, 78, '', { label: '出生日期', size: 'md', locale: 'zh-CN', granularity: 'day' }),
  item('DatePicker', '日期选择器', '数据录入', '▣', 'input', 320, 78, '', { label: '交付日期', size: 'md', selectionMode: 'single', locale: 'zh-CN' }),
  item('Calendar', '日历', '数据录入', '▦', 'card', 360, 360, '', { size: 'md', selectionMode: 'single', locale: 'zh-CN', hideOutsideDays: false, showWeekNumbers: false }),
  item('CheckboxCard', '多选卡片', '数据录入', '☑', 'checkbox', 340, 118, '', { variant: 'outline', colorPalette: 'blue', size: 'md', defaultChecked: false, title: '专业版', description: '适合完整产品设计。' }),
  item('ColorPicker', '颜色选择器', '数据录入', '◒', 'input', 300, 82, '', { label: '品牌主色', defaultValue: '#5D50DF', format: 'rgba', size: 'md', showAlpha: false }),
  item('ColorSwatch', '颜色色板', '数据录入', '●', 'badge', 48, 48, '', { value: '#5D50DF', size: 'lg', shape: 'rounded', showCheck: false }),
  item('Field', '表单字段', '数据录入', '▤', 'input', 320, 82, '请输入内容', { label: '电子邮箱', helperText: '', errorText: '', required: false, invalid: false, disabled: false }),
  item('FileUpload', '文件上传', '数据录入', '↑', 'input', 320, 92, '', { accept: 'image/*', multiple: false, maxFiles: 1, kind: 'button', label: '上传图片' }),
  item('NumberInput', '数字输入', '数据录入', '#', 'input', 220, 48, '', { defaultValue: '10', min: 0, max: 100, step: 1, size: 'md', format: 'decimal' }),
  item('PasswordInput', '密码输入', '数据录入', '••', 'input', 320, 48, '', { placeholder: '请输入密码', size: 'md', variant: 'outline', defaultVisible: false, showStrength: false }),
  item('PinInput', '验证码输入', '数据录入', '①', 'input', 300, 52, '', { count: 4, size: 'md', type: 'numeric', mask: false }),
  item('RadioCard', '单选卡片', '数据录入', '◉', 'checkbox', 460, 120, '', { orientation: 'horizontal', variant: 'outline', colorPalette: 'blue', size: 'md', defaultValue: 'react' }),
  item('Rating', '评分', '数据录入', '★', 'input', 220, 48, '', { count: 5, defaultValue: 3, size: 'md', colorPalette: 'yellow', allowHalf: false }),
  item('SegmentedControl', '分段控制器', '数据录入', '▥', 'input', 340, 44, '', { size: 'md', defaultValue: 'preview', orientation: 'horizontal', items: [{ value: 'design', label: '设计' }, { value: 'preview', label: '预览' }, { value: 'code', label: '代码' }] }),
  item('TagsInput', '标签输入', '数据录入', '◆', 'input', 380, 86, '', { label: '技术标签', defaultValue: ['React', 'Chakra', 'TypeScript'], size: 'md', max: 8, placeholder: '添加标签…' }),
  item('Combobox', '组合搜索框', '数据录入', '⌕', 'select', 340, 78, '', { label: '技术框架', placeholder: '输入并搜索', multiple: false, size: 'md' }),
  item('Listbox', '列表选择框', '数据录入', '☷', 'select', 340, 220, '', { label: '选择框架', selectionMode: 'single', orientation: 'vertical', defaultValue: ['react'] }),
  item('Select', '选择器', '数据录入', '⌄', 'select', 340, 78, '', { label: '技术框架', placeholder: '请选择框架', multiple: false, size: 'md', variant: 'outline' }),
  item('TreeView', '树视图', '数据录入', '⌁', 'list', 340, 280, '', { label: '项目文件', selectionMode: 'single', defaultExpandedValue: ['src'], showGuide: true }),
  item('Input', '输入框', '数据录入', '⌨', 'input', 280, 44, '请输入内容', { variant: 'outline', size: 'md' }),
  item('Textarea', '多行输入框', '数据录入', '▤', 'textarea', 320, 100, '请输入详细说明', { variant: 'outline', size: 'md', rows: 4 }),
  item('NativeSelect', '原生选择器', '数据录入', '⌄', 'select', 280, 44, '请选择方案', { variant: 'outline', size: 'md', options: [{ value: 'design', label: '产品设计' }, { value: 'frontend', label: '前端开发' }, { value: 'ai', label: 'AI 协作' }] }),
  item('Checkbox', '多选框', '数据录入', '☑', 'checkbox', 220, 44, '接收产品更新', { defaultChecked: true, colorPalette: 'blue' }),
  item('Switch', '开关', '数据录入', '◉', 'switch', 200, 44, '启用通知', { defaultChecked: true, colorPalette: 'blue' }),
  item('RadioGroup', '单选组', '数据录入', '◉', 'checkbox', 360, 70, '', { defaultValue: 'monthly', options: [{ value: 'monthly', label: '月付' }, { value: 'yearly', label: '年付' }, { value: 'enterprise', label: '企业版' }] }),
  item('Slider', '滑块', '数据录入', '━', 'input', 300, 54, '', { defaultValue: 58, min: 0, max: 100, colorPalette: 'blue' }),
  item('Fieldset', '字段组', '数据录入', '▤', 'section', 400, 240, '', { legend: '个人资料', helperText: '这些信息会展示在个人页面。' }),
  item('Editable', '可编辑文本', '数据录入', '✎', 'input', 300, 48, '点击编辑名称', { placeholder: '输入名称' }),

  item('Breadcrumb', '面包屑', '导航', '›', 'list', 340, 44, '', { items: [{ key: 'home', label: '首页' }, { key: 'products', label: '产品' }, { key: 'detail', label: '详情' }] }),
  item('Pagination', '分页', '导航', '•••', 'list', 340, 48, '', { count: 10, pageSize: 1, defaultPage: 3 }),
  item('Steps', '步骤', '导航', '①', 'list', 460, 78, '', { defaultStep: 1, items: [{ key: 'account', label: '账号' }, { key: 'profile', label: '资料' }, { key: 'done', label: '完成' }] }),
  item('Tabs', '标签页', '导航', '▤', 'card', 440, 190, '', { defaultValue: 'overview', items: navItems }),
  item('Accordion', '手风琴', '导航', '⌄', 'list', 420, 220, '', { defaultValue: ['design'], items: accordionItems, collapsible: true }),
  item('Collapsible', '折叠区域', '导航', '⌄', 'section', 380, 150, '展开更多内容', { defaultOpen: true }),
  item('Carousel', '轮播', '导航', '▣', 'card', 480, 260, '', { slideCount: 5, slidesPerPage: 1, loop: false, autoplay: false }),

  item('Avatar', '头像', '数据展示', '●', 'avatar', 64, 64, 'AI', { size: 'lg', name: 'AI Designer' }),
  item('Badge', '徽标', '数据展示', '◆', 'badge', 100, 36, '已发布', { colorPalette: 'green', variant: 'subtle' }),
  item('Card', '卡片', '数据展示', '▤', 'card', 360, 190, '清晰组织标题、说明和操作。', { variant: 'elevated', title: '产品卡片' }),
  item('Table', '表格', '数据展示', '▦', 'table', 520, 230, '', { striped: true, columns: ['项目', '状态', '负责人'], rows: [['设计系统', '进行中', '小林'], ['组件接入', '已完成', 'AI'], ['体验验收', '待处理', '产品']] }),
  item('Stat', '统计值', '数据展示', '#', 'card', 230, 110, '', { label: '本月活跃用户', value: '28,642', change: '+12.5%' }),
  item('Timeline', '时间轴', '数据展示', '│', 'list', 320, 190, '', { items: [{ key: '1', label: '完成需求分析', description: '10:20' }, { key: '2', label: '建立设计系统', description: '11:45' }, { key: '3', label: '开始组件接入', description: '14:10' }] }),
  item('Clipboard', '剪贴板', '数据展示', '⧉', 'button', 180, 44, '', { value: 'https://chakra-ui.com', kind: 'button', label: '复制链接' }),
  item('Image', '图片', '数据展示', '▧', 'image', 420, 240, '', { alt: '网站设计预览', fit: 'cover', borderRadius: 'lg', aspect: 'landscape' }),
  item('DataList', '数据列表', '数据展示', '☷', 'list', 360, 160, '', { orientation: 'horizontal', size: 'md' }),
  item('Icon', '图标', '数据展示', '♥', 'badge', 56, 56, '', { icon: 'heart', size: 'xl', color: 'pink.600' }),
  item('Marquee', '跑马灯', '数据展示', '⇠', 'section', 480, 120, '', { side: 'left', reverse: false, speed: 40, pauseOnInteraction: true, edge: true }),
  item('QRCode', '二维码', '数据展示', '▦', 'image', 190, 190, '', { value: 'https://chakra-ui.com', size: 160, color: '#111827', overlay: false }),
  item('Tag', '标签', '数据展示', '◆', 'badge', 130, 40, '设计系统', { variant: 'surface', colorPalette: 'gray', size: 'md', closable: false }),

  item('Alert', '提示', '反馈', '!', 'card', 420, 90, '组件库已经成功接入。', { status: 'success', variant: 'subtle', title: '保存成功' }),
  item('Progress', '进度条', '反馈', '━', 'card', 340, 64, '', { value: 68, colorPalette: 'blue', size: 'md' }),
  item('Spinner', '加载动画', '反馈', '◌', 'card', 90, 80, '', { size: 'xl', colorPalette: 'blue' }),
  item('Skeleton', '骨架屏', '反馈', '▥', 'card', 360, 130, '', { kind: 'text', lines: 3 }),
  item('EmptyState', '空状态', '反馈', '∅', 'card', 340, 190, '暂无项目', { title: '没有找到内容', description: '创建一个项目后，它会显示在这里。' }),
  item('ProgressCircle', '环形进度', '反馈', '◔', 'card', 120, 120, '', { value: 75, size: 'xl', colorPalette: 'blue', showValue: true }),
  item('Status', '状态', '反馈', '●', 'badge', 150, 40, '', { colorPalette: 'green', label: '运行正常', size: 'md' }),
  item('Toast', '通知', '反馈', '▢', 'button', 160, 44, '显示通知', { type: 'success', title: '保存成功', description: '设计已经安全保存。', closable: true }),

  item('ActionBar', '操作栏', '浮层', '⌘', 'button', 180, 44, '显示操作栏', { selectedCount: 2, placement: 'bottom', actions: ['复制', '移动', '删除'] }),
  item('FloatingPanel', '浮动面板', '浮层', '▣', 'button', 160, 44, '打开浮动面板', { title: '浮动工具', size: 'md', defaultOpen: false }),
  item('HoverCard', '悬浮卡片', '浮层', '▢', 'link', 180, 44, '@chakra_ui', { title: 'Chakra UI', description: '现代 Web 应用的可组合组件工具箱。', openDelay: 250, closeDelay: 150 }),
  item('OverlayManager', '浮层管理器', '浮层', '▣', 'button', 190, 48, '发布当前设计', { kind: 'confirm', title: '发布当前设计？', description: '通过 Overlay Manager 创建并管理浮层。' }),
  item('ToggleTip', '点击提示', '浮层', '?', 'button', 150, 44, '查看提示', { content: '点击触发、再次点击关闭。', showArrow: false, size: 'sm' }),
  item('Dialog', '对话框', '浮层', '▣', 'button', 160, 44, '打开对话框', { title: '确认操作', placement: 'center', size: 'md' }),
  item('Drawer', '抽屉', '浮层', '▥', 'button', 150, 44, '打开抽屉', { title: '详情面板', placement: 'end', size: 'md' }),
  item('Popover', '气泡卡片', '浮层', '▢', 'button', 150, 44, '查看详情', { title: '产品信息', placement: 'bottom' }),
  item('Tooltip', '文字提示', '浮层', '?', 'button', 150, 44, '悬停查看', { content: '这是 Chakra UI 提示内容', placement: 'top' }),
  item('Menu', '菜单', '浮层', '☰', 'button', 150, 44, '更多操作', { items: [{ key: 'edit', label: '编辑' }, { key: 'duplicate', label: '复制' }, { key: 'delete', label: '删除' }] }),

  item('LocaleProvider', '语言环境', '国际化', '文', 'section', 420, 180, '', { locale: 'zh-CN', direction: 'ltr', title: '欢迎使用 Chakra UI' }),
  item('FormatNumber', '数字格式化', '国际化', '#', 'text', 260, 64, '', { value: 1450.45, locale: 'zh-CN', style: 'decimal', maximumFractionDigits: 2 }),
  item('FormatByte', '字节格式化', '国际化', 'KB', 'text', 280, 64, '', { value: 1450.45, locale: 'zh-CN', unitSystem: 'decimal', unitDisplay: 'short' }),

  item('Checkmark', '勾选标记', '工具', '✓', 'checkbox', 56, 56, '', { checked: true, indeterminate: false, disabled: false, size: 'md', colorPalette: 'blue' }),
  item('ClientOnly', '仅客户端渲染', '工具', 'C', 'card', 320, 90, '', { fallback: '正在连接客户端…', kind: 'content' }),
  item('For', '循环渲染', '工具', '↻', 'section', 380, 150, '', { count: 4, kind: 'cards' }),
  item('Presence', '显隐过渡', '工具', '◐', 'card', 300, 130, '切换显示', { present: true, lazyMount: false, unmountOnExit: false, animation: 'fade' }),
  item('Portal', '传送门', '工具', '↗', 'button', 160, 44, '显示传送内容', { kind: 'badge', placement: 'top-end' }),
  item('Radiomark', '单选标记', '工具', '◉', 'checkbox', 56, 56, '', { checked: true, disabled: false, size: 'md', colorPalette: 'blue' }),
  item('Show', '条件渲染', '工具', '?', 'card', 320, 140, '', { threshold: 3, initialCount: 4, label: '条件内容已显示' }),
  item('SkipNav', '跳过导航', '工具', '⇥', 'section', 420, 220, '', { label: '跳到主要内容', navLabel: '页面导航', contentLabel: '主要内容' }),
  item('VisuallyHidden', '视觉隐藏', '工具', '◌', 'button', 180, 48, '', { hiddenText: '3 条未读通知', visibleText: '3', icon: 'bell' }),
  item('Theme', '局部主题', '工具', '◒', 'section', 360, 160, '', { appearance: 'dark', colorPalette: 'teal' })
];

export const CHAKRA_COMPONENT_VARIANTS: Record<string, ChakraComponentVariant[]> = Object.fromEntries(
  Object.entries(CHAKRA_OFFICIAL_COMPONENT_VARIANTS).map(([componentId, variants]) => [componentId, variants.map((variant) => ({ ...variant }))])
);

export const CHAKRA_LIBRARY: UiLibraryCatalog<ChakraCategory> = {
  id: 'chakra', displayName: 'Chakra UI', shortName: 'Chakra', version: CHAKRA_VERSION, brandMark: 'C',
  categories: CHAKRA_CATEGORIES, components: CHAKRA_COMPONENTS, variants: CHAKRA_COMPONENT_VARIANTS
};

export function variantsForChakraComponent(componentId: string): ChakraComponentVariant[] {
  return variantsForUiComponent(CHAKRA_LIBRARY, componentId);
}

export function createChakraComponent(definitionId: string, x: number, y: number): WebDesignComponent {
  return createUiLibraryComponent(CHAKRA_LIBRARY, definitionId, x, y);
}

export function applyChakraComponentVariant(component: WebDesignComponent, variantId: string): WebDesignComponent {
  return applyUiComponentVariant(CHAKRA_LIBRARY, component, variantId);
}
