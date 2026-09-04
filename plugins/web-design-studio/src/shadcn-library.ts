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
  AspectRatio: [
    { id: 'video', label: '16:9 视频画面', props: { kind: 'video', ratio: 16 / 9, label: '16:9', title: '产品演示视频' }, width: 400, height: 225 },
    { id: 'square', label: '1:1 方形媒体', props: { kind: 'square', ratio: 1, label: '1:1', title: '品牌视觉素材' }, width: 240, height: 240 },
    { id: 'portrait', label: '4:5 竖版海报', props: { kind: 'portrait', ratio: 4 / 5, label: '4:5', title: '移动端营销海报' }, width: 220, height: 275 }
  ],
  ButtonGroup: [
    { id: 'pagination', label: '分页按钮组', props: { kind: 'pagination', orientation: 'horizontal' }, width: 300, height: 48 },
    { id: 'split', label: '拆分操作按钮', props: { kind: 'split', orientation: 'horizontal' }, width: 260, height: 48 },
    { id: 'vertical', label: '垂直工具按钮组', props: { kind: 'tools', orientation: 'vertical' }, width: 150, height: 150 }
  ],
  Resizable: [
    { id: 'horizontal', label: '水平可调分栏', props: { direction: 'horizontal', defaultSizes: [42, 58], panelKind: 'layers' }, width: 460, height: 220 },
    { id: 'vertical', label: '垂直可调分栏', props: { direction: 'vertical', defaultSizes: [55, 45], panelKind: 'preview' }, width: 420, height: 260 },
    { id: 'code', label: '代码与预览分栏', props: { direction: 'horizontal', defaultSizes: [55, 45], panelKind: 'code' }, width: 520, height: 250 }
  ],
  ScrollArea: [
    { id: 'updates', label: '组件更新列表', props: { kind: 'updates', itemCount: 10 }, width: 360, height: 220 },
    { id: 'notifications', label: '通知滚动区域', props: { kind: 'notifications', itemCount: 8 }, width: 380, height: 240 },
    { id: 'horizontal', label: '横向卡片滚动', props: { kind: 'horizontal', itemCount: 6 }, width: 460, height: 150 }
  ],
  Separator: [
    { id: 'horizontal', label: '水平分隔线', props: { orientation: 'horizontal', label: '' }, width: 360, height: 28 },
    { id: 'label', label: '带文字分隔线', props: { orientation: 'horizontal', label: '或者继续使用' }, width: 360, height: 42 },
    { id: 'vertical', label: '垂直分隔线', props: { orientation: 'vertical', label: '' }, width: 40, height: 160 }
  ],
  Typography: [
    { id: 'lead', label: '引导正文', props: { scale: 'lead' }, content: '设计不仅是外观，更是产品如何工作。', width: 420, height: 86 },
    { id: 'heading', label: '产品大标题', props: { scale: 'heading' }, content: '让 AI 与人共同设计网站', width: 480, height: 100 },
    { id: 'quote', label: '引用排版', props: { scale: 'quote' }, content: '好的工具不会替代设计判断，而是让每次判断更容易落地。', width: 460, height: 120 }
  ],
  Label: [
    { id: 'default', label: '基础字段标签', props: { kind: 'default', required: false, disabled: false }, content: '电子邮箱' },
    { id: 'required', label: '必填字段标签', props: { kind: 'required', required: true, disabled: false }, content: '项目名称' },
    { id: 'helper', label: '带辅助说明标签', props: { kind: 'helper', required: false, disabled: false, helper: '仅团队成员可见' }, content: '内部备注', width: 220, height: 54 }
  ],
  Kbd: [
    { id: 'single', label: '单个按键', props: { keys: ['K'] }, content: 'K' },
    { id: 'combo', label: '组合快捷键', props: { keys: ['⌘', 'K'] }, content: '⌘ K', width: 130 },
    { id: 'sequence', label: '连续快捷键', props: { keys: ['G', 'D'] }, content: 'G D', width: 130 }
  ],
  Toggle: [
    { id: 'off', label: '未选切换按钮', props: { pressed: false, disabled: false, icon: 'B' }, content: '加粗' },
    { id: 'on', label: '已选切换按钮', props: { pressed: true, disabled: false, icon: 'I' }, content: '斜体' },
    { id: 'disabled', label: '禁用切换按钮', props: { pressed: true, disabled: true, icon: 'S' }, content: '删除线' }
  ],
  ToggleGroup: [
    { id: 'align', label: '单选对齐工具组', props: { type: 'single', orientation: 'horizontal', defaultValue: 'center', items: [{ key: 'left', label: '左' }, { key: 'center', label: '中' }, { key: 'right', label: '右' }] }, width: 240, height: 48 },
    { id: 'format', label: '多选格式工具组', props: { type: 'multiple', orientation: 'horizontal', defaultValue: ['bold', 'italic'], items: [{ key: 'bold', label: 'B' }, { key: 'italic', label: 'I' }, { key: 'underline', label: 'U' }] }, width: 220, height: 48 },
    { id: 'vertical', label: '垂直视图工具组', props: { type: 'single', orientation: 'vertical', defaultValue: 'design', items: [{ key: 'design', label: '设计' }, { key: 'preview', label: '预览' }, { key: 'code', label: '代码' }] }, width: 120, height: 150 }
  ],
  Sidebar: [
    { id: 'workspace', label: '工作区侧边栏', props: { kind: 'workspace', items: [{ key: 'home', label: '首页' }, { key: 'projects', label: '项目' }, { key: 'assets', label: '资源' }, { key: 'settings', label: '设置' }] }, width: 280, height: 420 },
    { id: 'rail', label: '图标收起侧边栏', props: { kind: 'rail', items: [{ key: 'home', label: '首页' }, { key: 'projects', label: '项目' }, { key: 'assets', label: '资源' }, { key: 'settings', label: '设置' }] }, width: 82, height: 360 },
    { id: 'settings', label: '设置分组侧边栏', props: { kind: 'settings', items: [{ key: 'profile', label: '个人资料', group: '账号' }, { key: 'billing', label: '账单', group: '账号' }, { key: 'members', label: '成员', group: '团队' }, { key: 'security', label: '安全', group: '团队' }] }, width: 280, height: 410 }
  ],
  Calendar: [
    { id: 'single', label: '单日期日历', props: { kind: 'single', month: '2026-09', selectedDay: 3 }, width: 320, height: 350 },
    { id: 'range', label: '日期范围日历', props: { kind: 'range', month: '2026-09', selectedDay: 8, endDay: 14 }, width: 340, height: 385 },
    { id: 'events', label: '事件日历', props: { kind: 'events', month: '2026-09', selectedDay: 18, eventCount: 4 }, width: 360, height: 410 }
  ],
  Breadcrumb: [
    { id: 'basic', label: '基础面包屑', props: { kind: 'basic', items: [{ key: 'home', label: '首页' }, { key: 'components', label: '组件' }, { key: 'button', label: '按钮' }] }, width: 360 },
    { id: 'icons', label: '带图标面包屑', props: { kind: 'icons', items: [{ key: 'home', label: '⌂ 工作区' }, { key: 'design', label: '▦ 网站设计' }, { key: 'hero', label: 'Hero' }] }, width: 400 },
    { id: 'collapsed', label: '折叠路径面包屑', props: { kind: 'collapsed', items: [{ key: 'home', label: '首页' }, { key: 'more', label: '…' }, { key: 'page', label: '页面' }, { key: 'settings', label: '响应式设置' }] }, width: 420 }
  ],
  Collapsible: [
    { id: 'details', label: '设计详情折叠区', props: { kind: 'details', defaultOpen: true }, content: '设计详情', height: 170 },
    { id: 'code', label: '代码片段折叠区', props: { kind: 'code', defaultOpen: true }, content: '查看生成代码', width: 420, height: 190 },
    { id: 'filters', label: '筛选条件折叠区', props: { kind: 'filters', defaultOpen: false }, content: '高级筛选', width: 380, height: 62 }
  ],
  Menubar: [
    { id: 'editor', label: '编辑器菜单栏', props: { kind: 'editor', menus: [{ key: 'file', label: '文件' }, { key: 'edit', label: '编辑' }, { key: 'view', label: '视图' }, { key: 'help', label: '帮助' }] }, width: 420 },
    { id: 'media', label: '媒体工具菜单栏', props: { kind: 'media', menus: [{ key: 'image', label: '图片' }, { key: 'video', label: '视频' }, { key: 'audio', label: '音频' }] }, width: 330 },
    { id: 'compact', label: '紧凑操作菜单栏', props: { kind: 'compact', menus: [{ key: 'undo', label: '↶' }, { key: 'redo', label: '↷' }, { key: 'zoom', label: '100%' }] }, width: 220 }
  ],
  NavigationMenu: [
    { id: 'product', label: '产品站导航', props: { kind: 'product', items: [{ key: 'products', label: '产品' }, { key: 'solutions', label: '解决方案' }, { key: 'pricing', label: '定价' }] }, width: 460 },
    { id: 'mega', label: '大型下拉导航', props: { kind: 'mega', items: [{ key: 'platform', label: '平台' }, { key: 'resources', label: '资源' }, { key: 'company', label: '公司' }] }, width: 500, height: 220 },
    { id: 'account', label: '账户导航', props: { kind: 'account', items: [{ key: 'docs', label: '文档' }, { key: 'community', label: '社区' }, { key: 'account', label: 'AI' }] }, width: 380 }
  ],
  Pagination: [
    { id: 'numbered', label: '数字分页', props: { kind: 'numbered', defaultPage: 2, totalPages: 8 }, width: 380 },
    { id: 'compact', label: '紧凑分页', props: { kind: 'compact', defaultPage: 3, totalPages: 12 }, width: 250 },
    { id: 'load-more', label: '加载更多分页', props: { kind: 'load-more', defaultPage: 2, totalPages: 6 }, width: 260, height: 54 }
  ],
  Avatar: [
    { id: 'initials', label: '文字头像', props: { kind: 'initials', fallback: 'AI', status: '' }, content: 'AI', width: 64, height: 64 },
    { id: 'image', label: '图片头像', props: { kind: 'image', fallback: '林', src: 'data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%22120%22 height=%22120%22%3E%3Crect width=%22120%22 height=%22120%22 rx=%2260%22 fill=%22%235856d6%22/%3E%3Ccircle cx=%2260%22 cy=%2246%22 r=%2222%22 fill=%22%23fff%22/%3E%3Cpath d=%22M20 112c5-28 20-42 40-42s35 14 40 42%22 fill=%22%23fff%22/%3E%3C/svg%3E', status: '' }, width: 72, height: 72 },
    { id: 'status', label: '在线状态头像', props: { kind: 'status', fallback: '陈', status: 'online' }, content: '陈', width: 72, height: 72 }
  ],
  Chart: [
    { id: 'bar', label: '柱状趋势图', props: { kind: 'bar', title: '访问趋势', change: '+18.2%', labels: ['一月', '二月', '三月', '四月', '五月'], values: [42, 68, 51, 86, 73] }, width: 460, height: 260 },
    { id: 'line', label: '折线趋势图', props: { kind: 'line', title: '转化率', change: '+6.4%', labels: ['周一', '周二', '周三', '周四', '周五'], values: [32, 48, 44, 72, 84] }, width: 460, height: 260 },
    { id: 'donut', label: '环形构成图', props: { kind: 'donut', title: '流量来源', change: '100%', labels: ['自然搜索', '直接访问', '社交媒体'], values: [54, 28, 18] }, width: 420, height: 260 }
  ],
  DataTable: [
    { id: 'projects', label: '项目数据表', props: { kind: 'projects', striped: false, columns: ['项目', '状态', '负责人'], rows: [['品牌官网', '设计中', '林设计师'], ['产品文档', '已发布', 'AI'], ['活动页面', '待审核', '陈产品']] }, width: 560, height: 250 },
    { id: 'members', label: '成员条纹表', props: { kind: 'members', striped: true, columns: ['成员', '角色', '最近在线'], rows: [['林设计师', '设计师', '刚刚'], ['陈产品', '所有者', '10 分钟前'], ['AI Designer', '协作者', '在线']] }, width: 560, height: 250 },
    { id: 'selectable', label: '可选择数据表', props: { kind: 'selectable', striped: false, selectable: true, columns: ['页面', '断点', '问题'], rows: [['首页', '桌面', '0'], ['定价页', '平板', '2'], ['登录页', '手机', '1']] }, width: 580, height: 270 }
  ],
  Table: [
    { id: 'basic', label: '基础表格', props: { kind: 'basic', striped: false, columns: ['组件', '状态', '版本'], rows: [['Button', '稳定', '1.0'], ['Dialog', '稳定', '1.0'], ['Sidebar', '新增', '1.1']] }, width: 520, height: 230 },
    { id: 'striped', label: '条纹表格', props: { kind: 'striped', striped: true, columns: ['页面', '访问', '转化率'], rows: [['首页', '42,860', '18.2%'], ['定价页', '18,420', '12.6%'], ['案例页', '9,860', '8.4%']] }, width: 540, height: 230 },
    { id: 'toolbar', label: '带工具栏表格', props: { kind: 'toolbar', striped: false, columns: ['资源', '类型', '大小'], rows: [['Hero.png', '图片', '2.8 MB'], ['Intro.mp4', '视频', '18 MB'], ['Logo.svg', '矢量', '42 KB']] }, width: 580, height: 290 }
  ],
  Empty: [
    { id: 'projects', label: '空项目状态', props: { kind: 'projects', title: '还没有网站项目', description: '创建第一个项目，让 AI 和你一起开始设计。', actionLabel: '新建项目' }, width: 360, height: 220 },
    { id: 'search', label: '无搜索结果状态', props: { kind: 'search', title: '没有匹配的组件', description: '尝试缩短关键词或切换组件库。', actionLabel: '清除搜索' }, width: 360, height: 210 },
    { id: 'offline', label: '离线错误状态', props: { kind: 'offline', title: '无法连接设计服务', description: '本地服务暂时不可用，请检查后重试。', actionLabel: '重新连接' }, width: 380, height: 230 }
  ],
  Spinner: [
    { id: 'spinner', label: '基础旋转加载', props: { kind: 'spinner', size: 28 }, width: 80, height: 72 },
    { id: 'label', label: '带文字加载', props: { kind: 'label', size: 24, label: '正在生成响应式布局…' }, width: 250, height: 72 },
    { id: 'dots', label: '点状加载状态', props: { kind: 'dots', size: 10, label: 'AI 正在思考' }, width: 190, height: 60 }
  ],
  Toast: [
    { id: 'success', label: '成功通知', props: { kind: 'success', title: '保存成功', description: '设计已同步到当前项目。', action: '' }, content: '显示成功通知' },
    { id: 'error', label: '错误通知', props: { kind: 'error', title: '保存失败', description: '服务暂时不可用，请稍后重试。', action: '重试' }, content: '显示错误通知' },
    { id: 'progress', label: '生成进度通知', props: { kind: 'progress', title: '正在生成页面', description: 'AI 正在完善视觉与响应式布局。', action: '后台运行' }, content: '显示生成进度' }
  ],
  AlertDialog: [
    { id: 'delete', label: '删除确认框', props: { kind: 'delete', title: '删除这个页面？', description: '页面中的组件和批注将一并删除。', size: 'sm' }, content: '删除页面' },
    { id: 'publish', label: '发布确认框', props: { kind: 'publish', title: '发布当前设计？', description: '将生成一个可分享的在线预览链接。', size: 'md' }, content: '发布设计' },
    { id: 'discard', label: '放弃修改确认框', props: { kind: 'discard', title: '放弃未保存的修改？', description: '最近 3 分钟内的操作将无法恢复。', size: 'sm' }, content: '关闭编辑器' }
  ],
  DropdownMenu: [
    { id: 'account', label: '账户菜单', props: { kind: 'account', items: [{ key: 'profile', label: '个人资料', shortcut: '⇧⌘P' }, { key: 'settings', label: '设置', shortcut: '⌘,' }, { key: 'logout', label: '退出登录', danger: true }] }, content: 'AI Designer' },
    { id: 'checkbox', label: '显示选项菜单', props: { kind: 'checkbox', items: [{ key: 'grid', label: '显示网格', checked: true }, { key: 'guides', label: '显示参考线', checked: true }, { key: 'rulers', label: '显示标尺', checked: false }] }, content: '视图选项' },
    { id: 'actions', label: '页面操作菜单', props: { kind: 'actions', items: [{ key: 'duplicate', label: '复制页面' }, { key: 'rename', label: '重命名' }, { key: 'delete', label: '删除页面', danger: true }] }, content: '页面操作' }
  ],
  ContextMenu: [
    { id: 'canvas', label: '画布右键菜单', props: { kind: 'canvas', items: [{ key: 'paste', label: '粘贴' }, { key: 'select', label: '全选' }, { key: 'frame', label: '创建画框' }] }, content: '在空白画布右键' },
    { id: 'text', label: '文字右键菜单', props: { kind: 'text', items: [{ key: 'edit', label: '编辑文本' }, { key: 'style', label: '复制样式' }, { key: 'convert', label: '转换为组件' }] }, content: '右键文字组件' },
    { id: 'image', label: '图片右键菜单', props: { kind: 'image', items: [{ key: 'replace', label: '替换图片' }, { key: 'crop', label: '裁剪' }, { key: 'download', label: '下载原图' }] }, content: '右键图片组件' }
  ],
  HoverCard: [
    { id: 'profile', label: '成员资料悬浮卡', props: { kind: 'profile', title: '林设计师', description: '产品设计师 · 负责官网与设计系统' }, content: '查看成员资料', width: 180 },
    { id: 'project', label: '项目数据悬浮卡', props: { kind: 'project', title: '品牌官网改版', description: '12 个页面 · 86 个组件' }, content: '查看项目概览', width: 180 },
    { id: 'status', label: '同步状态悬浮卡', props: { kind: 'status', title: '设计已同步', description: '刚刚保存到当前项目' }, content: '查看同步状态', width: 180 }
  ],
  Popover: [
    { id: 'dimensions', label: '尺寸设置气泡', props: { kind: 'dimensions', title: '组件尺寸' }, content: '调整尺寸' },
    { id: 'form', label: '快速表单气泡', props: { kind: 'form', title: '邀请协作者' }, content: '邀请成员', width: 170 },
    { id: 'command', label: '快捷操作气泡', props: { kind: 'command', title: '快速操作' }, content: '打开快捷操作', width: 180 }
  ],
  Tooltip: [
    { id: 'basic', label: '基础文字提示', props: { kind: 'basic', content: '保存当前设计' }, content: '保存' },
    { id: 'shortcut', label: '快捷键提示', props: { kind: 'shortcut', content: '打开命令面板', shortcut: '⌘ K' }, content: '命令面板' },
    { id: 'rich', label: '富内容提示', props: { kind: 'rich', content: '锁定后其他成员不能移动此组件。', title: '锁定组件' }, content: '锁定说明', width: 160 }
  ],
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
  InputGroup: [
    { id: 'email', label: '邮箱地址输入', props: { kind: 'email', prefix: '@', suffix: '.com', defaultValue: 'designer' }, content: '用户名' },
    { id: 'search', label: '带快捷键搜索框', props: { kind: 'search', prefix: '⌕', suffix: '⌘ K', defaultValue: '' }, content: '搜索页面和组件…', width: 360 },
    { id: 'currency', label: '人民币金额输入', props: { kind: 'currency', prefix: '¥', suffix: 'CNY', defaultValue: '12,800' }, content: '输入预算' }
  ],
  InputOTP: [
    { id: 'six', label: '六位验证码', props: { length: 6, defaultValue: '' } },
    { id: 'filled', label: '已填写验证码', props: { length: 6, defaultValue: '482619' } },
    { id: 'grouped', label: '分组验证码', props: { length: 6, defaultValue: '482', groupAt: 3 }, width: 340 }
  ],
  Textarea: [
    { id: 'default', label: '默认多行输入', props: { rows: 4, invalid: false, disabled: false, showCount: false }, content: '请输入详细说明' },
    { id: 'count', label: '带字数统计输入', props: { rows: 5, invalid: false, disabled: false, showCount: true, maxLength: 200, defaultValue: '首页需要体现 AI 设计能力与可视化编辑体验。' }, height: 130 },
    { id: 'invalid', label: '错误状态输入', props: { rows: 4, invalid: true, disabled: false, showCount: false, errorText: '项目说明不能少于 10 个字。' }, content: '补充项目说明', height: 120 }
  ],
  NativeSelect: [
    { id: 'framework', label: '框架选择器', props: { kind: 'framework', defaultValue: 'react', disabled: false, options: [{ value: 'react', label: 'React' }, { value: 'vue', label: 'Vue' }, { value: 'html', label: 'HTML' }] } },
    { id: 'status', label: '状态选择器', props: { kind: 'status', defaultValue: 'review', disabled: false, options: [{ value: 'draft', label: '草稿' }, { value: 'review', label: '待审核' }, { value: 'published', label: '已发布' }] } },
    { id: 'disabled', label: '禁用选择器', props: { kind: 'disabled', defaultValue: 'locked', disabled: true, options: [{ value: 'locked', label: '由项目设置锁定' }] } }
  ],
  Select: [
    { id: 'single', label: '单选设计方案', props: { kind: 'single', defaultValue: ['apple'], multiple: false, options: [{ value: 'apple', label: 'Apple 产品风格' }, { value: 'editorial', label: '杂志编辑风格' }, { value: 'dashboard', label: '数据看板风格' }] }, content: '选择设计方案', height: 210 },
    { id: 'multiple', label: '多选页面标签', props: { kind: 'multiple', defaultValue: ['responsive', 'marketing'], multiple: true, options: [{ value: 'responsive', label: '响应式' }, { value: 'marketing', label: '营销页' }, { value: 'commerce', label: '电商' }, { value: 'editorial', label: '内容型' }] }, content: '选择页面标签', width: 360, height: 250 },
    { id: 'disabled', label: '锁定选择器', props: { kind: 'disabled', defaultValue: ['system'], multiple: false, disabled: true, options: [{ value: 'system', label: '继承团队设计系统' }] }, content: '设计系统已锁定', height: 74 }
  ],
  Combobox: [
    { id: 'framework', label: '框架搜索选择', props: { kind: 'framework', defaultValue: ['next'], multiple: false, options: [{ value: 'next', label: 'Next.js' }, { value: 'vite', label: 'Vite' }, { value: 'astro', label: 'Astro' }] }, content: '搜索技术框架', height: 245 },
    { id: 'people', label: '成员搜索选择', props: { kind: 'people', defaultValue: ['lin'], multiple: false, options: [{ value: 'lin', label: '林设计师' }, { value: 'chen', label: '陈产品' }, { value: 'ai', label: 'AI Designer' }] }, content: '搜索项目成员', width: 340, height: 245 },
    { id: 'multiple', label: '多选技术栈', props: { kind: 'multiple', defaultValue: ['react', 'typescript'], multiple: true, options: [{ value: 'react', label: 'React' }, { value: 'typescript', label: 'TypeScript' }, { value: 'tailwind', label: 'Tailwind CSS' }, { value: 'radix', label: 'Radix UI' }] }, content: '搜索并添加技术栈', width: 380, height: 280 }
  ],
  Checkbox: [
    { id: 'unchecked', label: '未选复选框', props: { defaultChecked: false, disabled: false }, content: '接收产品更新' },
    { id: 'checked', label: '已选复选框', props: { defaultChecked: true, disabled: false }, content: '接受服务条款' },
    { id: 'disabled', label: '禁用复选框', props: { defaultChecked: true, disabled: true }, content: '企业策略强制启用' }
  ],
  Switch: [
    { id: 'off', label: '关闭状态开关', props: { defaultChecked: false, disabled: false }, content: '公开预览链接' },
    { id: 'on', label: '开启状态开关', props: { defaultChecked: true, disabled: false }, content: '自动保存设计' },
    { id: 'disabled', label: '禁用状态开关', props: { defaultChecked: true, disabled: true }, content: '团队同步由管理员控制' }
  ],
  RadioGroup: [
    { id: 'horizontal', label: '水平单选组', props: { kind: 'plain', orientation: 'horizontal', defaultValue: 'comfortable', options: [{ value: 'default', label: '默认' }, { value: 'comfortable', label: '舒适' }, { value: 'compact', label: '紧凑' }] }, width: 380 },
    { id: 'vertical', label: '垂直单选组', props: { kind: 'plain', orientation: 'vertical', defaultValue: 'desktop', options: [{ value: 'desktop', label: '桌面优先' }, { value: 'mobile', label: '移动优先' }, { value: 'adaptive', label: '自适应' }] }, width: 280, height: 130 },
    { id: 'cards', label: '单选方案卡片', props: { kind: 'cards', orientation: 'horizontal', defaultValue: 'pro', options: [{ value: 'basic', label: '基础版' }, { value: 'pro', label: '专业版' }, { value: 'team', label: '团队版' }] }, width: 440, height: 110 }
  ],
  Slider: [
    { id: 'value', label: '单值滑块', props: { kind: 'value', defaultValue: [58], min: 0, max: 100, step: 1 } },
    { id: 'range', label: '范围滑块', props: { kind: 'range', defaultValue: [24, 76], min: 0, max: 100, step: 1 }, height: 72 },
    { id: 'steps', label: '离散步进滑块', props: { kind: 'steps', defaultValue: [3], min: 1, max: 5, step: 1, marks: ['小', '中', '大', '宽', '满'] }, width: 360, height: 82 }
  ],
  Field: [
    { id: 'default', label: '基础字段', props: { kind: 'default', label: '用户名', description: '', error: '' }, content: '请输入用户名' },
    { id: 'description', label: '带帮助说明字段', props: { kind: 'description', label: '网站域名', description: '发布后仍可在项目设置中修改。', error: '' }, content: 'example.com', height: 112 },
    { id: 'error', label: '错误字段', props: { kind: 'error', label: '工作邮箱', description: '', error: '请输入有效的企业邮箱地址。' }, content: 'name@company.com', height: 112 }
  ],
  Card: [
    { id: 'project', label: '项目概览卡片', props: { kind: 'project', title: '品牌官网改版', description: '最近更新于 10 分钟前' }, content: '12 个页面 · 86 个组件', height: 210 },
    { id: 'activity', label: '协作动态卡片', props: { kind: 'activity', title: '团队动态', description: '今天有 6 项更新' }, content: '查看全部动态', height: 230 },
    { id: 'metric', label: '数据指标卡片', props: { kind: 'metric', title: '本周访问', description: '较上周增长 18.2%', featured: true }, content: '128,420', height: 210 }
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
