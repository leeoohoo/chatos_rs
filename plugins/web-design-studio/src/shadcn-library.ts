import type { WebDesignComponent } from './schema.js';
import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';

export type ShadcnCategory = '布局' | '排版' | '按钮' | '数据录入' | '导航' | '数据展示' | '反馈' | '浮层';
export type ShadcnComponentDefinition = UiComponentDefinition<ShadcnCategory>;
export type ShadcnComponentVariant = UiComponentVariant;

export const SHADCN_VERSION = 'registry-2026.09';
export const SHADCN_CATEGORIES: ShadcnCategory[] = ['布局', '排版', '按钮', '数据录入', '导航', '数据展示', '反馈', '浮层'];
const item = defineUiComponent<ShadcnCategory>;

const genericExamples = (): ShadcnComponentVariant[] => [
  { id: 'default', label: '默认示例', props: { density: 'default', tone: 'neutral' } },
  { id: 'compact', label: '紧凑示例', props: { density: 'compact', tone: 'neutral' } },
  { id: 'accent', label: '强调示例', props: { density: 'comfortable', tone: 'accent' } }
];

const GENERIC_VARIANT_COMPONENTS = [
  'AspectRatio', 'ButtonGroup', 'Resizable', 'ScrollArea', 'Separator', 'Sidebar', 'Typography', 'Label', 'Kbd',
  'Toggle', 'ToggleGroup', 'InputGroup', 'InputOTP', 'Textarea', 'NativeSelect', 'Select', 'Combobox', 'Checkbox',
  'Switch', 'RadioGroup', 'Slider', 'Calendar', 'Field', 'Breadcrumb', 'Collapsible', 'Menubar', 'NavigationMenu',
  'Pagination', 'Avatar', 'Chart', 'DataTable', 'Empty', 'Table', 'Spinner', 'Toast', 'AlertDialog', 'DropdownMenu',
  'ContextMenu', 'HoverCard', 'Popover', 'Tooltip', 'Attachment', 'Bubble', 'Carousel', 'Command', 'DatePicker',
  'Direction', 'Item', 'Marker', 'Message', 'MessageScroller', 'Questionnaire'
];

export const SHADCN_COMPONENT_VARIANTS: Record<string, ShadcnComponentVariant[]> = {
  ...Object.fromEntries(GENERIC_VARIANT_COMPONENTS.map((component) => [component, genericExamples()])),
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
  ],
  Attachment: [
    { id: 'uploading', label: '上传中附件', props: { status: 'uploading', progress: 68, fileName: 'design-system.fig', fileSize: '18.4 MB' } },
    { id: 'complete', label: '已完成附件', props: { status: 'complete', progress: 100, fileName: 'homepage.png', fileSize: '2.8 MB' } },
    { id: 'error', label: '失败附件', props: { status: 'error', progress: 36, fileName: 'prototype.mov', fileSize: '84 MB' } }
  ],
  Bubble: [
    { id: 'assistant', label: 'AI 回复气泡', props: { role: 'assistant', avatar: 'AI', timestamp: '刚刚' }, content: '我已经根据你的要求调整了页面层级。' },
    { id: 'user', label: '用户消息气泡', props: { role: 'user', avatar: '你', timestamp: '10:24' }, content: '把首屏做得更像苹果官网。' },
    { id: 'thinking', label: '思考状态气泡', props: { role: 'assistant', avatar: 'AI', timestamp: '分析中', thinking: true }, content: '正在分析布局、颜色和组件关系…' }
  ],
  Carousel: [
    { id: 'product', label: '产品轮播', props: { slideCount: 3, loop: true, labels: ['网站设计', 'AI 协作', '响应式布局'] } },
    { id: 'testimonial', label: '评价轮播', props: { slideCount: 3, loop: true, labels: ['设计师评价', '产品团队评价', '开发团队评价'] } },
    { id: 'gallery', label: '图片画廊', props: { slideCount: 4, loop: false, labels: ['封面', '细节', '移动端', '交付'] } }
  ],
  Command: [
    { id: 'launcher', label: '命令启动器', props: { placeholder: '输入命令或搜索…', groups: [{ label: '建议', items: ['新建设计', '打开组件库', '切换主题'] }] } },
    { id: 'search', label: '全局搜索', props: { placeholder: '搜索页面、组件和资源…', groups: [{ label: '结果', items: ['首页 / Hero', '组件 / Button', '资源 / Logo'] }] } },
    { id: 'actions', label: '快捷操作', props: { placeholder: '执行操作…', groups: [{ label: '页面', items: ['复制页面', '导出预览', '邀请协作者'] }] } }
  ],
  DatePicker: [
    { id: 'single', label: '单日期选择', props: { mode: 'single', placeholder: '选择日期', selectedDay: 3 } },
    { id: 'range', label: '日期范围', props: { mode: 'range', placeholder: '选择日期范围', selectedDay: 3, endDay: 9 }, width: 320 },
    { id: 'birthday', label: '生日选择', props: { mode: 'single', placeholder: '选择生日', selectedDay: 18 } }
  ],
  Direction: [
    { id: 'ltr', label: '从左到右', props: { direction: 'ltr', locale: '中文' }, content: '从左到右排列的界面内容' },
    { id: 'rtl', label: '从右到左', props: { direction: 'rtl', locale: 'العربية' }, content: 'واجهة مرتبة من اليمين إلى اليسار' },
    { id: 'mixed', label: '混合语言', props: { direction: 'auto', locale: 'Auto' }, content: 'Web Design · 网站设计 · تصميم الويب' }
  ],
  Item: [
    { id: 'basic', label: '基础条目', props: { icon: '◈', title: '设计系统', description: '统一颜色、排版和组件规范。', action: '' } },
    { id: 'action', label: '带操作条目', props: { icon: '↗', title: '发布预览', description: '生成可分享的网站预览链接。', action: '发布' } },
    { id: 'status', label: '状态条目', props: { icon: '✓', title: '组件同步', description: '全部组件已更新到最新版本。', action: '已完成' } }
  ],
  Marker: [
    { id: 'highlight', label: '高亮标记', props: { kind: 'highlight', color: 'yellow' }, content: '重要设计决策' },
    { id: 'underline', label: '下划线标记', props: { kind: 'underline', color: 'blue' }, content: '需要重点关注的内容' },
    { id: 'badge', label: '徽章标记', props: { kind: 'badge', color: 'green' }, content: '最新' }
  ],
  Message: [
    { id: 'assistant', label: 'AI 消息', props: { role: 'assistant', sender: 'AI 设计师', avatar: 'AI', time: '刚刚' }, content: '首页结构已经完成，可以继续调整视觉细节。' },
    { id: 'user', label: '用户消息', props: { role: 'user', sender: '你', avatar: '你', time: '10:24' }, content: '把按钮和标题的层级再拉开一点。' },
    { id: 'system', label: '系统消息', props: { role: 'system', sender: '系统', avatar: '•', time: '已保存' }, content: '设计已同步到本地服务。' }
  ],
  MessageScroller: [
    { id: 'conversation', label: '对话记录', props: { kind: 'conversation', messageCount: 6 } },
    { id: 'activity', label: '活动记录', props: { kind: 'activity', messageCount: 8 } },
    { id: 'compact', label: '紧凑消息流', props: { kind: 'compact', messageCount: 10, density: 'compact' } }
  ],
  Questionnaire: [
    { id: 'single', label: '单选问卷', props: { type: 'single', question: '你最常设计哪类网站？', options: ['产品官网', '管理后台', '内容社区'] } },
    { id: 'multiple', label: '多选问卷', props: { type: 'multiple', question: '你希望 AI 帮你完成哪些工作？', options: ['页面结构', '视觉设计', '响应式适配'] } },
    { id: 'rating', label: '评分问卷', props: { type: 'rating', question: '这次设计体验如何？', options: ['1', '2', '3', '4', '5'] } }
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
  item('Direction', '文字方向', '布局', '↔', 'section', 360, 110, '从左到右排列的界面内容', { direction: 'ltr', locale: '中文' }),

  item('Typography', '排版', '排版', 'T', 'text', 380, 90, '设计不仅是外观，更是产品如何工作。', { as: 'p', scale: 'lead' }),
  item('Label', '标签', '排版', 'L', 'text', 180, 36, '电子邮箱', { htmlFor: 'email' }),
  item('Kbd', '键盘按键', '排版', '⌘', 'badge', 120, 38, '⌘ K', {}),
  item('Marker', '文本标记', '排版', '▰', 'text', 240, 44, '重要设计决策', { kind: 'highlight', color: 'yellow' }),

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
  item('DatePicker', '日期选择器', '数据录入', '▣', 'input', 280, 42, '选择日期', { mode: 'single', selectedDay: 3 }),
  item('Field', '字段', '数据录入', '▤', 'section', 360, 130, '', { label: '用户名', description: '这是你的公开显示名称。', error: '' }),
  item('Questionnaire', '问卷', '数据录入', '?', 'section', 420, 220, '', { type: 'single', question: '你最常设计哪类网站？', options: ['产品官网', '管理后台', '内容社区'] }),

  item('Accordion', '手风琴', '导航', '⌄', 'list', 420, 220, '', { type: 'single', collapsible: true, defaultValue: 'design', items: accordionItems }),
  item('Breadcrumb', '面包屑', '导航', '›', 'list', 360, 44, '', { items: [{ key: 'home', label: '首页' }, { key: 'components', label: '组件' }, { key: 'button', label: '按钮' }] }),
  item('Collapsible', '折叠区域', '导航', '⌄', 'section', 380, 150, '展开更多内容', { defaultOpen: true }),
  item('Command', '命令面板', '导航', '⌘', 'section', 420, 260, '', { placeholder: '输入命令或搜索…', groups: [{ label: '建议', items: ['新建设计', '打开组件库', '切换主题'] }] }),
  item('Menubar', '菜单栏', '导航', '☰', 'list', 420, 44, '', { menus: [{ key: 'file', label: '文件' }, { key: 'edit', label: '编辑' }, { key: 'view', label: '视图' }] }),
  item('NavigationMenu', '导航菜单', '导航', '☰', 'list', 460, 50, '', { items: [{ key: 'products', label: '产品' }, { key: 'solutions', label: '解决方案' }, { key: 'pricing', label: '定价' }] }),
  item('Pagination', '分页', '导航', '•••', 'list', 340, 44, '', { defaultPage: 2, totalPages: 8 }),
  item('Tabs', '标签页', '导航', '▤', 'card', 440, 190, '', { orientation: 'horizontal', defaultValue: 'account', items: tabItems }),

  item('Avatar', '头像', '数据展示', '●', 'avatar', 64, 64, 'AI', { fallback: 'AI', src: '' }),
  item('Attachment', '附件', '数据展示', '⌁', 'card', 420, 92, '', { status: 'uploading', progress: 68, fileName: 'design-system.fig', fileSize: '18.4 MB' }),
  item('Badge', '徽标', '数据展示', '◆', 'badge', 100, 34, 'Beta', { variant: 'secondary' }),
  item('Bubble', '消息气泡', '数据展示', '◒', 'card', 380, 110, '我已经根据你的要求调整了页面层级。', { role: 'assistant', avatar: 'AI', timestamp: '刚刚' }),
  item('Card', '卡片', '数据展示', '▤', 'card', 360, 190, '用于组织信息、操作和相关内容。', { title: '创建项目', description: '快速开始一个新的网站设计。', density: 'default' }),
  item('Carousel', '轮播图', '数据展示', '▣', 'card', 460, 260, '', { slideCount: 3, loop: true, labels: ['网站设计', 'AI 协作', '响应式布局'] }),
  item('Chart', '图表', '数据展示', '⌁', 'card', 460, 260, '', { type: 'bar', labels: ['一月', '二月', '三月', '四月', '五月'], values: [42, 68, 51, 86, 73] }),
  item('DataTable', '数据表格', '数据展示', '▦', 'table', 560, 250, '', { columns: ['任务', '状态', '优先级'], rows: [['设计首页', '进行中', '高'], ['接入组件库', '已完成', '高'], ['移动端验收', '待处理', '中']] }),
  item('Empty', '空状态', '数据展示', '∅', 'card', 340, 190, '暂无内容', { title: '没有找到项目', description: '创建第一个项目以开始设计。', actionLabel: '新建项目' }),
  item('Item', '内容条目', '数据展示', '☷', 'card', 420, 88, '', { icon: '◈', title: '设计系统', description: '统一颜色、排版和组件规范。', action: '' }),
  item('Message', '消息', '数据展示', '◉', 'card', 440, 110, '首页结构已经完成，可以继续调整视觉细节。', { role: 'assistant', sender: 'AI 设计师', avatar: 'AI', time: '刚刚' }),
  item('MessageScroller', '消息滚动区', '数据展示', '↕', 'section', 440, 300, '', { kind: 'conversation', messageCount: 6 }),
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
