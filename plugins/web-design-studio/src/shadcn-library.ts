import type { WebDesignComponent } from './schema.js';
import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';

export type ShadcnCategory = '布局' | '排版' | '按钮' | '数据录入' | '导航' | '数据展示' | '反馈' | '浮层';
export type ShadcnComponentDefinition = UiComponentDefinition<ShadcnCategory>;
export type ShadcnComponentVariant = UiComponentVariant;

export const SHADCN_VERSION = 'registry-2026.09';
export const SHADCN_CATEGORIES: ShadcnCategory[] = ['布局', '排版', '按钮', '数据录入', '导航', '数据展示', '反馈', '浮层'];
const item = defineUiComponent<ShadcnCategory>;

export const SHADCN_COMPONENT_VARIANTS: Record<string, ShadcnComponentVariant[]> = {
  Button: [
    { id: 'default', label: '默认按钮', props: { variant: 'default', size: 'default' }, content: '主要操作' },
    { id: 'secondary', label: '次要按钮', props: { variant: 'secondary', size: 'default' }, content: '次要操作' },
    { id: 'outline', label: '描边按钮', props: { variant: 'outline', size: 'default' }, content: '描边按钮' },
    { id: 'ghost', label: '幽灵按钮', props: { variant: 'ghost', size: 'default' }, content: '幽灵按钮' },
    { id: 'destructive', label: '危险按钮', props: { variant: 'destructive', size: 'default' }, content: '删除' },
    { id: 'link', label: '链接按钮', props: { variant: 'link', size: 'default' }, content: '了解更多' }
  ],
  Badge: [
    { id: 'default', label: '默认徽标', props: { variant: 'default' } },
    { id: 'secondary', label: '次要徽标', props: { variant: 'secondary' } },
    { id: 'outline', label: '描边徽标', props: { variant: 'outline' } },
    { id: 'destructive', label: '危险徽标', props: { variant: 'destructive' } }
  ],
  Alert: [
    { id: 'default', label: '默认提示', props: { variant: 'default', title: '提示' } },
    { id: 'destructive', label: '危险提示', props: { variant: 'destructive', title: '操作失败' } }
  ],
  Input: [
    { id: 'default', label: '默认输入框', props: { invalid: false, disabled: false } },
    { id: 'invalid', label: '错误输入框', props: { invalid: true, disabled: false } },
    { id: 'disabled', label: '禁用输入框', props: { invalid: false, disabled: true } }
  ],
  Card: [
    { id: 'default', label: '默认卡片', props: { density: 'default' } },
    { id: 'compact', label: '紧凑卡片', props: { density: 'compact' }, height: 160 },
    { id: 'featured', label: '重点卡片', props: { density: 'default', featured: true } }
  ],
  Tabs: [
    { id: 'default', label: '默认标签页', props: { orientation: 'horizontal', defaultValue: 'account' } },
    { id: 'vertical', label: '垂直标签页', props: { orientation: 'vertical', defaultValue: 'account' }, width: 480, height: 220 }
  ],
  Accordion: [
    { id: 'single', label: '单项展开', props: { type: 'single', collapsible: true, defaultValue: 'design' } },
    { id: 'multiple', label: '多项展开', props: { type: 'multiple', collapsible: true, defaultValue: ['design', 'ai'] } }
  ],
  Dialog: [
    { id: 'default', label: '默认对话框', props: { title: '编辑资料', size: 'md' } },
    { id: 'small', label: '紧凑对话框', props: { title: '确认操作', size: 'sm' } },
    { id: 'large', label: '大型对话框', props: { title: '产品详情', size: 'lg' } }
  ],
  Drawer: [
    { id: 'bottom', label: '底部抽屉', props: { side: 'bottom', title: '快捷操作' } },
    { id: 'right', label: '右侧抽屉', props: { side: 'right', title: '详情面板' } },
    { id: 'left', label: '左侧抽屉', props: { side: 'left', title: '导航面板' } }
  ],
  Sheet: [
    { id: 'right', label: '右侧面板', props: { side: 'right', title: '编辑设置' } },
    { id: 'left', label: '左侧面板', props: { side: 'left', title: '网站导航' } },
    { id: 'top', label: '顶部面板', props: { side: 'top', title: '通知中心' } },
    { id: 'bottom', label: '底部面板', props: { side: 'bottom', title: '快捷操作' } }
  ],
  Progress: [
    { id: 'quarter', label: '25% 进度', props: { value: 25 } },
    { id: 'half', label: '55% 进度', props: { value: 55 } },
    { id: 'complete', label: '完成进度', props: { value: 100 } }
  ],
  Skeleton: [
    { id: 'text', label: '文本骨架', props: { kind: 'text', lines: 3 } },
    { id: 'card', label: '卡片骨架', props: { kind: 'card', lines: 2 }, height: 150 },
    { id: 'profile', label: '用户骨架', props: { kind: 'profile', lines: 2 }, height: 90 }
  ]
};

const tabItems = [{ key: 'account', label: '账号' }, { key: 'password', label: '密码' }, { key: 'billing', label: '账单' }];
const accordionItems = [{ key: 'design', label: '设计能力' }, { key: 'ai', label: 'AI 协作' }, { key: 'export', label: '交付方式' }];

export const SHADCN_COMPONENTS: ShadcnComponentDefinition[] = [
  item('AspectRatio', '宽高比容器', '布局', '▭', 'section', 400, 225, '', { ratio: 1.7778 }),
  item('ButtonGroup', '按钮组', '布局', '▣', 'section', 340, 52, '', { orientation: 'horizontal', attached: true }),
  item('Resizable', '可调分栏', '布局', '↔', 'section', 460, 220, '', { direction: 'horizontal', defaultSizes: [45, 55] }),
  item('ScrollArea', '滚动区域', '布局', '↕', 'section', 360, 220, '', { maxHeight: 220 }),
  item('Separator', '分隔线', '布局', '—', 'divider', 360, 28, '', { orientation: 'horizontal', decorative: true }),
  item('Sidebar', '侧边栏', '布局', '▥', 'section', 280, 420, '', { side: 'left', collapsible: 'icon', items: [{ key: 'home', label: '首页' }, { key: 'projects', label: '项目' }, { key: 'settings', label: '设置' }] }),

  item('Typography', '排版', '排版', 'T', 'text', 380, 90, '设计不仅是外观，更是产品如何工作。', { as: 'p', scale: 'lead' }),
  item('Label', '标签', '排版', 'L', 'text', 180, 36, '电子邮箱', { htmlFor: 'email' }),
  item('Kbd', '键盘按键', '排版', '⌘', 'badge', 120, 38, '⌘ K', {}),

  item('Button', '按钮', '按钮', '▣', 'button', 150, 44, '主要操作', { variant: 'default', size: 'default' }),
  item('Toggle', '切换按钮', '按钮', '◩', 'button', 120, 42, '加粗', { pressed: false, variant: 'outline' }),
  item('ToggleGroup', '切换按钮组', '按钮', '▥', 'button', 240, 44, '', { type: 'single', defaultValue: 'center', items: [{ key: 'left', label: '左' }, { key: 'center', label: '中' }, { key: 'right', label: '右' }] }),

  item('Input', '输入框', '数据录入', '⌨', 'input', 280, 42, '请输入内容', { invalid: false, disabled: false }),
  item('InputGroup', '组合输入框', '数据录入', '@', 'input', 320, 42, 'name@example.com', { prefix: '@', suffix: '.com' }),
  item('InputOTP', '验证码输入', '数据录入', '•••', 'input', 300, 48, '', { length: 6, defaultValue: '123' }),
  item('Textarea', '多行输入框', '数据录入', '▤', 'textarea', 320, 100, '请输入详细说明', { rows: 4 }),
  item('NativeSelect', '原生选择器', '数据录入', '⌄', 'select', 280, 42, '选择框架', { options: [{ value: 'react', label: 'React' }, { value: 'vue', label: 'Vue' }, { value: 'html', label: 'HTML' }] }),
  item('Select', '选择器', '数据录入', '⌄', 'select', 280, 42, '选择设计方案', { defaultValue: 'apple', options: [{ value: 'apple', label: 'Apple 风格' }, { value: 'editorial', label: '杂志风格' }, { value: 'dashboard', label: '数据看板' }] }),
  item('Combobox', '组合选择器', '数据录入', '⌕', 'select', 300, 42, '搜索技术栈', { options: [{ value: 'next', label: 'Next.js' }, { value: 'vite', label: 'Vite' }, { value: 'astro', label: 'Astro' }] }),
  item('Checkbox', '多选框', '数据录入', '☑', 'checkbox', 220, 42, '接受服务条款', { defaultChecked: true }),
  item('Switch', '开关', '数据录入', '◉', 'switch', 200, 42, '启用通知', { defaultChecked: true }),
  item('RadioGroup', '单选组', '数据录入', '◉', 'checkbox', 350, 74, '', { defaultValue: 'comfortable', options: [{ value: 'default', label: '默认' }, { value: 'comfortable', label: '舒适' }, { value: 'compact', label: '紧凑' }] }),
  item('Slider', '滑块', '数据录入', '━', 'input', 300, 48, '', { defaultValue: [58], min: 0, max: 100, step: 1 }),
  item('Calendar', '日历', '数据录入', '▦', 'card', 320, 330, '', { month: '2026-09', selectedDay: 3 }),
  item('Field', '字段', '数据录入', '▤', 'section', 360, 130, '', { label: '用户名', description: '这是你的公开显示名称。', error: '' }),

  item('Accordion', '手风琴', '导航', '⌄', 'list', 420, 220, '', { type: 'single', collapsible: true, defaultValue: 'design', items: accordionItems }),
  item('Breadcrumb', '面包屑', '导航', '›', 'list', 360, 44, '', { items: [{ key: 'home', label: '首页' }, { key: 'components', label: '组件' }, { key: 'button', label: '按钮' }] }),
  item('Collapsible', '折叠区域', '导航', '⌄', 'section', 380, 150, '展开更多内容', { defaultOpen: true }),
  item('Menubar', '菜单栏', '导航', '☰', 'list', 420, 44, '', { menus: [{ key: 'file', label: '文件' }, { key: 'edit', label: '编辑' }, { key: 'view', label: '视图' }] }),
  item('NavigationMenu', '导航菜单', '导航', '☰', 'list', 460, 50, '', { items: [{ key: 'products', label: '产品' }, { key: 'solutions', label: '解决方案' }, { key: 'pricing', label: '定价' }] }),
  item('Pagination', '分页', '导航', '•••', 'list', 340, 44, '', { defaultPage: 2, totalPages: 8 }),
  item('Tabs', '标签页', '导航', '▤', 'card', 440, 190, '', { orientation: 'horizontal', defaultValue: 'account', items: tabItems }),

  item('Avatar', '头像', '数据展示', '●', 'avatar', 64, 64, 'AI', { fallback: 'AI', src: '' }),
  item('Badge', '徽标', '数据展示', '◆', 'badge', 100, 34, 'Beta', { variant: 'secondary' }),
  item('Card', '卡片', '数据展示', '▤', 'card', 360, 190, '用于组织信息、操作和相关内容。', { title: '创建项目', description: '快速开始一个新的网站设计。', density: 'default' }),
  item('Chart', '图表', '数据展示', '⌁', 'card', 460, 260, '', { type: 'bar', labels: ['一月', '二月', '三月', '四月', '五月'], values: [42, 68, 51, 86, 73] }),
  item('DataTable', '数据表格', '数据展示', '▦', 'table', 560, 250, '', { columns: ['任务', '状态', '优先级'], rows: [['设计首页', '进行中', '高'], ['接入组件库', '已完成', '高'], ['移动端验收', '待处理', '中']] }),
  item('Empty', '空状态', '数据展示', '∅', 'card', 340, 190, '暂无内容', { title: '没有找到项目', description: '创建第一个项目以开始设计。', actionLabel: '新建项目' }),
  item('Table', '表格', '数据展示', '▦', 'table', 520, 230, '', { striped: false, columns: ['组件', '状态', '版本'], rows: [['Button', '稳定', '1.0'], ['Dialog', '稳定', '1.0'], ['Sidebar', '新增', '1.1']] }),

  item('Alert', '提示', '反馈', '!', 'card', 420, 88, '你可以继续编辑所有组件属性。', { variant: 'default', title: '组件已接入' }),
  item('Progress', '进度条', '反馈', '━', 'card', 340, 54, '', { value: 68 }),
  item('Skeleton', '骨架屏', '反馈', '▥', 'card', 360, 130, '', { kind: 'text', lines: 3 }),
  item('Spinner', '加载动画', '反馈', '◌', 'card', 80, 72, '', { size: 28 }),
  item('Toast', '消息提示', '反馈', '▢', 'button', 150, 44, '显示通知', { title: '保存成功', description: '你的设计已经保存。' }),

  item('AlertDialog', '确认对话框', '浮层', '!', 'button', 170, 44, '删除项目', { title: '确定删除吗？', description: '此操作无法撤销。' }),
  item('Dialog', '对话框', '浮层', '▣', 'button', 160, 44, '打开对话框', { title: '编辑资料', size: 'md' }),
  item('Drawer', '抽屉', '浮层', '▥', 'button', 150, 44, '打开抽屉', { title: '快捷操作', side: 'bottom' }),
  item('Sheet', '侧边面板', '浮层', '▥', 'button', 150, 44, '打开面板', { title: '编辑设置', side: 'right' }),
  item('DropdownMenu', '下拉菜单', '浮层', '⌄', 'button', 160, 44, '打开菜单', { items: [{ key: 'profile', label: '个人资料' }, { key: 'billing', label: '账单' }, { key: 'logout', label: '退出登录' }] }),
  item('ContextMenu', '右键菜单', '浮层', '☰', 'card', 260, 110, '在此区域点击右键', { items: [{ key: 'back', label: '返回' }, { key: 'reload', label: '重新加载' }, { key: 'save', label: '保存页面' }] }),
  item('HoverCard', '悬浮卡片', '浮层', '▢', 'button', 160, 44, '悬停查看用户', { title: '@shadcn', description: '创建 shadcn/ui 的设计师与开发者。' }),
  item('Popover', '气泡卡片', '浮层', '▢', 'button', 150, 44, '打开气泡', { title: '尺寸设置', placement: 'bottom' }),
  item('Tooltip', '文字提示', '浮层', '?', 'button', 150, 44, '悬停查看', { content: '添加到组件库', placement: 'top' })
];

export const SHADCN_LIBRARY: UiLibraryCatalog<ShadcnCategory> = {
  id: 'shadcn', displayName: 'shadcn/ui', shortName: 'shadcn', version: SHADCN_VERSION, brandMark: 'S',
  categories: SHADCN_CATEGORIES, components: SHADCN_COMPONENTS, variants: SHADCN_COMPONENT_VARIANTS
};

export function variantsForShadcnComponent(componentId: string): ShadcnComponentVariant[] {
  return variantsForUiComponent(SHADCN_LIBRARY, componentId);
}

export function createShadcnComponent(definitionId: string, x: number, y: number): WebDesignComponent {
  return createUiLibraryComponent(SHADCN_LIBRARY, definitionId, x, y);
}

export function applyShadcnComponentVariant(component: WebDesignComponent, variantId: string): WebDesignComponent {
  return applyUiComponentVariant(SHADCN_LIBRARY, component, variantId);
}
