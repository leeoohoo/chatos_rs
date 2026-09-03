import type { WebDesignComponent } from './schema.js';
import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';

export type ChakraCategory = '布局' | '排版' | '按钮' | '数据录入' | '导航' | '数据展示' | '反馈' | '浮层';
export type ChakraComponentDefinition = UiComponentDefinition<ChakraCategory>;
export type ChakraComponentVariant = UiComponentVariant;

export const CHAKRA_VERSION = '3.37.0';
export const CHAKRA_CATEGORIES: ChakraCategory[] = ['布局', '排版', '按钮', '数据录入', '导航', '数据展示', '反馈', '浮层'];
const item = defineUiComponent<ChakraCategory>;

export const CHAKRA_COMPONENT_VARIANTS: Record<string, ChakraComponentVariant[]> = {
  Button: [
    { id: 'solid', label: '实心按钮', props: { variant: 'solid', colorPalette: 'blue', size: 'md' }, content: '主要操作' },
    { id: 'subtle', label: '柔和按钮', props: { variant: 'subtle', colorPalette: 'blue', size: 'md' }, content: '柔和操作' },
    { id: 'outline', label: '描边按钮', props: { variant: 'outline', colorPalette: 'gray', size: 'md' }, content: '描边按钮' },
    { id: 'ghost', label: '幽灵按钮', props: { variant: 'ghost', colorPalette: 'gray', size: 'md' }, content: '幽灵按钮' },
    { id: 'danger', label: '危险按钮', props: { variant: 'solid', colorPalette: 'red', size: 'md' }, content: '删除' }
  ],
  Input: [
    { id: 'outline', label: '描边输入框', props: { variant: 'outline', size: 'md' } },
    { id: 'subtle', label: '柔和输入框', props: { variant: 'subtle', size: 'md' } },
    { id: 'flushed', label: '下划线输入框', props: { variant: 'flushed', size: 'md' } }
  ],
  NativeSelect: [
    { id: 'outline', label: '描边选择框', props: { variant: 'outline', size: 'md' } },
    { id: 'subtle', label: '柔和选择框', props: { variant: 'subtle', size: 'md' } }
  ],
  Checkbox: [
    { id: 'blue', label: '蓝色多选框', props: { colorPalette: 'blue', defaultChecked: true } },
    { id: 'green', label: '绿色多选框', props: { colorPalette: 'green', defaultChecked: true } },
    { id: 'outline', label: '未选中', props: { colorPalette: 'gray', defaultChecked: false } }
  ],
  Switch: [
    { id: 'blue', label: '蓝色开关', props: { colorPalette: 'blue', defaultChecked: true } },
    { id: 'green', label: '绿色开关', props: { colorPalette: 'green', defaultChecked: true } },
    { id: 'off', label: '关闭状态', props: { colorPalette: 'gray', defaultChecked: false } }
  ],
  Badge: ['gray', 'blue', 'green', 'orange', 'red', 'purple'].map((colorPalette) => ({ id: colorPalette, label: `${colorPalette} 徽标`, props: { colorPalette, variant: 'subtle' } })),
  Alert: ['info', 'success', 'warning', 'error'].map((status) => ({ id: status, label: `${status} 提示`, props: { status, variant: 'subtle' } })),
  Card: [
    { id: 'elevated', label: '悬浮卡片', props: { variant: 'elevated' } },
    { id: 'outline', label: '描边卡片', props: { variant: 'outline' } },
    { id: 'subtle', label: '柔和卡片', props: { variant: 'subtle' } }
  ],
  Tabs: [
    { id: 'line', label: '线形标签页', props: { variant: 'line', defaultValue: 'overview' } },
    { id: 'subtle', label: '柔和标签页', props: { variant: 'subtle', defaultValue: 'overview' } },
    { id: 'enclosed', label: '卡片标签页', props: { variant: 'enclosed', defaultValue: 'overview' } }
  ],
  Accordion: [
    { id: 'outline', label: '描边手风琴', props: { variant: 'outline', multiple: false } },
    { id: 'subtle', label: '柔和手风琴', props: { variant: 'subtle', multiple: false } },
    { id: 'plain', label: '简洁手风琴', props: { variant: 'plain', multiple: true } }
  ],
  Dialog: [
    { id: 'center', label: '居中对话框', props: { placement: 'center', size: 'md', title: '确认操作' } },
    { id: 'top', label: '顶部对话框', props: { placement: 'top', size: 'md', title: '编辑信息' } },
    { id: 'large', label: '大型对话框', props: { placement: 'center', size: 'lg', title: '产品详情' }, width: 180 }
  ],
  Drawer: [
    { id: 'right', label: '右侧抽屉', props: { placement: 'end', size: 'md', title: '详情面板' } },
    { id: 'left', label: '左侧抽屉', props: { placement: 'start', size: 'md', title: '导航面板' } },
    { id: 'bottom', label: '底部抽屉', props: { placement: 'bottom', size: 'md', title: '快捷操作' } }
  ],
  Progress: [
    { id: 'blue', label: '蓝色进度', props: { value: 68, colorPalette: 'blue', size: 'md' } },
    { id: 'green', label: '绿色进度', props: { value: 82, colorPalette: 'green', size: 'md' } },
    { id: 'striped', label: '条纹进度', props: { value: 54, colorPalette: 'purple', striped: true, animated: true } }
  ],
  Skeleton: [
    { id: 'text', label: '文本骨架', props: { kind: 'text', lines: 3 } },
    { id: 'card', label: '卡片骨架', props: { kind: 'card', lines: 2 }, height: 150 },
    { id: 'avatar', label: '头像骨架', props: { kind: 'avatar', lines: 2 }, height: 90 }
  ]
};

const navItems = [{ key: 'overview', label: '概览' }, { key: 'features', label: '功能' }, { key: 'settings', label: '设置' }];
const accordionItems = [{ key: 'design', label: '设计系统' }, { key: 'collaboration', label: 'AI 协作' }, { key: 'delivery', label: '交付能力' }];

export const CHAKRA_COMPONENTS: ChakraComponentDefinition[] = [
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
  item('Blockquote', '引用', '排版', '❝', 'card', 420, 110, '优秀的设计系统让产品团队更专注于用户价值。', { cite: 'Web Design Studio' }),
  item('Kbd', '键盘按键', '排版', '⌘', 'badge', 120, 40, '⌘ K', {}),
  item('Link', '链接', '排版', '↗', 'link', 180, 42, '查看完整文档', { colorPalette: 'blue', variant: 'underline' }),
  item('List', '列表', '排版', '☷', 'list', 340, 150, '', { items: ['组合式组件', '主题令牌', '无障碍交互'] }),

  item('Button', '按钮', '按钮', '▣', 'button', 150, 44, '主要操作', { variant: 'solid', colorPalette: 'blue', size: 'md' }),
  item('IconButton', '图标按钮', '按钮', '✦', 'button', 48, 44, '＋', { variant: 'solid', colorPalette: 'blue', size: 'md' }),

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

  item('Avatar', '头像', '数据展示', '●', 'avatar', 64, 64, 'AI', { size: 'lg', name: 'AI Designer' }),
  item('Badge', '徽标', '数据展示', '◆', 'badge', 100, 36, '已发布', { colorPalette: 'green', variant: 'subtle' }),
  item('Card', '卡片', '数据展示', '▤', 'card', 360, 190, '清晰组织标题、说明和操作。', { variant: 'elevated', title: '产品卡片' }),
  item('Table', '表格', '数据展示', '▦', 'table', 520, 230, '', { striped: true, columns: ['项目', '状态', '负责人'], rows: [['设计系统', '进行中', '小林'], ['组件接入', '已完成', 'AI'], ['体验验收', '待处理', '产品']] }),
  item('Stat', '统计值', '数据展示', '#', 'card', 230, 110, '', { label: '本月活跃用户', value: '28,642', change: '+12.5%' }),
  item('Timeline', '时间轴', '数据展示', '│', 'list', 320, 190, '', { items: [{ key: '1', label: '完成需求分析', description: '10:20' }, { key: '2', label: '建立设计系统', description: '11:45' }, { key: '3', label: '开始组件接入', description: '14:10' }] }),

  item('Alert', '提示', '反馈', '!', 'card', 420, 90, '组件库已经成功接入。', { status: 'success', variant: 'subtle', title: '保存成功' }),
  item('Progress', '进度条', '反馈', '━', 'card', 340, 64, '', { value: 68, colorPalette: 'blue', size: 'md' }),
  item('Spinner', '加载动画', '反馈', '◌', 'card', 90, 80, '', { size: 'xl', colorPalette: 'blue' }),
  item('Skeleton', '骨架屏', '反馈', '▥', 'card', 360, 130, '', { kind: 'text', lines: 3 }),
  item('EmptyState', '空状态', '反馈', '∅', 'card', 340, 190, '暂无项目', { title: '没有找到内容', description: '创建一个项目后，它会显示在这里。' }),

  item('Dialog', '对话框', '浮层', '▣', 'button', 160, 44, '打开对话框', { title: '确认操作', placement: 'center', size: 'md' }),
  item('Drawer', '抽屉', '浮层', '▥', 'button', 150, 44, '打开抽屉', { title: '详情面板', placement: 'end', size: 'md' }),
  item('Popover', '气泡卡片', '浮层', '▢', 'button', 150, 44, '查看详情', { title: '产品信息', placement: 'bottom' }),
  item('Tooltip', '文字提示', '浮层', '?', 'button', 150, 44, '悬停查看', { content: '这是 Chakra UI 提示内容', placement: 'top' }),
  item('Menu', '菜单', '浮层', '☰', 'button', 150, 44, '更多操作', { items: [{ key: 'edit', label: '编辑' }, { key: 'duplicate', label: '复制' }, { key: 'delete', label: '删除' }] })
];

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
