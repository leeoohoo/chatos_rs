import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';
import { type WebComponentType, type WebDesignComponent, type WebDesignJsonValue } from './schema.js';

export type AntdCategory = '通用' | '布局' | '导航' | '数据录入' | '数据展示' | '反馈' | '其他';

export type AntdComponentDefinition = UiComponentDefinition<AntdCategory>;
export type AntdComponentVariant = UiComponentVariant;

const baseItem = defineUiComponent<AntdCategory>;
type AntdItemArgs = Parameters<typeof baseItem>;

const ANTD_COMPONENT_METADATA: Partial<Record<string, Pick<AntdComponentDefinition, 'introduced' | 'status'>>> = {
  BorderBeam: { introduced: '6.4.0' },
  List: { status: 'deprecated' },
  Listy: { introduced: '6.6.0' }
};

function item(...args: AntdItemArgs): AntdComponentDefinition {
  const definition = baseItem(...args);
  const slug = definition.id.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase();
  return {
    ...definition,
    docsUrl: `https://ant.design/components/${slug}-cn`,
    status: 'stable',
    ...ANTD_COMPONENT_METADATA[definition.id]
  };
}

export const ANTD_VERSION = '6.6.2';
export const ANTD_OFFICIAL_COMPONENT_COUNT = 72;
export const ANTD_CATEGORIES: AntdCategory[] = ['通用', '布局', '导航', '数据录入', '数据展示', '反馈', '其他'];

export const ANTD_COMPONENT_VARIANTS: Record<string, AntdComponentVariant[]> = {
  FloatButton: [
    { id: 'default', label: '默认悬浮按钮', props: { type: 'default', shape: 'circle', tooltip: '快捷操作' } },
    { id: 'primary', label: '主要悬浮按钮', props: { type: 'primary', shape: 'circle', tooltip: '主要操作' } },
    { id: 'square', label: '方形悬浮按钮', props: { type: 'default', shape: 'square', tooltip: '快捷操作' } }
  ],
  Icon: [
    { id: 'primary', label: '主色图标', props: { color: '#1677ff', size: 32 } },
    { id: 'success', label: '成功图标', props: { color: '#52c41a', size: 32 } },
    { id: 'warning', label: '警告图标', props: { color: '#faad14', size: 32 } },
    { id: 'large', label: '大型图标', props: { color: '#722ed1', size: 44 }, width: 68, height: 68 }
  ],
  Button: [
    { id: 'primary', label: '主色实心按钮', props: { color: 'primary', variant: 'solid', danger: false }, content: '主要按钮' },
    { id: 'default', label: '默认描边按钮', props: { color: 'default', variant: 'outlined', danger: false }, content: '默认按钮' },
    { id: 'dashed', label: '虚线按钮', props: { color: 'default', variant: 'dashed', danger: false }, content: '虚线按钮' },
    { id: 'filled', label: '柔和填充按钮', props: { color: 'primary', variant: 'filled', danger: false }, content: '填充按钮' },
    { id: 'text', label: '文本按钮', props: { color: 'primary', variant: 'text', danger: false }, content: '文本按钮' },
    { id: 'link', label: '链接按钮', props: { color: 'primary', variant: 'link', danger: false }, content: '链接按钮' },
    { id: 'danger', label: '危险实心按钮', props: { color: 'danger', variant: 'solid', danger: true }, content: '危险操作' },
    { id: 'gradient', label: '渐变按钮', props: { color: 'primary', variant: 'solid', gradient: true, danger: false }, content: 'AI 生成' }
  ],
  Input: [
    { id: 'outlined', label: '描边输入框', props: { variant: 'outlined', size: 'middle' }, content: '请输入内容' },
    { id: 'filled', label: '填充输入框', props: { variant: 'filled', size: 'middle' }, content: '请输入内容' },
    { id: 'borderless', label: '无边框输入框', props: { variant: 'borderless', size: 'middle' }, content: '请输入内容' },
    { id: 'underlined', label: '下划线输入框', props: { variant: 'underlined', size: 'middle' }, content: '请输入内容' },
    { id: 'search', label: '搜索输入框', props: { variant: 'outlined', size: 'middle', enterButton: true }, content: '搜索内容' },
    { id: 'password', label: '密码输入框', props: { variant: 'outlined', size: 'middle' }, content: '请输入密码' },
    { id: 'textarea', label: '多行输入框', props: { variant: 'outlined', autoSize: { minRows: 3, maxRows: 5 } }, content: '请输入详细内容', height: 96 },
    { id: 'otp', label: '一次性密码输入框', props: { length: 6, size: 'large' }, content: '', width: 360, height: 54 },
    { id: 'prefix-suffix', label: '前后缀输入框', props: { variant: 'outlined', prefixText: 'https://', suffixText: '.com' }, content: 'your-site' }
  ],
  Select: [
    { id: 'outlined', label: '描边选择器', props: { variant: 'outlined', mode: null } },
    { id: 'filled', label: '填充选择器', props: { variant: 'filled', mode: null } },
    { id: 'borderless', label: '无边框选择器', props: { variant: 'borderless', mode: null } },
    { id: 'underlined', label: '下划线选择器', props: { variant: 'underlined', mode: null } },
    { id: 'multiple', label: '多选选择器', props: { variant: 'outlined', mode: 'multiple', defaultValue: ['apple', 'antd'] }, height: 52 },
    { id: 'tags', label: '标签选择器', props: { variant: 'outlined', mode: 'tags', defaultValue: ['AI'] }, height: 52 },
    { id: 'search', label: '可搜索选择器', props: { variant: 'outlined', showSearch: true, optionFilterProp: 'label', allowClear: true, mode: null } },
    { id: 'grouped', label: '分组选项选择器', props: { variant: 'outlined', mode: null, optionGroups: [{ label: '设计', options: [{ value: 'figma', label: 'Figma' }, { value: 'web', label: '网站设计' }] }, { label: '开发', options: [{ value: 'react', label: 'React' }, { value: 'ai', label: 'AI 应用' }] }] } },
    { id: 'large', label: '大型选择器', props: { variant: 'outlined', size: 'large', mode: null }, height: 52 }
  ],
  Typography: [
    { id: 'title', label: '标题', props: { level: 2 }, content: '设计系统标题', height: 72 },
    { id: 'subtitle', label: '小标题', props: { level: 4 }, content: '清晰的小节标题', height: 54 },
    { id: 'paragraph', label: '段落', props: {}, content: '这是一段用于说明产品价值和使用场景的正文内容。', height: 88 },
    { id: 'text', label: '行内文本', props: {}, content: '用于标签、说明或简短文案', height: 42 }
  ],
  Flex: [
    { id: 'horizontal', label: '横向排列', props: { vertical: false, gap: 'middle', align: 'center', justify: 'start' } },
    { id: 'between', label: '两端对齐', props: { vertical: false, gap: 'middle', align: 'center', justify: 'space-between' } },
    { id: 'vertical', label: '纵向排列', props: { vertical: true, gap: 'small', align: 'start', justify: 'start' }, height: 150 },
    { id: 'wrap', label: '自动换行', props: { vertical: false, gap: 'small', align: 'center', wrap: true }, height: 110 }
  ],
  Divider: [
    { id: 'left', label: '左侧标题分割线', props: { orientation: 'left', dashed: false, type: 'horizontal' }, content: '分割线' },
    { id: 'center', label: '居中标题分割线', props: { orientation: 'center', dashed: false, type: 'horizontal' }, content: '内容分组' },
    { id: 'dashed', label: '虚线分割线', props: { orientation: 'left', dashed: true, type: 'horizontal' }, content: '虚线分割' },
    { id: 'plain', label: '无文字分割线', props: { orientation: 'center', dashed: false, type: 'horizontal' }, content: '' }
  ],
  Grid: [
    { id: 'two', label: '两列栅格', props: { gutter: 12, columns: 2 }, width: 420, height: 90 },
    { id: 'three', label: '三列栅格', props: { gutter: 8, columns: 3 }, width: 420, height: 90 },
    { id: 'four', label: '四列栅格', props: { gutter: 8, columns: 4 }, width: 480, height: 90 }
  ],
  Layout: [
    { id: 'sidebar', label: '左侧导航布局', props: {}, width: 460, height: 180 },
    { id: 'top', label: '顶部导航布局', props: {}, width: 460, height: 180 },
    { id: 'right-sidebar', label: '右侧导航布局', props: {}, width: 460, height: 180 }
  ],
  Masonry: [
    { id: 'two', label: '两列瀑布流', props: { columns: 2, gutter: 12 }, width: 400, height: 220 },
    { id: 'three', label: '三列瀑布流', props: { columns: 3, gutter: 8 }, width: 440, height: 220 },
    { id: 'four', label: '四列瀑布流', props: { columns: 4, gutter: 8 }, width: 500, height: 220 }
  ],
  Space: [
    { id: 'compact', label: '紧凑间距', props: { size: 'small', direction: 'horizontal', wrap: false } },
    { id: 'default', label: '标准间距', props: { size: 'middle', direction: 'horizontal', wrap: false } },
    { id: 'large', label: '宽松间距', props: { size: 'large', direction: 'horizontal', wrap: false }, width: 360 },
    { id: 'vertical', label: '纵向间距', props: { size: 'small', direction: 'vertical', wrap: false }, width: 180, height: 130 }
  ],
  Splitter: [
    { id: 'horizontal', label: '水平分隔面板', props: { orientation: 'horizontal' }, width: 460, height: 170 },
    { id: 'vertical', label: '垂直分隔面板', props: { orientation: 'vertical' }, width: 420, height: 220 },
    { id: 'collapsible', label: '可折叠分隔面板', props: { orientation: 'horizontal', collapsible: true }, width: 480, height: 190 }
  ],
  Anchor: [
    { id: 'vertical', label: '垂直锚点', props: { direction: 'vertical', affix: false }, width: 220, height: 150 },
    { id: 'horizontal', label: '水平锚点', props: { direction: 'horizontal', affix: false }, width: 460, height: 54 },
    { id: 'bounds', label: '紧凑锚点', props: { direction: 'vertical', affix: false, bounds: 8 }, width: 200, height: 140 }
  ],
  Breadcrumb: [
    { id: 'default', label: '标准面包屑', props: { separator: '/' } },
    { id: 'arrow', label: '箭头面包屑', props: { separator: '>' } },
    { id: 'dot', label: '圆点面包屑', props: { separator: '·' } }
  ],
  Menu: [
    { id: 'horizontal', label: '顶部导航', props: { mode: 'horizontal', theme: 'light', defaultSelectedKeys: ['home'] }, width: 520, height: 48 },
    { id: 'vertical', label: '垂直菜单', props: { mode: 'vertical', theme: 'light', defaultSelectedKeys: ['home'] }, width: 240, height: 150 },
    { id: 'inline', label: '内嵌侧栏', props: { mode: 'inline', theme: 'light', defaultSelectedKeys: ['product'] }, width: 240, height: 150 },
    { id: 'dark', label: '深色导航', props: { mode: 'horizontal', theme: 'dark', defaultSelectedKeys: ['home'] }, width: 520, height: 48 }
  ],
  Dropdown: [
    { id: 'hover', label: '悬停菜单', props: { trigger: ['hover'], placement: 'bottomLeft', arrow: false } },
    { id: 'click', label: '点击菜单', props: { trigger: ['click'], placement: 'bottomLeft', arrow: false } },
    { id: 'arrow', label: '带箭头菜单', props: { trigger: ['click'], placement: 'bottom', arrow: true } }
  ],
  Pagination: [
    { id: 'default', label: '标准分页', props: { defaultCurrent: 1, total: 80, showSizeChanger: false }, width: 380 },
    { id: 'compact', label: '迷你分页', props: { defaultCurrent: 1, total: 80, size: 'small', showSizeChanger: false }, width: 320 },
    { id: 'simple', label: '简单分页', props: { defaultCurrent: 1, total: 80, simple: true }, width: 220 },
    { id: 'jumper', label: '快速跳转', props: { defaultCurrent: 1, total: 200, showQuickJumper: true, showSizeChanger: false }, width: 460 }
  ],
  Steps: [
    { id: 'horizontal', label: '横向步骤', props: { direction: 'horizontal', current: 1, progressDot: false }, width: 520, height: 74 },
    { id: 'dots', label: '点状步骤', props: { direction: 'horizontal', current: 1, progressDot: true }, width: 520, height: 74 },
    { id: 'vertical', label: '纵向步骤', props: { direction: 'vertical', current: 1, progressDot: false }, width: 300, height: 210 }
  ],
  Form: [
    { id: 'vertical', label: '纵向表单', props: { layout: 'vertical', size: 'middle' }, width: 360, height: 210 },
    { id: 'horizontal', label: '横向表单', props: { layout: 'horizontal', size: 'middle', labelCol: { span: 6 }, wrapperCol: { span: 18 } }, width: 460, height: 180 },
    { id: 'compact', label: '紧凑表单', props: { layout: 'vertical', size: 'small' }, width: 330, height: 185 },
    { id: 'inline-login', label: '内联登录栏', props: { layout: 'inline', size: 'middle', formTemplate: 'login' }, width: 620, height: 72 },
    { id: 'login', label: '登录表单', props: { layout: 'vertical', size: 'large', formTemplate: 'login' }, width: 380, height: 250 },
    { id: 'registration', label: '注册表单', props: { layout: 'vertical', size: 'middle', formTemplate: 'registration' }, width: 400, height: 360 }
  ],
  Radio: [
    { id: 'radio', label: '普通单选', props: { optionType: 'default', buttonStyle: 'outline', defaultValue: 'a' } },
    { id: 'button', label: '按钮单选', props: { optionType: 'button', buttonStyle: 'outline', defaultValue: 'a' } },
    { id: 'solid', label: '填充按钮单选', props: { optionType: 'button', buttonStyle: 'solid', defaultValue: 'a' } }
  ],
  AutoComplete: [
    { id: 'outlined', label: '描边自动完成', props: { variant: 'outlined', allowClear: true } },
    { id: 'filled', label: '填充自动完成', props: { variant: 'filled', allowClear: true } },
    { id: 'borderless', label: '无边框自动完成', props: { variant: 'borderless', allowClear: true } }
  ],
  Cascader: [
    { id: 'outlined', label: '描边级联', props: { variant: 'outlined', multiple: false } },
    { id: 'filled', label: '填充级联', props: { variant: 'filled', multiple: false } },
    { id: 'multiple', label: '多选级联', props: { variant: 'outlined', multiple: true }, height: 52 },
    { id: 'search', label: '可搜索级联', props: { variant: 'outlined', multiple: false, showSearch: true } }
  ],
  Checkbox: [
    { id: 'checked', label: '已选中', props: { defaultChecked: true, disabled: false }, content: '已同意服务条款' },
    { id: 'unchecked', label: '未选中', props: { defaultChecked: false, disabled: false }, content: '订阅产品更新' },
    { id: 'indeterminate', label: '半选状态', props: { defaultChecked: false, indeterminate: true, disabled: false }, content: '选择部分项目' },
    { id: 'disabled', label: '禁用状态', props: { defaultChecked: true, disabled: true }, content: '不可修改' }
  ],
  ColorPicker: [
    { id: 'default', label: '标准颜色选择器', props: { defaultValue: '#1677ff', showText: true, size: 'middle', disabled: false } },
    { id: 'compact', label: '紧凑颜色选择器', props: { defaultValue: '#52c41a', showText: false, size: 'small', disabled: false }, width: 54 },
    { id: 'large', label: '大型颜色选择器', props: { defaultValue: '#722ed1', showText: true, size: 'large', disabled: false }, width: 110, height: 52 },
    { id: 'disabled', label: '禁用颜色选择器', props: { defaultValue: '#8c8c8c', showText: true, size: 'middle', disabled: true } }
  ],
  InputNumber: [
    { id: 'outlined', label: '描边数字框', props: { variant: 'outlined', controls: true } },
    { id: 'filled', label: '填充数字框', props: { variant: 'filled', controls: true } },
    { id: 'underlined', label: '下划线数字框', props: { variant: 'underlined', controls: true } },
    { id: 'no-controls', label: '无控制按钮', props: { variant: 'outlined', controls: false } }
  ],
  Mentions: [
    { id: 'default', label: '标准提及输入', props: { variant: 'outlined', autoSize: { minRows: 2, maxRows: 4 } }, height: 88 },
    { id: 'filled', label: '填充提及输入', props: { variant: 'filled', autoSize: { minRows: 2, maxRows: 4 } }, height: 88 },
    { id: 'long', label: '长文本提及输入', props: { variant: 'outlined', autoSize: { minRows: 4, maxRows: 7 } }, height: 140 }
  ],
  Rate: [
    { id: 'default', label: '五星评分', props: { defaultValue: 4, allowHalf: false, count: 5, disabled: false } },
    { id: 'half', label: '半星评分', props: { defaultValue: 3.5, allowHalf: true, count: 5, disabled: false } },
    { id: 'ten', label: '十分评分', props: { defaultValue: 8, allowHalf: false, count: 10, disabled: false }, width: 360 },
    { id: 'readonly', label: '只读评分', props: { defaultValue: 4.5, allowHalf: true, count: 5, disabled: true } }
  ],
  Slider: [
    { id: 'single', label: '单值滑块', props: { defaultValue: 48, range: false, vertical: false } },
    { id: 'range', label: '范围滑块', props: { defaultValue: [20, 70], range: true, vertical: false } },
    { id: 'marks', label: '带刻度滑块', props: { defaultValue: 50, range: false, vertical: false, marks: { 0: '0', 50: '50', 100: '100' } }, height: 64 },
    { id: 'vertical', label: '垂直滑块', props: { defaultValue: 48, range: false, vertical: true }, width: 64, height: 180 }
  ],
  Switch: [
    { id: 'on', label: '开启状态', props: { defaultChecked: true, size: 'default', disabled: false }, content: '已启用' },
    { id: 'off', label: '关闭状态', props: { defaultChecked: false, size: 'default', disabled: false }, content: '未启用' },
    { id: 'small', label: '小型开关', props: { defaultChecked: true, size: 'small', disabled: false }, content: '紧凑模式' },
    { id: 'disabled', label: '禁用开关', props: { defaultChecked: true, size: 'default', disabled: true }, content: '不可修改' }
  ],
  TimePicker: [
    { id: 'outlined', label: '描边时间', props: { variant: 'outlined', use12Hours: false } },
    { id: 'filled', label: '填充时间', props: { variant: 'filled', use12Hours: false } },
    { id: 'underlined', label: '下划线时间', props: { variant: 'underlined', use12Hours: false } },
    { id: 'twelve-hour', label: '12 小时制', props: { variant: 'outlined', use12Hours: true, format: 'h:mm a' } }
  ],
  Transfer: [
    { id: 'default', label: '标准穿梭框', props: { oneWay: false, showSearch: false, showSelectAll: true }, width: 520, height: 220 },
    { id: 'search', label: '可搜索穿梭框', props: { oneWay: false, showSearch: true, showSelectAll: true }, width: 560, height: 250 },
    { id: 'one-way', label: '单向穿梭框', props: { oneWay: true, showSearch: false, showSelectAll: true }, width: 520, height: 220 }
  ],
  TreeSelect: [
    { id: 'single', label: '单选树选择', props: { multiple: false, treeCheckable: false, allowClear: true } },
    { id: 'multiple', label: '多选树选择', props: { multiple: true, treeCheckable: false, allowClear: true }, height: 52 },
    { id: 'checkable', label: '复选树选择', props: { multiple: true, treeCheckable: true, allowClear: true }, height: 52 }
  ],
  Upload: [
    { id: 'button', label: '按钮上传', props: { listType: 'text', showUploadList: false, multiple: false } },
    { id: 'multiple', label: '多文件上传', props: { listType: 'text', showUploadList: true, multiple: true }, width: 180, height: 70 },
    { id: 'picture', label: '图片上传', props: { listType: 'picture-card', showUploadList: false, multiple: false }, width: 110, height: 110 },
    { id: 'dragger', label: '拖拽上传', props: { multiple: true, showUploadList: true }, width: 420, height: 180 }
  ],
  DatePicker: [
    { id: 'outlined', label: '描边日期', props: { variant: 'outlined' } },
    { id: 'filled', label: '填充日期', props: { variant: 'filled' } },
    { id: 'borderless', label: '无边框日期', props: { variant: 'borderless' } },
    { id: 'underlined', label: '下划线日期', props: { variant: 'underlined' } },
    { id: 'range', label: '日期范围选择器', props: { variant: 'outlined', pickerMode: 'range' }, width: 360 },
    { id: 'multiple', label: '多日期选择器', props: { variant: 'outlined', multiple: true, needConfirm: true }, width: 320, height: 52 },
    { id: 'datetime', label: '日期时间选择器', props: { variant: 'outlined', showTime: true, showNow: true }, width: 280 }
  ],
  Card: [
    { id: 'default', label: '默认卡片', props: { size: 'default', bordered: true } },
    { id: 'small', label: '紧凑卡片', props: { size: 'small', bordered: true } },
    { id: 'borderless', label: '无边框卡片', props: { size: 'default', bordered: false } },
    { id: 'hoverable', label: '悬浮卡片', props: { size: 'default', bordered: true, hoverable: true } }
  ],
  List: [
    { id: 'basic', label: '基础列表', props: { bordered: false, size: 'default', dataSource: ['设计页面结构', '选择视觉风格', '与 AI 一起完善细节'] }, width: 380, height: 180 },
    { id: 'bordered', label: '带边框列表', props: { bordered: true, size: 'default', dataSource: ['设计页面结构', '选择视觉风格', '与 AI 一起完善细节'] }, width: 380, height: 190 },
    { id: 'compact', label: '紧凑列表', props: { bordered: true, size: 'small', dataSource: ['待处理设计请求', '组件属性更新', '响应式检查'] }, width: 360, height: 150 },
    { id: 'metadata', label: '图文信息列表', props: { bordered: false, itemLayout: 'horizontal', dataSource: [{ title: '设计系统', description: '统一颜色、字体、圆角与间距。', avatar: 'DS' }, { title: 'AI 协作', description: '针对整页或组件提交修改任务。', avatar: 'AI' }, { title: '响应式', description: '分别检查桌面、平板和手机布局。', avatar: 'R' }] }, width: 440, height: 230 },
    { id: 'actions', label: '操作列表', props: { bordered: true, itemLayout: 'horizontal', dataSource: [{ title: '首页设计', description: '刚刚更新' }, { title: '定价页面', description: '2 个待处理批注' }, { title: '登录流程', description: '等待确认' }] }, width: 460, height: 230 },
    { id: 'grid', label: '网格卡片列表', props: { grid: { gutter: 12, column: 2 }, dataSource: [{ title: '组件库', description: '72 个组件' }, { title: '视觉主题', description: '6 套主题' }, { title: '页面模板', description: '快速起稿' }, { title: 'AI 修改', description: '精确到组件' }] }, width: 460, height: 230 },
    { id: 'pagination', label: '分页列表', props: { bordered: false, size: 'default', pagination: { pageSize: 3, position: 'bottom', align: 'center' }, dataSource: ['首页设计', '定价页面', '登录流程', '工作台', '数据报表', '设置中心'] }, width: 420, height: 260 },
    { id: 'vertical', label: '竖排图文列表', props: { itemLayout: 'vertical', dataSource: [{ title: 'AI 网站设计', description: '通过自然语言和可视化画布共同完成网页。' }, { title: '成熟组件库', description: '直接复用 Ant Design、Chakra UI 和 shadcn/ui。' }] }, width: 500, height: 260 }
  ],
  Listy: [
    { id: 'basic', label: '基础虚拟列表', props: { height: 260, virtual: false, itemCount: 20 }, width: 420, height: 280 },
    { id: 'virtual', label: '万条虚拟滚动', props: { height: 300, virtual: true, itemCount: 10000 }, width: 440, height: 320 },
    { id: 'grouped', label: '分组与吸顶', props: { height: 300, virtual: true, sticky: true, itemCount: 80 }, width: 440, height: 320 },
    { id: 'rich', label: '复杂内容列表', props: { height: 320, virtual: true, itemCount: 60 }, width: 480, height: 340 },
    { id: 'drag-sorting', label: '拖拽排序列表', props: { height: 300, virtual: false, itemCount: 12 }, width: 440, height: 320 },
    { id: 'infinite', label: '无限加载列表', props: { height: 280, virtual: true, itemCount: 200 }, width: 440, height: 340 },
    { id: 'style-class', label: '自定义语义样式', props: { height: 300, virtual: false, itemCount: 16, semanticStyle: true }, width: 440, height: 320 },
    { id: 'scroll-control', label: '滚动控制列表', props: { height: 280, virtual: true, itemCount: 200 }, width: 440, height: 340 }
  ],
  Table: [
    { id: 'basic', label: '基础表格', props: { bordered: false, size: 'middle', pagination: false }, width: 520, height: 220 },
    { id: 'bordered', label: '带边框表格', props: { bordered: true, size: 'middle', pagination: false }, width: 520, height: 220 },
    { id: 'compact', label: '紧凑表格', props: { bordered: true, size: 'small', pagination: false }, width: 500, height: 190 },
    { id: 'pagination', label: '分页表格', props: { bordered: false, size: 'middle', pagination: { pageSize: 2 } }, width: 540, height: 260 },
    { id: 'selection', label: '可选择表格', props: { bordered: false, size: 'middle', pagination: false, rowSelection: { type: 'checkbox' } }, width: 560, height: 240 },
    { id: 'expandable', label: '可展开表格', props: { bordered: true, size: 'middle', pagination: false, expandable: { defaultExpandedRowKeys: ['1'] } }, width: 580, height: 280 },
    { id: 'scroll', label: '固定表头表格', props: { bordered: true, size: 'small', pagination: false, scroll: { y: 180 } }, width: 560, height: 250 }
  ],
  Tabs: [
    { id: 'line', label: '线形标签页', props: { type: 'line', tabPosition: 'top', defaultActiveKey: '1' }, width: 420, height: 150 },
    { id: 'card', label: '卡片标签页', props: { type: 'card', tabPosition: 'top', defaultActiveKey: '1' }, width: 420, height: 160 },
    { id: 'left', label: '左侧标签页', props: { type: 'line', tabPosition: 'left', defaultActiveKey: '1' }, width: 460, height: 190 },
    { id: 'centered', label: '居中标签页', props: { type: 'line', tabPosition: 'top', centered: true, defaultActiveKey: '1' }, width: 420, height: 150 },
    { id: 'bottom', label: '底部标签页', props: { type: 'line', tabPosition: 'bottom', defaultActiveKey: '1' }, width: 430, height: 170 },
    { id: 'editable', label: '可新增关闭标签页', props: { type: 'editable-card', tabPosition: 'top', defaultActiveKey: '1', hideAdd: false }, width: 500, height: 180 }
  ],
  Collapse: [
    { id: 'default', label: '基础折叠面板', props: { accordion: false, bordered: true, ghost: false, defaultActiveKey: ['1'] }, width: 400, height: 170 },
    { id: 'accordion', label: '手风琴', props: { accordion: true, bordered: true, ghost: false, defaultActiveKey: ['1'] }, width: 400, height: 170 },
    { id: 'ghost', label: '幽灵折叠面板', props: { accordion: false, bordered: false, ghost: true, defaultActiveKey: ['1'] }, width: 400, height: 170 }
  ],
  Descriptions: [
    { id: 'basic', label: '基础描述', props: { bordered: false, size: 'middle', column: 2 }, width: 500, height: 160 },
    { id: 'bordered', label: '带边框描述', props: { bordered: true, size: 'middle', column: 2 }, width: 520, height: 190 },
    { id: 'compact', label: '紧凑描述', props: { bordered: true, size: 'small', column: 3 }, width: 560, height: 160 }
  ],
  Calendar: [
    { id: 'card', label: '卡片日历', props: { fullscreen: false }, width: 420, height: 320 },
    { id: 'fullscreen', label: '完整日历', props: { fullscreen: true }, width: 720, height: 520 }
  ],
  Carousel: [
    { id: 'default', label: '基础轮播', props: { autoplay: false, effect: 'scrollx', dotPosition: 'bottom' } },
    { id: 'autoplay', label: '自动轮播', props: { autoplay: true, effect: 'scrollx', dotPosition: 'bottom' } },
    { id: 'fade', label: '淡入轮播', props: { autoplay: false, effect: 'fade', dotPosition: 'bottom' } },
    { id: 'left-dots', label: '左侧指示器', props: { autoplay: false, effect: 'scrollx', dotPosition: 'left' } },
    { id: 'arrows', label: '带切换箭头', props: { autoplay: false, arrows: true, dotPosition: 'bottom' } },
    { id: 'progress', label: '进度式指示器', props: { autoplay: true, autoplaySpeed: 3000, dots: { className: 'antd-carousel-progress-dots' } } }
  ],
  Empty: [
    { id: 'default', label: '默认空状态', props: { image: 'default' }, content: '暂无数据' },
    { id: 'simple', label: '简洁空状态', props: { image: 'simple' }, content: '这里还没有内容' },
    { id: 'compact', label: '紧凑空状态', props: { image: 'simple' }, content: '无结果', width: 220, height: 120 }
  ],
  Image: [
    { id: 'basic', label: '基础图片', props: { preview: false, fallback: '' } },
    { id: 'preview', label: '可预览图片', props: { preview: true, fallback: '' } },
    { id: 'rounded', label: '圆角图片', props: { preview: true, fallback: '', style: { borderRadius: 18 } } },
    { id: 'square', label: '方形图片', props: { preview: true, fallback: '' }, width: 200, height: 200 }
  ],
  QRCode: [
    { id: 'default', label: '标准二维码', props: { size: 140, color: '#000000', bordered: true, status: 'active' } },
    { id: 'brand', label: '品牌色二维码', props: { size: 140, color: '#1677ff', bordered: true, status: 'active' } },
    { id: 'borderless', label: '无边框二维码', props: { size: 140, color: '#000000', bordered: false, status: 'active' } },
    { id: 'loading', label: '加载中二维码', props: { size: 140, color: '#000000', bordered: true, status: 'loading' } }
  ],
  Popover: [
    { id: 'top', label: '顶部气泡卡片', props: { placement: 'top', trigger: 'hover' } },
    { id: 'right', label: '右侧气泡卡片', props: { placement: 'right', trigger: 'hover' } },
    { id: 'click', label: '点击气泡卡片', props: { placement: 'bottom', trigger: 'click' } }
  ],
  Segmented: [
    { id: 'default', label: '标准分段器', props: { size: 'middle', vertical: false, block: true } },
    { id: 'large', label: '大型分段器', props: { size: 'large', vertical: false, block: true }, height: 52 },
    { id: 'vertical', label: '垂直分段器', props: { size: 'middle', vertical: true, block: false }, width: 120, height: 130 },
    { id: 'pill', label: '胶囊分段器', props: { size: 'middle', vertical: false, block: true, shape: 'round' } }
  ],
  Statistic: [
    { id: 'basic', label: '基础统计值', props: { title: '活跃用户', value: 112893, precision: 0, suffix: '人' } },
    { id: 'currency', label: '金额统计值', props: { title: '本月收入', value: 12680.5, precision: 2, prefix: '¥' } },
    { id: 'percentage', label: '百分比统计值', props: { title: '转化率', value: 28.6, precision: 1, suffix: '%' } },
    { id: 'compact', label: '紧凑统计值', props: { title: '任务', value: 24, precision: 0, suffix: '项' }, width: 160, height: 80 }
  ],
  Timeline: [
    { id: 'left', label: '左对齐时间轴', props: { mode: 'left', pending: false } },
    { id: 'alternate', label: '交替时间轴', props: { mode: 'alternate', pending: false }, width: 420 },
    { id: 'pending', label: '进行中时间轴', props: { mode: 'left', pending: '正在发布…' } }
  ],
  Tooltip: [
    { id: 'top', label: '顶部提示', props: { placement: 'top', trigger: 'hover' } },
    { id: 'right', label: '右侧提示', props: { placement: 'right', trigger: 'hover' } },
    { id: 'click', label: '点击提示', props: { placement: 'bottom', trigger: 'click' } }
  ],
  Tour: [
    { id: 'bottom', label: '底部引导', props: { placement: 'bottom', title: '功能引导', description: '了解这个组件。' } },
    { id: 'right', label: '右侧引导', props: { placement: 'right', title: '功能引导', description: '从右侧展示说明。' } },
    { id: 'maskless', label: '无蒙层引导', props: { placement: 'bottom', mask: false, title: '轻量引导', description: '不遮挡其他页面内容。' } }
  ],
  Tree: [
    { id: 'basic', label: '基础树形控件', props: { defaultExpandAll: true, checkable: false, showLine: false } },
    { id: 'checkable', label: '可勾选树', props: { defaultExpandAll: true, checkable: true, showLine: false } },
    { id: 'lines', label: '连接线树', props: { defaultExpandAll: true, checkable: false, showLine: true } }
  ],
  Drawer: [
    { id: 'right', label: '右侧抽屉', props: { placement: 'right', title: '详情面板', width: 420 } },
    { id: 'left', label: '左侧抽屉', props: { placement: 'left', title: '导航面板', width: 360 } },
    { id: 'top', label: '顶部抽屉', props: { placement: 'top', title: '通知中心', height: 300 } },
    { id: 'bottom', label: '底部抽屉', props: { placement: 'bottom', title: '快捷操作', height: 300 } },
    { id: 'resizable', label: '可调整大小抽屉', props: { placement: 'right', title: '可调整详情面板', width: 420, resizable: true } },
    { id: 'loading', label: '加载中抽屉', props: { placement: 'right', title: '正在加载', width: 420, loading: true } },
    { id: 'maskless', label: '无遮罩抽屉', props: { placement: 'right', title: '辅助面板', width: 360, mask: false } }
  ],
  Modal: [
    { id: 'default', label: '基础对话框', props: { title: '确认操作', width: 520 } },
    { id: 'compact', label: '紧凑对话框', props: { title: '快速确认', width: 400 } },
    { id: 'wide', label: '宽幅对话框', props: { title: '产品详情', width: 760 } },
    { id: 'loading', label: '加载中对话框', props: { title: '正在处理', width: 520, loading: true } },
    { id: 'maskless', label: '无遮罩对话框', props: { title: '轻量对话框', width: 520, mask: false } }
  ],
  Message: [
    { id: 'info', label: '信息提示', props: { type: 'info', content: '操作已完成' }, content: '显示信息' },
    { id: 'success', label: '成功提示', props: { type: 'success', content: '保存成功' }, content: '显示成功' },
    { id: 'warning', label: '警告提示', props: { type: 'warning', content: '请检查输入内容' }, content: '显示警告' },
    { id: 'error', label: '错误提示', props: { type: 'error', content: '操作失败' }, content: '显示错误' }
  ],
  Popconfirm: [
    { id: 'top', label: '顶部确认框', props: { placement: 'top', title: '确定执行吗？', okText: '确定', cancelText: '取消' } },
    { id: 'bottom', label: '底部确认框', props: { placement: 'bottom', title: '确定执行吗？', okText: '确定', cancelText: '取消' } },
    { id: 'danger', label: '危险操作确认', props: { placement: 'top', title: '此操作不可撤销，确定吗？', okText: '删除', cancelText: '取消', okButtonProps: { danger: true } } }
  ],
  Notification: [
    { id: 'top-right', label: '右上通知', props: { placement: 'topRight', type: 'info', message: '设计已更新', description: '操作已经完成。' } },
    { id: 'bottom-right', label: '右下通知', props: { placement: 'bottomRight', type: 'success', message: '保存成功', description: '最新修改已经保存。' } },
    { id: 'top-left', label: '左上通知', props: { placement: 'topLeft', type: 'warning', message: '需要注意', description: '还有未处理的设计批注。' } }
  ],
  Result: ['success', 'info', 'warning', 'error'].map((status) => ({ id: status, label: `${status} 结果`, props: { status, title: status === 'success' ? '操作成功' : status === 'error' ? '操作失败' : status === 'warning' ? '需要注意' : '处理完成', subTitle: '这是结果页面的补充说明。' } })),
  Skeleton: [
    { id: 'basic', label: '基础骨架屏', props: { active: false, avatar: false, paragraph: { rows: 3 } } },
    { id: 'active', label: '动态骨架屏', props: { active: true, avatar: false, paragraph: { rows: 3 } } },
    { id: 'avatar', label: '头像骨架屏', props: { active: true, avatar: true, paragraph: { rows: 2 } } }
  ],
  Spin: [
    { id: 'small', label: '小型加载', props: { size: 'small' }, width: 140, height: 80 },
    { id: 'default', label: '标准加载', props: { size: 'default' }, width: 160, height: 90 },
    { id: 'large', label: '大型加载', props: { size: 'large' }, width: 180, height: 100 }
  ],
  Affix: [
    { id: 'top', label: '顶部固钉', props: { offsetTop: 20 }, content: '固定在顶部' },
    { id: 'bottom', label: '底部固钉', props: { offsetBottom: 20 }, content: '固定在底部' }
  ],
  Watermark: [
    { id: 'text', label: '文字水印', props: { content: 'Web Design Studio', rotate: -22, gap: [100, 100] } },
    { id: 'brand', label: '品牌水印', props: { content: 'AI DESIGN', rotate: -18, gap: [90, 90], font: { color: 'rgba(22,119,255,.16)', fontSize: 16 } } },
    { id: 'dense', label: '密集水印', props: { content: 'CONFIDENTIAL', rotate: -25, gap: [60, 60], font: { color: 'rgba(0,0,0,.12)', fontSize: 13 } } }
  ],
  App: [
    { id: 'default', label: '应用上下文容器', props: { messageMaxCount: 3, notificationPlacement: 'topRight' }, width: 420, height: 180 },
    { id: 'limited', label: '单条消息上下文', props: { messageMaxCount: 1, notificationPlacement: 'bottomLeft' }, width: 420, height: 180 }
  ],
  ConfigProvider: [
    { id: 'default', label: '默认全局配置', props: { direction: 'ltr', componentSize: 'middle', componentDisabled: false }, width: 420, height: 180 },
    { id: 'rtl', label: 'RTL 从右到左', props: { direction: 'rtl', componentSize: 'middle', componentDisabled: false }, width: 420, height: 180 },
    { id: 'large', label: '大型组件配置', props: { direction: 'ltr', componentSize: 'large', componentDisabled: false }, width: 460, height: 190 },
    { id: 'disabled', label: '全局禁用配置', props: { direction: 'ltr', componentSize: 'middle', componentDisabled: true }, width: 420, height: 180 }
  ],
  BorderBeam: [
    { id: 'basic', label: '基础边框流光', props: { count: 1, duration: 6, size: 80, lineWidth: 2, color: '#1677ff' }, width: 380, height: 190 },
    { id: 'multiple', label: '多条边框流光', props: { count: 3, duration: 7, size: 72, lineWidth: 2, color: '#1677ff' }, width: 380, height: 190 },
    { id: 'aurora', label: '极光渐变流光', props: { count: 2, duration: 6, size: 92, lineWidth: 2, color: [{ color: '#7c3aed', percent: 0 }, { color: '#06b6d4', percent: 57 }, { color: '#67e8f9', percent: 100 }] }, width: 400, height: 200 },
    { id: 'slow', label: '慢速细线流光', props: { count: 1, duration: 12, size: 110, lineWidth: 1, color: '#722ed1' }, width: 380, height: 190 },
    { id: 'bold', label: '高亮粗线流光', props: { count: 2, duration: 5, size: 90, lineWidth: 4, color: '#ff4d4f' }, width: 400, height: 200 }
  ],
  Alert: ['info', 'success', 'warning', 'error'].map((type) => ({ id: type, label: `${type} 提示`, props: { type, showIcon: true } })),
  Progress: [
    { id: 'line', label: '线形进度', props: { type: 'line', percent: 68 }, width: 320, height: 70 },
    { id: 'circle', label: '圆形进度', props: { type: 'circle', percent: 68 }, width: 140, height: 140 },
    { id: 'dashboard', label: '仪表盘进度', props: { type: 'dashboard', percent: 68 }, width: 140, height: 140 }
  ],
  Tag: [
    { id: 'blue', label: '蓝色标签', props: { color: 'blue' }, content: '已发布' },
    { id: 'success', label: '成功标签', props: { color: 'success' }, content: '成功' },
    { id: 'warning', label: '警告标签', props: { color: 'warning' }, content: '警告' },
    { id: 'error', label: '错误标签', props: { color: 'error' }, content: '错误' }
  ],
  Avatar: [
    { id: 'circle', label: '圆形头像', props: { shape: 'circle', size: 48 } },
    { id: 'square', label: '方形头像', props: { shape: 'square', size: 48 } }
  ],
  Badge: [
    { id: 'count', label: '数字徽标', props: { count: 8 } },
    { id: 'dot', label: '状态点', props: { dot: true, count: 0 } },
    { id: 'status', label: '状态徽标', props: { status: 'success', text: '运行中', count: 0 } }
  ]
};

export function variantsForAntdComponent(componentId: string): AntdComponentVariant[] {
  return variantsForUiComponent(ANTD_LIBRARY, componentId);
}

export const ANTD_COMPONENTS: AntdComponentDefinition[] = [
  item('Button', '按钮', '通用', '▣', 'button', 150, 48, '主要按钮', { color: 'primary', variant: 'solid', size: 'middle' }),
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
  item('Menu', '导航菜单', '导航', '☰', 'list', 420, 48, '', { mode: 'horizontal', defaultSelectedKeys: ['home'], items: [{ key: 'home', label: '首页' }, { key: 'product', label: '产品' }, { key: 'about', label: '关于' }] }),
  item('Pagination', '分页', '导航', '•••', 'list', 360, 48, '', { defaultCurrent: 1, total: 80, showSizeChanger: false }),
  item('Steps', '步骤条', '导航', '①', 'list', 500, 74, '', { current: 1, items: [{ title: '创建' }, { title: '配置' }, { title: '完成' }] }),
  item('Tabs', '标签页', '导航', '▤', 'card', 420, 150, '', { defaultActiveKey: '1', items: [{ key: '1', label: '概览', children: '产品概览内容' }, { key: '2', label: '功能', children: '功能说明内容' }, { key: '3', label: '设置', children: '设置内容' }] }),

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
  item('List', '列表（已废弃）', '数据展示', '☷', 'list', 360, 190, '', { bordered: true, dataSource: ['可视化编辑', '响应式页面', 'AI 协作修改'] }, ['deprecated', '废弃', '旧列表']),
  item('Listy', '虚拟列表', '数据展示', '☷', 'list', 420, 280, '', { height: 260, virtual: false, itemCount: 20 }, ['virtual list', '虚拟滚动', '长列表']),
  item('Popover', '气泡卡片', '数据展示', '▢', 'button', 160, 44, '查看详情', { title: '产品信息', content: '这是一个气泡卡片。' }),
  item('QRCode', '二维码', '数据展示', '▦', 'image', 170, 170, '', { value: 'https://ant.design', size: 140 }),
  item('Segmented', '分段控制器', '数据展示', '▥', 'button', 300, 44, '', { defaultValue: '日', options: ['日', '周', '月'] }),
  item('Statistic', '统计数值', '数据展示', '#', 'card', 220, 100, '', { title: '活跃用户', value: 112893, suffix: '人' }),
  item('Table', '表格', '数据展示', '▦', 'table', 520, 220, '', { pagination: false, size: 'small', columns: [{ title: '名称', dataIndex: 'name', key: 'name' }, { title: '状态', dataIndex: 'status', key: 'status' }, { title: '数量', dataIndex: 'count', key: 'count' }], dataSource: [{ key: '1', name: '网站组件', status: '可用', count: 20 }, { key: '2', name: 'Ant Design', status: '新增', count: 60 }] }),
  item('Tag', '标签', '数据展示', '◆', 'badge', 100, 36, '已发布', { color: 'blue' }),
  item('Timeline', '时间轴', '数据展示', '│', 'list', 300, 180, '', { items: [{ children: '创建项目' }, { children: '完成设计', color: 'blue' }, { children: '准备发布', color: 'gray' }] }),
  item('Tooltip', '文字提示', '数据展示', '?', 'button', 150, 44, '悬停查看', { title: '这是提示文字' }),
  item('Tour', '漫游式引导', '数据展示', '◎', 'button', 150, 44, '开始引导', { title: '功能引导', description: '这是 Ant Design Tour 的引导步骤。' }),
  item('Tree', '树形控件', '数据展示', '⌁', 'list', 300, 180, '', { defaultExpandAll: true, treeData: [{ title: '页面', key: '0', children: [{ title: '首页', key: '0-0' }, { title: '关于', key: '0-1' }] }] }),

  item('Alert', '警告提示', '反馈', '!', 'card', 380, 74, '', { type: 'info', message: '设计已自动保存', showIcon: true }),
  item('Modal', '对话框', '反馈', '▣', 'button', 160, 44, '打开对话框', { title: '确认操作' }),
  item('Drawer', '抽屉', '反馈', '▤', 'button', 150, 44, '打开抽屉', { title: '详情面板' }),
  item('Message', '全局提示', '反馈', '●', 'button', 150, 44, '显示提示', { content: '操作已完成' }),
  item('Popconfirm', '气泡确认框', '反馈', '?', 'button', 160, 44, '删除项目', { title: '确定删除吗？' }),
  item('Notification', '通知提醒框', '反馈', '▢', 'button', 160, 44, '显示通知', { message: '设计已更新', description: 'Ant Design 组件已经应用到画布。' }),
  item('Progress', '进度条', '反馈', '◔', 'card', 320, 70, '', { percent: 68 }),
  item('Result', '结果', '反馈', '✓', 'card', 380, 240, '', { status: 'success', title: '发布成功', subTitle: '页面已经成功发布。' }),
  item('Skeleton', '骨架屏', '反馈', '▧', 'card', 360, 150, '', { active: true, paragraph: { rows: 3 } }),
  item('Spin', '加载中', '反馈', '◌', 'card', 180, 100, '加载中…', { size: 'large' }),
  item('Watermark', '水印', '反馈', 'W', 'card', 360, 180, '', { content: 'Web Design Studio' }),

  item('Affix', '固钉', '其他', '⌖', 'button', 150, 44, '固定操作', { offsetTop: 20 }),
  item('App', '包裹组件', '其他', 'A', 'section', 420, 180, '', { messageMaxCount: 3, notificationPlacement: 'topRight' }),
  item('BorderBeam', '边框流光', '其他', '◈', 'section', 380, 190, '', { count: 1, duration: 6, size: 80, lineWidth: 2, color: '#1677ff' }),
  item('ConfigProvider', '全局化配置', '其他', '⚙', 'section', 420, 180, '', { direction: 'ltr', componentSize: 'middle', componentDisabled: false })
];

export function createAntdComponent(definitionId: string, x: number, y: number): WebDesignComponent {
  return createUiLibraryComponent(ANTD_LIBRARY, definitionId, x, y);
}

export function applyAntdComponentVariant(component: WebDesignComponent, variantId: string): WebDesignComponent {
  return applyUiComponentVariant(ANTD_LIBRARY, component, variantId);
}

export const ANTD_LIBRARY: UiLibraryCatalog<AntdCategory> = {
  id: 'antd',
  displayName: 'Ant Design',
  shortName: 'AntD',
  version: ANTD_VERSION,
  brandMark: 'A',
  categories: ANTD_CATEGORIES,
  components: ANTD_COMPONENTS,
  variants: ANTD_COMPONENT_VARIANTS
};
