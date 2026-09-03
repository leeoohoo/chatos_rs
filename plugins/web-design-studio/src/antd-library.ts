import { type WebComponentType, type WebDesignComponent, type WebDesignJsonValue } from './schema.js';
import { componentDefaults } from './templates.js';

export type AntdCategory = '通用' | '布局' | '导航' | '数据录入' | '数据展示' | '反馈' | '其他';

export interface AntdComponentDefinition {
  id: string;
  label: string;
  category: AntdCategory;
  icon: string;
  keywords: string[];
  baseType: WebComponentType;
  content: string;
  width: number;
  height: number;
  props?: Record<string, WebDesignJsonValue>;
}

const item = (id: string, label: string, category: AntdCategory, icon: string, baseType: WebComponentType, width: number, height: number, content: string, props: Record<string, WebDesignJsonValue> = {}, keywords: string[] = []): AntdComponentDefinition => ({
  id, label, category, icon, baseType, width, height, content, props, keywords: [id, label, ...keywords]
});

export const ANTD_VERSION = '6.2.2';
export const ANTD_CATEGORIES: AntdCategory[] = ['通用', '布局', '导航', '数据录入', '数据展示', '反馈', '其他'];

export const ANTD_COMPONENTS: AntdComponentDefinition[] = [
  item('Button', '按钮', '通用', '▣', 'button', 150, 48, '主要按钮', { type: 'primary', size: 'middle' }),
  item('FloatButton', '悬浮按钮', '通用', '◉', 'button', 64, 64, '', { tooltip: '快捷操作' }),
  item('Icon', '图标', '通用', '✦', 'icon', 56, 56, ''),
  item('Typography', '排版', '通用', 'T', 'text', 320, 72, 'Ant Design 排版文本', { level: 3 }),

  item('Divider', '分割线', '布局', '—', 'divider', 360, 32, '分割线', { orientation: 'left' }),
  item('Flex', '弹性布局', '布局', '⇥', 'section', 360, 80, '', { gap: 'small', align: 'center' }),
  item('Grid', '栅格', '布局', '▦', 'section', 420, 90, '', { gutter: 8 }),
  item('Layout', '布局', '布局', '▤', 'section', 440, 150, '', {}),
  item('Masonry', '瀑布流', '布局', '▥', 'section', 420, 180, '', { columns: 3, gutter: 8 }),
  item('Space', '间距', '布局', '↔', 'section', 320, 64, '', { size: 'middle' }),
  item('Splitter', '分隔面板', '布局', '⋮', 'section', 440, 150, '', { orientation: 'horizontal' }),

  item('Anchor', '锚点', '导航', '⌁', 'list', 220, 150, '', { items: [{ key: 'overview', href: '#overview', title: '概览' }, { key: 'features', href: '#features', title: '功能' }, { key: 'api', href: '#api', title: 'API' }] }),
  item('Breadcrumb', '面包屑', '导航', '›', 'list', 320, 48, '', { items: [{ title: '首页' }, { title: '产品' }, { title: '详情' }] }),
  item('Dropdown', '下拉菜单', '导航', '⌄', 'button', 150, 44, '更多操作', { menu: { items: [{ key: '1', label: '编辑' }, { key: '2', label: '复制' }, { key: '3', label: '删除', danger: true }] } }),
  item('Menu', '导航菜单', '导航', '☰', 'list', 420, 48, '', { mode: 'horizontal', selectedKeys: ['home'], items: [{ key: 'home', label: '首页' }, { key: 'product', label: '产品' }, { key: 'about', label: '关于' }] }),
  item('Pagination', '分页', '导航', '•••', 'list', 360, 48, '', { current: 1, total: 80, showSizeChanger: false }),
  item('Steps', '步骤条', '导航', '①', 'list', 500, 74, '', { current: 1, items: [{ title: '创建' }, { title: '配置' }, { title: '完成' }] }),

  item('AutoComplete', '自动完成', '数据录入', '⌨', 'input', 260, 44, '输入关键词', { options: [{ value: 'Apple' }, { value: 'Ant Design' }, { value: 'AI Website' }] }),
  item('Cascader', '级联选择', '数据录入', '⌄', 'select', 260, 44, '请选择城市', { options: [{ value: 'zhejiang', label: '浙江', children: [{ value: 'hangzhou', label: '杭州' }] }, { value: 'jiangsu', label: '江苏', children: [{ value: 'nanjing', label: '南京' }] }] }),
  item('Checkbox', '多选框', '数据录入', '☑', 'checkbox', 220, 44, '同意服务条款', { defaultChecked: true }),
  item('ColorPicker', '颜色选择器', '数据录入', '◒', 'input', 84, 44, '', { defaultValue: '#007AFF', showText: true }),
  item('DatePicker', '日期选择框', '数据录入', '▣', 'input', 220, 44, '', { placeholder: '选择日期' }),
  item('Form', '表单', '数据录入', '▤', 'section', 360, 180, '', { layout: 'vertical' }),
  item('Input', '输入框', '数据录入', '⌨', 'input', 280, 44, '请输入内容', { allowClear: true }),
  item('InputNumber', '数字输入框', '数据录入', '#', 'input', 180, 44, '', { defaultValue: 100, min: 0, max: 1000 }),
  item('Mentions', '提及', '数据录入', '@', 'textarea', 320, 88, '输入 @ 提及成员', { options: [{ value: 'ai', label: 'AI 助手' }, { value: 'designer', label: '设计师' }] }),
  item('Radio', '单选框', '数据录入', '◉', 'checkbox', 300, 44, '', { defaultValue: 'a', options: [{ label: '选项 A', value: 'a' }, { label: '选项 B', value: 'b' }] }),
  item('Rate', '评分', '数据录入', '★', 'list', 220, 44, '', { defaultValue: 4, allowHalf: true }),
  item('Select', '选择器', '数据录入', '⌄', 'select', 260, 44, '请选择', { defaultValue: 'apple', options: [{ value: 'apple', label: 'Apple' }, { value: 'antd', label: 'Ant Design' }, { value: 'web', label: '网站设计' }] }),
  item('Slider', '滑动输入条', '数据录入', '━', 'input', 300, 44, '', { defaultValue: 48 }),
  item('Switch', '开关', '数据录入', '◉', 'switch', 150, 44, '启用通知', { defaultChecked: true }),
  item('TimePicker', '时间选择框', '数据录入', '◷', 'input', 220, 44, '', { placeholder: '选择时间' }),
  item('Transfer', '穿梭框', '数据录入', '⇄', 'section', 520, 220, '', { targetKeys: ['2'], dataSource: [{ key: '1', title: '内容一' }, { key: '2', title: '内容二' }, { key: '3', title: '内容三' }] }),
  item('TreeSelect', '树选择', '数据录入', '⌄', 'select', 280, 44, '请选择节点', { treeData: [{ value: 'parent', title: '父节点', children: [{ value: 'child-1', title: '子节点一' }, { value: 'child-2', title: '子节点二' }] }] }),
  item('Upload', '上传', '数据录入', '↑', 'button', 150, 44, '上传文件', { showUploadList: false }),

  item('Avatar', '头像', '数据展示', '●', 'avatar', 64, 64, 'AI', { size: 48 }),
  item('Badge', '徽标数', '数据展示', '●', 'badge', 120, 52, '消息', { count: 8 }),
  item('Calendar', '日历', '数据展示', '▦', 'card', 420, 320, '', { fullscreen: false }),
  item('Card', '卡片', '数据展示', '▤', 'card', 320, 160, '这里是卡片内容', { title: '产品卡片', bordered: true }),
  item('Carousel', '走马灯', '数据展示', '▣', 'card', 380, 180, '', { autoplay: false }),
  item('Collapse', '折叠面板', '数据展示', '⌄', 'list', 380, 160, '', { defaultActiveKey: ['1'], items: [{ key: '1', label: '什么是 Ant Design？', children: '企业级产品设计体系。' }, { key: '2', label: '可以编辑吗？', children: '可以继续调整内容和属性。' }] }),
  item('Descriptions', '描述列表', '数据展示', '☷', 'table', 480, 150, '', { title: '产品信息', column: 2, items: [{ key: '1', label: '名称', children: 'Web Design Studio' }, { key: '2', label: '版本', children: '0.9.0' }, { key: '3', label: '状态', children: '开发中' }] }),
  item('Empty', '空状态', '数据展示', '∅', 'card', 280, 160, '暂无数据', {}),
  item('Image', '图片', '数据展示', '▧', 'image', 240, 160, '', { src: 'https://images.unsplash.com/photo-1550745165-9bc0b252726f?auto=format&fit=crop&w=600&q=80', preview: false }),
  item('List', '列表', '数据展示', '☷', 'list', 360, 190, '', { bordered: true, dataSource: ['可视化编辑', '响应式页面', 'AI 协作修改'] }),
  item('Popover', '气泡卡片', '数据展示', '▢', 'button', 160, 44, '查看详情', { title: '产品信息', content: '这是一个气泡卡片。' }),
  item('QRCode', '二维码', '数据展示', '▦', 'image', 170, 170, '', { value: 'https://ant.design', size: 140 }),
  item('Segmented', '分段控制器', '数据展示', '▥', 'button', 300, 44, '', { defaultValue: '日', options: ['日', '周', '月'] }),
  item('Statistic', '统计数值', '数据展示', '#', 'card', 220, 100, '', { title: '活跃用户', value: 112893, suffix: '人' }),
  item('Table', '表格', '数据展示', '▦', 'table', 520, 220, '', { pagination: false, size: 'small', columns: [{ title: '名称', dataIndex: 'name', key: 'name' }, { title: '状态', dataIndex: 'status', key: 'status' }, { title: '数量', dataIndex: 'count', key: 'count' }], dataSource: [{ key: '1', name: '网站组件', status: '可用', count: 20 }, { key: '2', name: 'Ant Design', status: '新增', count: 60 }] }),
  item('Tabs', '标签页', '数据展示', '▤', 'card', 420, 150, '', { defaultActiveKey: '1', items: [{ key: '1', label: '概览', children: '产品概览内容' }, { key: '2', label: '功能', children: '功能说明内容' }, { key: '3', label: '设置', children: '设置内容' }] }),
  item('Tag', '标签', '数据展示', '◆', 'badge', 100, 36, '已发布', { color: 'blue' }),
  item('Timeline', '时间轴', '数据展示', '│', 'list', 300, 180, '', { items: [{ children: '创建项目' }, { children: '完成设计', color: 'blue' }, { children: '准备发布', color: 'gray' }] }),
  item('Tooltip', '文字提示', '数据展示', '?', 'button', 150, 44, '悬停查看', { title: '这是提示文字' }),
  item('Tree', '树形控件', '数据展示', '⌁', 'list', 300, 180, '', { defaultExpandAll: true, treeData: [{ title: '页面', key: '0', children: [{ title: '首页', key: '0-0' }, { title: '关于', key: '0-1' }] }] }),

  item('Alert', '警告提示', '反馈', '!', 'card', 380, 74, '', { type: 'info', message: '设计已自动保存', showIcon: true }),
  item('Modal', '对话框', '反馈', '▣', 'button', 160, 44, '打开对话框', { title: '确认操作' }),
  item('Drawer', '抽屉', '反馈', '▤', 'button', 150, 44, '打开抽屉', { title: '详情面板' }),
  item('Popconfirm', '气泡确认框', '反馈', '?', 'button', 160, 44, '删除项目', { title: '确定删除吗？' }),
  item('Progress', '进度条', '反馈', '◔', 'card', 320, 70, '', { percent: 68 }),
  item('Result', '结果', '反馈', '✓', 'card', 380, 240, '', { status: 'success', title: '发布成功', subTitle: '页面已经成功发布。' }),
  item('Skeleton', '骨架屏', '反馈', '▧', 'card', 360, 150, '', { active: true, paragraph: { rows: 3 } }),
  item('Spin', '加载中', '反馈', '◌', 'card', 180, 100, '加载中…', { size: 'large' }),

  item('Affix', '固钉', '其他', '⌖', 'button', 150, 44, '固定操作', { offsetTop: 20 }),
  item('Watermark', '水印', '其他', 'W', 'card', 360, 180, '', { content: 'Web Design Studio' })
];

export function createAntdComponent(definitionId: string, x: number, y: number): WebDesignComponent {
  const definition = ANTD_COMPONENTS.find((candidate) => candidate.id === definitionId);
  if (!definition) throw new Error(`Ant Design component not found: ${definitionId}`);
  const component = componentDefaults(definition.baseType, x, y);
  component.id = `antd-${definition.id.toLowerCase()}-${globalThis.crypto.randomUUID().slice(0, 8)}`;
  component.name = `Ant Design · ${definition.id} ${definition.label}`;
  component.width = definition.width;
  component.height = definition.height;
  component.content = definition.content;
  component.style = { background: 'transparent', color: '#1D1D1F', borderWidth: 0, borderRadius: 0, fontSize: 14, fontWeight: 400 };
  component.library = { name: 'antd', version: ANTD_VERSION, component: definition.id, props: structuredClone(definition.props ?? {}) };
  return component;
}
