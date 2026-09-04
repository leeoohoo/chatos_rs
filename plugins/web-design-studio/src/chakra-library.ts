import type { WebDesignComponent, WebDesignJsonValue } from './schema.js';
import { applyUiComponentVariant, createUiLibraryComponent, defineUiComponent, variantsForUiComponent, type UiComponentDefinition, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';

export type ChakraCategory = '布局' | '排版' | '按钮' | '数据录入' | '导航' | '数据展示' | '反馈' | '浮层' | '国际化' | '工具';
export type ChakraComponentDefinition = UiComponentDefinition<ChakraCategory>;
export type ChakraComponentVariant = UiComponentVariant;

export const CHAKRA_VERSION = '3.37.0';
export const CHAKRA_CATEGORIES: ChakraCategory[] = ['布局', '排版', '按钮', '数据录入', '导航', '数据展示', '反馈', '浮层', '国际化', '工具'];
const item = defineUiComponent<ChakraCategory>;
const variant = (
  id: string,
  label: string,
  props: Record<string, WebDesignJsonValue>,
  options: Omit<ChakraComponentVariant, 'id' | 'label' | 'props'> = {}
): ChakraComponentVariant => ({ id, label, props, ...options });

const listItems = [
  { key: 'composition', label: '组合式组件' },
  { key: 'tokens', label: '主题令牌' },
  { key: 'accessibility', label: '无障碍交互' }
];

const listDetailItems = [
  { key: 'design', label: '设计系统', description: '统一颜色、字体和间距令牌。' },
  { key: 'compose', label: '组合能力', description: '用可组合部件构建复杂界面。' },
  { key: 'accessibility', label: '无障碍体验', description: '默认提供清晰的语义结构。' }
];

export const CHAKRA_COMPONENT_VARIANTS: Record<string, ChakraComponentVariant[]> = {
  AspectRatio: [
    variant('video', '16:9 视频比例', { ratio: 16 / 9, kind: 'video', background: 'gray.900' }, { width: 480, height: 270 }),
    variant('photo', '4:3 图片比例', { ratio: 4 / 3, kind: 'photo', background: 'blue.50' }, { width: 400, height: 300 }),
    variant('square', '1:1 方形比例', { ratio: 1, kind: 'square', background: 'purple.50' }, { width: 280, height: 280 }),
    variant('cinema', '21:9 电影比例', { ratio: 21 / 9, kind: 'video', background: 'gray.900' }, { width: 520, height: 223 })
  ],
  Bleed: [
    variant('inline', '横向溢出', { inline: 6, block: 0, edge: 'inline', colorPalette: 'blue' }),
    variant('block', '纵向溢出', { inline: 0, block: 6, edge: 'block', colorPalette: 'purple' }),
    variant('start', '起始边溢出', { inlineStart: 6, inlineEnd: 0, block: 0, edge: 'start', colorPalette: 'teal' }),
    variant('all', '四边溢出', { inline: 6, block: 6, edge: 'all', colorPalette: 'orange' })
  ],
  AbsoluteCenter: [
    variant('both', '水平垂直居中', { axis: 'both', colorPalette: 'blue' }),
    variant('horizontal', '仅水平居中', { axis: 'horizontal', colorPalette: 'purple' }),
    variant('vertical', '仅垂直居中', { axis: 'vertical', colorPalette: 'green' })
  ],
  Center: [
    variant('box', '容器居中', { inline: false, colorPalette: 'blue' }),
    variant('inline', '行内居中', { inline: true, colorPalette: 'purple' }, { width: 320, height: 72 }),
    variant('avatar', '头像居中', { inline: false, colorPalette: 'teal', contentKind: 'avatar' }, { width: 200, height: 160 })
  ],
  Float: [
    variant('top-start', '左上角浮标', { placement: 'top-start', offset: 3, colorPalette: 'red' }),
    variant('top-end', '右上角浮标', { placement: 'top-end', offset: 3, colorPalette: 'red' }),
    variant('bottom-start', '左下角浮标', { placement: 'bottom-start', offset: 3, colorPalette: 'blue' }),
    variant('bottom-end', '右下角浮标', { placement: 'bottom-end', offset: 3, colorPalette: 'blue' }),
    variant('middle-start', '左侧居中浮标', { placement: 'middle-start', offset: 3, colorPalette: 'purple' }),
    variant('middle-end', '右侧居中浮标', { placement: 'middle-end', offset: 3, colorPalette: 'purple' })
  ],
  Wrap: [
    variant('start', '起始换行', { justify: 'start', align: 'center', direction: 'row', gap: 3, itemCount: 8 }),
    variant('center', '居中换行', { justify: 'center', align: 'center', direction: 'row', gap: 3, itemCount: 8 }),
    variant('between', '两端分布换行', { justify: 'space-between', align: 'center', direction: 'row', gap: 3, itemCount: 7 }),
    variant('column', '纵向换列', { justify: 'start', align: 'stretch', direction: 'column', gap: 2, itemCount: 5 }, { width: 260, height: 220 })
  ],
  Box: [
    variant('outline', '描边容器', { padding: 4, borderWidth: 1, borderRadius: 'lg', background: 'white' }),
    variant('subtle', '柔和背景容器', { padding: 5, borderWidth: 0, borderRadius: 'xl', background: 'blue.50' }),
    variant('elevated', '悬浮容器', { padding: 5, borderWidth: 0, borderRadius: 'xl', background: 'white', shadow: 'md' })
  ],
  Container: [
    variant('narrow', '窄内容居中', { maxWidth: 'md', centerContent: true, fluid: false }),
    variant('wide', '宽内容容器', { maxWidth: '4xl', centerContent: false, fluid: false }),
    variant('fluid', '流式全宽容器', { maxWidth: 'full', centerContent: false, fluid: true })
  ],
  Flex: [
    variant('row', '水平排列', { direction: 'row', gap: 4, align: 'center', justify: 'start', wrap: 'nowrap' }),
    variant('between', '两端对齐', { direction: 'row', gap: 4, align: 'center', justify: 'space-between', wrap: 'nowrap' }),
    variant('column', '垂直排列', { direction: 'column', gap: 3, align: 'stretch', justify: 'start', wrap: 'nowrap' }, { height: 180 }),
    variant('wrap', '自动换行', { direction: 'row', gap: 3, align: 'center', justify: 'start', wrap: 'wrap' }, { height: 150 })
  ],
  Grid: [
    variant('two-columns', '两列栅格', { columns: 2, gap: 4 }),
    variant('three-columns', '三列栅格', { columns: 3, gap: 4 }),
    variant('four-columns', '四列紧凑栅格', { columns: 4, gap: 2 })
  ],
  SimpleGrid: [
    variant('two-columns', '两列等分', { columns: 2, gap: 4 }),
    variant('three-columns', '三列等分', { columns: 3, gap: 4 }),
    variant('four-columns', '四列等分', { columns: 4, gap: 3 })
  ],
  Stack: [
    variant('vertical', '垂直堆叠', { direction: 'column', gap: 4, align: 'stretch' }),
    variant('horizontal', '水平堆叠', { direction: 'row', gap: 3, align: 'center' }, { height: 90 }),
    variant('centered', '居中堆叠', { direction: 'column', gap: 3, align: 'center' })
  ],
  Group: [
    variant('spaced', '间隔组合', { attached: false, orientation: 'horizontal' }),
    variant('attached', '紧贴组合', { attached: true, orientation: 'horizontal' }),
    variant('vertical', '垂直组合', { attached: false, orientation: 'vertical' }, { width: 180, height: 130 })
  ],
  Separator: [
    variant('solid', '实线分隔', { orientation: 'horizontal', variant: 'solid', size: 'sm' }),
    variant('dashed', '虚线分隔', { orientation: 'horizontal', variant: 'dashed', size: 'sm' }),
    variant('dotted', '点线分隔', { orientation: 'horizontal', variant: 'dotted', size: 'md' }),
    variant('vertical', '垂直分隔', { orientation: 'vertical', variant: 'solid', size: 'sm' }, { width: 32, height: 150 })
  ],
  ScrollArea: [
    variant('hover', '悬停滚动条', { variant: 'hover', size: 'md', itemCount: 9 }),
    variant('always', '始终显示滚动条', { variant: 'always', size: 'md', itemCount: 9 }),
    variant('compact', '紧凑滚动区域', { variant: 'always', size: 'xs', itemCount: 12 }, { width: 300, height: 180 })
  ],
  Splitter: [
    variant('horizontal', '水平分隔面板', { orientation: 'horizontal', defaultSizes: [45, 55] }),
    variant('balanced', '水平等分面板', { orientation: 'horizontal', defaultSizes: [50, 50] }),
    variant('vertical', '垂直分隔面板', { orientation: 'vertical', defaultSizes: [45, 55] }, { height: 300 })
  ],
  Heading: [
    variant('hero', '超大主标题', { size: '4xl', level: 1 }, { content: '构建漂亮的网站', height: 84 }),
    variant('section', '章节标题', { size: '2xl', level: 2 }, { content: '产品设计能力' }),
    variant('card', '卡片标题', { size: 'lg', level: 3 }, { content: '核心功能', height: 48 }),
    variant('caption', '小型标题', { size: 'sm', level: 4 }, { content: '更多信息', height: 40 })
  ],
  Text: [
    variant('body', '正文', { textStyle: 'md', color: 'fg' }),
    variant('large', '大号正文', { textStyle: 'lg', color: 'fg' }),
    variant('small', '辅助文字', { textStyle: 'sm', color: 'fg.muted' }),
    variant('caption', '说明文字', { textStyle: 'xs', color: 'fg.subtle' })
  ],
  Code: ['solid', 'subtle', 'outline', 'surface', 'plain'].map((value) => variant(value, `${value} 代码`, { variant: value, colorPalette: 'gray', size: 'sm' })),
  CodeBlock: [
    variant('basic', '基础代码块', { language: 'tsx', title: 'app.tsx', showHeader: false, showLineNumbers: false, wordWrap: false, maxLines: 0 }, { width: 500, height: 220 }),
    variant('header', '带标题与复制', { language: 'tsx', title: 'app.tsx', showHeader: true, showLineNumbers: false, wordWrap: false, maxLines: 0 }, { width: 520, height: 250 }),
    variant('numbers', '显示行号', { language: 'tsx', title: 'component.tsx', showHeader: true, showLineNumbers: true, wordWrap: false, maxLines: 0 }, { width: 540, height: 260 }),
    variant('wrapped', '自动换行', { language: 'css', title: 'styles.css', showHeader: true, showLineNumbers: true, wordWrap: true, maxLines: 6 }, { width: 480, height: 250 })
  ],
  Em: [
    variant('default', '默认强调', { color: 'fg' }),
    variant('brand', '品牌色强调', { color: 'blue.600' }),
    variant('muted', '柔和强调', { color: 'fg.muted' })
  ],
  Highlight: [
    variant('single', '单关键词高亮', { query: ['AI'], colorPalette: 'yellow', ignoreCase: true }),
    variant('multiple', '多关键词高亮', { query: ['设计', '网站'], colorPalette: 'blue', ignoreCase: true }),
    variant('success', '成功色高亮', { query: ['漂亮', '可用'], colorPalette: 'green', ignoreCase: true }),
    variant('underline', '下划线高亮', { query: ['协作'], colorPalette: 'purple', ignoreCase: true, treatment: 'underline' })
  ],
  LinkOverlay: [
    variant('card', '整卡可点击', { href: '#product', external: false, variant: 'card' }, { width: 380, height: 150 }),
    variant('article', '文章卡片链接', { href: '#article', external: false, variant: 'article' }, { width: 420, height: 180 }),
    variant('external', '外部卡片链接', { href: 'https://chakra-ui.com', external: true, variant: 'external' }, { width: 380, height: 150 })
  ],
  Mark: [
    variant('yellow', '黄色标记', { colorPalette: 'yellow', variant: 'subtle' }),
    variant('blue', '蓝色标记', { colorPalette: 'blue', variant: 'subtle' }),
    variant('solid', '实色标记', { colorPalette: 'purple', variant: 'solid' }),
    variant('outline', '描边标记', { colorPalette: 'orange', variant: 'outline' })
  ],
  Prose: [
    variant('medium', '标准文章排版', { size: 'md', maxWidth: '65ch', showTable: false }, { width: 560, height: 360 }),
    variant('large', '大号文章排版', { size: 'lg', maxWidth: '70ch', showTable: false }, { width: 620, height: 400 }),
    variant('complete', '完整富内容排版', { size: 'md', maxWidth: '72ch', showTable: true }, { width: 680, height: 500 })
  ],
  RichTextEditor: [
    variant('basic', '基础富文本编辑器', { toolbar: ['bold', 'italic', 'strike', 'code'], editable: true, showFooter: false, placeholder: '开始输入内容…' }, { width: 620, height: 300 }),
    variant('headings', '带标题工具栏', { toolbar: ['bold', 'italic', 'strike', 'code', 'h1', 'h2', 'h3'], editable: true, showFooter: false, placeholder: '编写页面正文…' }, { width: 680, height: 340 }),
    variant('document', '文档编辑器', { toolbar: ['bold', 'italic', 'strike', 'code', 'h1', 'h2', 'bullet', 'ordered', 'quote', 'undo', 'redo'], editable: true, showFooter: true, placeholder: '开始撰写文档…' }, { width: 760, height: 440 }),
    variant('readonly', '只读富文本', { toolbar: [], editable: false, showFooter: true, placeholder: '' }, { width: 620, height: 280 })
  ],
  Blockquote: [
    variant('subtle', '柔和引用', { variant: 'subtle', justify: 'start', colorPalette: 'blue', cite: 'Web Design Studio' }),
    variant('solid', '实色引用', { variant: 'solid', justify: 'start', colorPalette: 'purple', cite: '设计团队' }),
    variant('plain', '简洁居中引用', { variant: 'plain', justify: 'center', colorPalette: 'gray', cite: '产品负责人' })
  ],
  Kbd: ['raised', 'outline', 'subtle', 'plain'].map((value) => variant(value, `${value} 按键`, { variant: value, size: 'md' })),
  Link: [
    variant('underline', '下划线链接', { variant: 'underline', colorPalette: 'blue', external: false }),
    variant('plain', '简洁链接', { variant: 'plain', colorPalette: 'blue', external: false }),
    variant('external', '外部链接', { variant: 'underline', colorPalette: 'purple', external: true }, { content: '打开外部文档' })
  ],
  List: [
    {
      id: 'basic',
      label: '基础无序列表',
      props: { items: listItems, ordered: false, variant: 'marker', align: 'start', indicator: 'none', gap: 2, unstyled: false },
      width: 360,
      height: 150
    },
    {
      id: 'ordered',
      label: '有序列表',
      props: { items: listItems, ordered: true, variant: 'marker', align: 'start', indicator: 'none', gap: 2, unstyled: false },
      width: 360,
      height: 160
    },
    {
      id: 'icon-check',
      label: '勾选图标列表',
      props: { items: listItems, ordered: false, variant: 'plain', align: 'center', indicator: 'check', indicatorColor: 'green.500', gap: 3, unstyled: false },
      width: 380,
      height: 170
    },
    {
      id: 'icon-info',
      label: '信息图标与说明',
      props: { items: listDetailItems, ordered: false, variant: 'plain', align: 'start', indicator: 'info', indicatorColor: 'blue.500', gap: 4, unstyled: false },
      width: 430,
      height: 230
    },
    {
      id: 'nested',
      label: '嵌套列表',
      props: {
        items: [
          { key: 'foundation', label: '基础能力', children: [{ key: 'tokens', label: '主题令牌' }, { key: 'responsive', label: '响应式样式' }] },
          { key: 'components', label: '组合组件', children: [{ key: 'forms', label: '表单组件' }, { key: 'overlay', label: '浮层组件' }] },
          { key: 'delivery', label: '产品交付' }
        ],
        ordered: false,
        nestedOrdered: false,
        variant: 'marker',
        align: 'start',
        indicator: 'none',
        gap: 2,
        unstyled: false
      },
      width: 390,
      height: 260
    },
    {
      id: 'custom-marker',
      label: '自定义 Marker',
      props: { items: listItems, ordered: false, variant: 'marker', align: 'start', indicator: 'none', markerColor: 'purple.500', markerContent: '→', gap: 3, unstyled: false },
      width: 380,
      height: 180
    },
    {
      id: 'plain',
      label: '无样式列表',
      props: { items: listItems, ordered: false, variant: 'plain', align: 'start', indicator: 'none', gap: 3, unstyled: true },
      width: 360,
      height: 160
    },
    {
      id: 'align-end',
      label: '图标底部对齐',
      props: { items: listDetailItems, ordered: false, variant: 'plain', align: 'end', indicator: 'circle', indicatorColor: 'orange.500', gap: 4, unstyled: false },
      width: 430,
      height: 230
    }
  ],
  IconButton: [
    variant('solid', '实心图标按钮', { variant: 'solid', colorPalette: 'blue', size: 'md' }),
    variant('subtle', '柔和图标按钮', { variant: 'subtle', colorPalette: 'blue', size: 'md' }),
    variant('surface', '表面图标按钮', { variant: 'surface', colorPalette: 'gray', size: 'md' }),
    variant('outline', '描边图标按钮', { variant: 'outline', colorPalette: 'gray', size: 'md' }),
    variant('ghost', '幽灵图标按钮', { variant: 'ghost', colorPalette: 'gray', size: 'md' }),
    variant('plain', '简洁图标按钮', { variant: 'plain', colorPalette: 'blue', size: 'md' })
  ],
  CloseButton: [
    variant('small', '小号关闭按钮', { size: 'sm', variant: 'ghost', colorPalette: 'gray' }, { width: 36, height: 36 }),
    variant('medium', '标准关闭按钮', { size: 'md', variant: 'ghost', colorPalette: 'gray' }, { width: 44, height: 44 }),
    variant('outline', '描边关闭按钮', { size: 'md', variant: 'outline', colorPalette: 'gray' }, { width: 44, height: 44 }),
    variant('danger', '危险色关闭按钮', { size: 'md', variant: 'subtle', colorPalette: 'red' }, { width: 44, height: 44 })
  ],
  DownloadTrigger: [
    variant('text', '下载文本文件', { fileName: 'design-notes.txt', mimeType: 'text/plain', data: '由 Web Design Studio 生成的设计说明。', variant: 'solid', colorPalette: 'blue' }, { width: 170, height: 44 }),
    variant('json', '下载 JSON 数据', { fileName: 'design.json', mimeType: 'application/json', data: '{\n  "project": "Web Design Studio"\n}', variant: 'outline', colorPalette: 'purple' }, { width: 180, height: 44 }),
    variant('csv', '下载 CSV 数据', { fileName: 'components.csv', mimeType: 'text/csv', data: 'component,status\\nButton,ready\\nInput,ready', variant: 'subtle', colorPalette: 'green' }, { width: 170, height: 44 })
  ],
  DateInput: [
    variant('medium', '标准日期输入', { label: '出生日期', size: 'md', locale: 'zh-CN', granularity: 'day', defaultValue: '1994-06-18' }),
    variant('small', '紧凑日期输入', { label: '选择日期', size: 'sm', locale: 'zh-CN', granularity: 'day', defaultValue: '2026-09-04' }, { width: 260, height: 70 }),
    variant('range', '日期范围输入', { label: '请假日期', size: 'md', locale: 'zh-CN', granularity: 'day', selectionMode: 'range', defaultValue: ['2026-09-08', '2026-09-12'] }, { width: 420, height: 96 }),
    variant('time', '日期时间输入', { label: '发布时间', size: 'md', locale: 'zh-CN', granularity: 'minute', defaultValue: '2026-09-04T14:30' }, { width: 420, height: 78 })
  ],
  DatePicker: [
    variant('single', '单日期选择', { label: '交付日期', size: 'md', variant: 'outline', colorPalette: 'purple', selectionMode: 'single', locale: 'zh-CN', closeOnSelect: true, defaultValue: ['2026-09-11'] }),
    variant('range', '日期范围选择', { label: '项目周期', size: 'md', variant: 'outline', colorPalette: 'blue', selectionMode: 'range', locale: 'zh-CN', closeOnSelect: true, defaultValue: ['2026-09-11', '2026-09-20'] }, { width: 420, height: 82 }),
    variant('multiple', '多日期选择', { label: '可用日期', size: 'md', variant: 'subtle', colorPalette: 'teal', selectionMode: 'multiple', locale: 'zh-CN', closeOnSelect: false, maxSelectedDates: 6, defaultValue: ['2026-09-03', '2026-09-11', '2026-09-21'] }, { width: 420, height: 118 }),
    variant('large', '大号日期选择', { label: '活动日期', size: 'lg', variant: 'outline', colorPalette: 'orange', selectionMode: 'single', locale: 'zh-CN', closeOnSelect: true, defaultValue: ['2026-09-13'] }, { width: 400, height: 94 })
  ],
  Calendar: [
    variant('single', '单选日历', { size: 'md', selectionMode: 'single', locale: 'zh-CN', hideOutsideDays: false, showWeekNumbers: false }, { width: 360, height: 360 }),
    variant('range', '范围日历', { size: 'md', selectionMode: 'range', locale: 'zh-CN', hideOutsideDays: false, showWeekNumbers: false }, { width: 360, height: 360 }),
    variant('multiple', '多选日历', { size: 'md', selectionMode: 'multiple', locale: 'zh-CN', hideOutsideDays: false, showWeekNumbers: false }, { width: 360, height: 360 }),
    variant('compact', '紧凑日历', { size: 'sm', selectionMode: 'single', locale: 'zh-CN', hideOutsideDays: true, showWeekNumbers: false }, { width: 320, height: 320 }),
    variant('week', '显示周序号', { size: 'md', selectionMode: 'single', locale: 'zh-CN', hideOutsideDays: false, showWeekNumbers: true }, { width: 400, height: 360 })
  ],
  CheckboxCard: [
    variant('outline', '描边选择卡', { variant: 'outline', colorPalette: 'blue', size: 'md', defaultChecked: false, title: '专业版', description: '适合完整产品设计。' }),
    variant('subtle', '柔和选择卡', { variant: 'subtle', colorPalette: 'purple', size: 'md', defaultChecked: true, title: '团队版', description: '包含协作和共享能力。' }),
    variant('surface', '表面选择卡', { variant: 'surface', colorPalette: 'green', size: 'md', defaultChecked: false, title: '企业版', description: '提供高级安全与治理。' }),
    variant('compact', '紧凑选择卡', { variant: 'outline', colorPalette: 'gray', size: 'sm', defaultChecked: true, title: '启用 AI', description: '让 AI 参与设计。' }, { width: 280, height: 100 })
  ],
  ColorPicker: [
    variant('hex', 'HEX 颜色选择', { label: '品牌主色', defaultValue: '#5D50DF', format: 'rgba', size: 'md', showAlpha: false }),
    variant('alpha', '透明度颜色选择', { label: '浮层颜色', defaultValue: '#2563EBCC', format: 'rgba', size: 'md', showAlpha: true }),
    variant('small', '紧凑颜色选择', { label: '强调色', defaultValue: '#22C55E', format: 'rgba', size: 'sm', showAlpha: false }, { width: 240, height: 72 }),
    variant('large', '大号颜色选择', { label: '页面背景', defaultValue: '#F5F5F7', format: 'rgba', size: 'lg', showAlpha: false }, { width: 340, height: 90 })
  ],
  ColorSwatch: [
    variant('purple', '品牌紫色板', { value: '#5D50DF', size: 'lg', shape: 'rounded', showCheck: false }),
    variant('green', '成功绿色板', { value: '#22C55E', size: 'lg', shape: 'rounded', showCheck: true }),
    variant('circle', '圆形色板', { value: '#2563EB', size: 'xl', shape: 'full', showCheck: false }, { width: 56, height: 56 }),
    variant('transparent', '透明色板', { value: 'transparent', size: 'xl', shape: 'rounded', showCheck: false }, { width: 56, height: 56 })
  ],
  Field: [
    variant('basic', '基础字段', { label: '电子邮箱', helperText: '', errorText: '', required: false, invalid: false, disabled: false }),
    variant('helper', '带帮助说明', { label: '网站地址', helperText: '请输入完整的 https 地址。', errorText: '', required: false, invalid: false, disabled: false }, { height: 100 }),
    variant('required', '必填字段', { label: '项目名称', helperText: '', errorText: '', required: true, invalid: false, disabled: false }),
    variant('invalid', '错误字段', { label: '工作邮箱', helperText: '', errorText: '邮箱格式不正确。', required: true, invalid: true, disabled: false }, { height: 104 }),
    variant('disabled', '禁用字段', { label: '组织 ID', helperText: '由系统自动生成。', errorText: '', required: false, invalid: false, disabled: true }, { height: 100 })
  ],
  FileUpload: [
    variant('button', '按钮上传', { accept: 'image/*', multiple: false, maxFiles: 1, kind: 'button', label: '上传图片' }),
    variant('multiple', '多文件上传', { accept: 'image/*', multiple: true, maxFiles: 5, kind: 'button', label: '继续添加图片', sampleFiles: ['homepage-cover.png', 'feature-grid.png'] }, { width: 420, height: 190 }),
    variant('dropzone', '拖放上传区', { accept: 'image/*', multiple: true, maxFiles: 10, kind: 'dropzone', label: '拖入图片或点击选择' }, { width: 420, height: 190 }),
    variant('document', '文档上传', { accept: '.pdf,.doc,.docx', multiple: false, maxFiles: 1, kind: 'dropzone', label: '上传需求文档' }, { width: 420, height: 190 })
  ],
  NumberInput: [
    variant('basic', '基础数字输入', { kind: 'basic', defaultValue: '10', min: 0, max: 100, step: 1, size: 'md', format: 'decimal' }),
    variant('currency', '人民币金额输入', { kind: 'currency', defaultValue: '1299', min: 0, max: 999999, step: 100, size: 'md', format: 'currency', currency: 'CNY' }, { height: 74 }),
    variant('percent', '百分比输入', { kind: 'percent', defaultValue: '68', min: 0, max: 100, step: 1, size: 'md', format: 'percent' }, { height: 74 }),
    variant('stepper', '精密步进输入', { kind: 'stepper', defaultValue: '1.5', min: 0, max: 10, step: 0.1, size: 'md', format: 'decimal' }, { height: 82 }),
    variant('large', '大号数量输入', { kind: 'large', defaultValue: '24', min: 0, max: 999, step: 1, size: 'lg', format: 'decimal' }, { width: 280, height: 72 })
  ],
  PasswordInput: [
    variant('basic', '基础密码框', { placeholder: '请输入密码', size: 'md', variant: 'outline', defaultVisible: false, showStrength: false }),
    variant('visible', '默认显示密码', { placeholder: '请输入密码', size: 'md', variant: 'outline', defaultVisible: true, showStrength: false }),
    variant('strength', '带强度提示', { placeholder: '创建强密码', size: 'md', variant: 'outline', defaultVisible: false, showStrength: true, strength: 3 }, { height: 78 }),
    variant('subtle', '柔和密码框', { placeholder: '请输入访问密钥', size: 'md', variant: 'subtle', defaultVisible: false, showStrength: false })
  ],
  PinInput: [
    variant('four', '四位验证码', { count: 4, size: 'md', type: 'numeric', mask: false }),
    variant('six', '六位验证码', { count: 6, size: 'md', type: 'numeric', mask: false }, { width: 360 }),
    variant('masked', '掩码验证码', { count: 4, size: 'md', type: 'numeric', mask: true, defaultValue: ['4', '8', '2', '6'] }),
    variant('large', '大号验证码', { count: 4, size: 'lg', type: 'numeric', mask: false }, { width: 340, height: 64 })
  ],
  RadioCard: [
    variant('horizontal', '水平单选卡', { orientation: 'horizontal', variant: 'outline', colorPalette: 'blue', size: 'md', defaultValue: 'react' }),
    variant('vertical', '垂直单选卡', { orientation: 'vertical', variant: 'outline', colorPalette: 'purple', size: 'md', defaultValue: 'vue' }, { width: 300, height: 250 }),
    variant('subtle', '柔和单选卡', { orientation: 'horizontal', variant: 'subtle', colorPalette: 'green', size: 'md', defaultValue: 'react' }),
    variant('compact', '紧凑单选卡', { orientation: 'horizontal', variant: 'surface', colorPalette: 'gray', size: 'sm', defaultValue: 'vue' }, { width: 360, height: 90 })
  ],
  Rating: [
    variant('three', '三星评分', { count: 5, defaultValue: 3, size: 'md', colorPalette: 'yellow', allowHalf: false }),
    variant('five', '五星评分', { count: 5, defaultValue: 5, size: 'md', colorPalette: 'yellow', allowHalf: false }),
    variant('half', '半星评分', { count: 5, defaultValue: 3.5, size: 'md', colorPalette: 'orange', allowHalf: true }),
    variant('large', '大号评分', { count: 5, defaultValue: 4, size: 'lg', colorPalette: 'purple', allowHalf: false }, { width: 280, height: 60 }),
    variant('ten', '十分制评分', { count: 10, defaultValue: 8, size: 'sm', colorPalette: 'green', allowHalf: false }, { width: 360, height: 48 })
  ],
  SegmentedControl: [
    variant('basic', '基础分段控制', { size: 'md', defaultValue: 'preview', orientation: 'horizontal', items: [{ value: 'design', label: '设计' }, { value: 'preview', label: '预览' }, { value: 'code', label: '代码' }] }),
    variant('compact', '紧凑分段控制', { size: 'sm', defaultValue: 'week', orientation: 'horizontal', items: [{ value: 'day', label: '日' }, { value: 'week', label: '周' }, { value: 'month', label: '月' }] }, { width: 260, height: 40 }),
    variant('large', '大号分段控制', { size: 'lg', defaultValue: 'desktop', orientation: 'horizontal', items: [{ value: 'desktop', label: '桌面' }, { value: 'tablet', label: '平板' }, { value: 'mobile', label: '手机' }] }, { width: 420, height: 58 }),
    variant('vertical', '垂直分段控制', { size: 'md', defaultValue: 'layout', orientation: 'vertical', items: [{ value: 'layout', label: '布局' }, { value: 'style', label: '样式' }, { value: 'data', label: '数据' }] }, { width: 180, height: 150 })
  ],
  TagsInput: [
    variant('basic', '基础标签输入', { label: '技术标签', defaultValue: ['React', 'Chakra', 'TypeScript'], size: 'md', max: 8, placeholder: '添加标签…' }),
    variant('compact', '紧凑标签输入', { label: '筛选条件', defaultValue: ['设计', 'AI'], size: 'sm', max: 6, placeholder: '添加筛选…' }, { height: 74 }),
    variant('empty', '空标签输入', { label: '关键词', defaultValue: [], size: 'md', max: 10, placeholder: '输入后按回车…' }),
    variant('many', '多标签输入', { label: '项目成员', defaultValue: ['产品', '设计', '前端', '后端', '测试'], size: 'md', max: 10, placeholder: '添加角色…' }, { width: 460, height: 100 })
  ],
  Combobox: [
    variant('framework', '框架搜索选择', { label: '技术框架', placeholder: '输入并搜索', multiple: false, size: 'md', defaultValue: ['react'] }),
    variant('people', '成员搜索选择', { label: '负责人', placeholder: '搜索成员', multiple: false, size: 'md', dataKind: 'people', defaultValue: ['lin'] }),
    variant('multiple', '多选搜索框', { label: '技术栈', placeholder: '搜索并多选', multiple: true, size: 'md', defaultValue: ['react', 'vue'] }, { width: 380, height: 106 }),
    variant('small', '紧凑搜索框', { label: '快速选择', placeholder: '输入关键词', multiple: false, size: 'sm', defaultValue: ['nextjs'] }, { width: 280, height: 68 })
  ],
  Listbox: [
    variant('single', '单选列表框', { label: '选择框架', selectionMode: 'single', orientation: 'vertical', defaultValue: ['react'] }),
    variant('multiple', '多选列表框', { label: '选择技术栈', selectionMode: 'multiple', orientation: 'vertical', defaultValue: ['react', 'vue'] }),
    variant('horizontal', '水平列表框', { label: '选择尺寸', selectionMode: 'single', orientation: 'horizontal', defaultValue: ['medium'], dataKind: 'sizes' }, { width: 460, height: 110 }),
    variant('compact', '紧凑列表框', { label: '选择状态', selectionMode: 'single', orientation: 'vertical', defaultValue: ['ready'], dataKind: 'status' }, { width: 280, height: 190 })
  ],
  Select: [
    variant('basic', '基础选择器', { label: '技术框架', placeholder: '请选择框架', multiple: false, size: 'md', variant: 'outline', defaultValue: ['react'] }),
    variant('multiple', '多选选择器', { label: '技术栈', placeholder: '请选择多项', multiple: true, size: 'md', variant: 'outline', defaultValue: ['react', 'vue'] }, { width: 380, height: 82 }),
    variant('subtle', '柔和选择器', { label: '页面类型', placeholder: '请选择类型', multiple: false, size: 'md', variant: 'subtle', dataKind: 'pages', defaultValue: ['landing'] }),
    variant('small', '紧凑选择器', { label: '状态', placeholder: '选择状态', multiple: false, size: 'sm', variant: 'outline', dataKind: 'status', defaultValue: ['ready'] }, { width: 260, height: 68 })
  ],
  TreeView: [
    variant('files', '文件树', { label: '项目文件', selectionMode: 'single', defaultExpandedValue: ['src'], defaultSelectedValue: ['src/app.tsx'], showGuide: true }),
    variant('expanded', '默认展开文件树', { label: '组件目录', selectionMode: 'single', defaultExpandedValue: ['src', 'src/components'], defaultSelectedValue: ['src/components/form.tsx'], showGuide: true }),
    variant('multiple', '多选文件树', { label: '选择资源', selectionMode: 'multiple', defaultExpandedValue: ['src', 'public'], defaultSelectedValue: ['src/app.tsx', 'public/logo.svg'], showGuide: true }),
    variant('plain', '简洁文件树', { label: '页面结构', selectionMode: 'single', defaultExpandedValue: ['src'], showGuide: false }, { width: 300, height: 250 })
  ],
  ActionBar: [variant('selection', '对象快捷操作栏', { kind: 'selection', selectedCount: 2, placement: 'bottom', actions: ['复制', '移动', '删除'] }), variant('bulk', '批量发布操作栏', { kind: 'bulk', selectedCount: 8, placement: 'bottom', actions: ['发布', '归档', '删除'] }, { width: 220, height: 52 }), variant('top', '布局编排操作栏', { kind: 'layout', selectedCount: 3, placement: 'top', actions: ['左对齐', '组合', '锁定'] }, { width: 230, height: 52 })],
  FloatingPanel: [variant('basic', '工具浮动面板', { kind: 'tools', title: '快捷工具', size: 'md', defaultOpen: false }), variant('open', '图层浮动面板', { kind: 'layers', title: '图层属性', size: 'md', defaultOpen: true }), variant('large', '大型检查面板', { kind: 'audit', title: '设计检查', size: 'lg', defaultOpen: false }, { width: 220, height: 72 })],
  HoverCard: [variant('profile', '资料悬浮卡', { kind: 'profile', title: '林设计师', description: '产品设计师 · 负责官网与设计系统', openDelay: 250, closeDelay: 150 }), variant('product', '产品悬浮卡', { kind: 'product', title: 'Web Design Studio', description: 'AI 与人共同完成高质量网站设计。', openDelay: 200, closeDelay: 100 }), variant('instant', '紧凑状态悬浮卡', { kind: 'compact', title: '设计已同步', description: '刚刚保存到当前项目', openDelay: 0, closeDelay: 100 })],
  OverlayManager: [variant('dialog', '程序化确认框', { kind: 'confirm', title: '发布设计？', description: '确认后会把当前页面发布到预览环境。' }), variant('details', '程序化详情层', { kind: 'details', title: '组件详情', description: '统一管理浮层的创建、更新与关闭。' }), variant('form', '程序化表单层', { kind: 'form', title: '编辑项目', description: '提交后由管理器关闭浮层。' })],
  ToggleTip: [variant('basic', '图标帮助提示', { kind: 'icon', content: '点击触发、再次点击关闭。', showArrow: false, size: 'sm' }, { width: 76, height: 48 }), variant('arrow', '带箭头快捷键提示', { kind: 'shortcut', content: '打开命令面板', shortcut: '⌘ K', showArrow: true, size: 'sm' }, { width: 160, height: 48 }), variant('large', '富内容操作提示', { kind: 'rich', content: '锁定后，协作者仍可查看和批注，但无法移动组件。', showArrow: true, size: 'md' }, { width: 220, height: 58 })],
  Carousel: [variant('basic', '内容故事轮播', { kind: 'story', slideCount: 4, slidesPerPage: 1, defaultPage: 0, loop: false, autoplay: false }), variant('loop', '循环作品轮播', { kind: 'gallery', slideCount: 5, slidesPerPage: 1, defaultPage: 4, loop: true, autoplay: false }), variant('cards', '多卡片轮播', { kind: 'metrics', slideCount: 6, slidesPerPage: 2, defaultPage: 1, loop: true, autoplay: false }, { width: 560, height: 260 }), variant('autoplay', '自动营销轮播', { kind: 'campaign', slideCount: 4, slidesPerPage: 1, defaultPage: 2, loop: true, autoplay: true })],
  ProgressCircle: [variant('blue', '蓝色环形进度', { value: 75, size: 'xl', colorPalette: 'blue', showValue: true }), variant('success', '成功环形进度', { value: 100, size: 'xl', colorPalette: 'green', showValue: true }), variant('warning', '警告环形进度', { value: 42, size: 'lg', colorPalette: 'orange', showValue: true }), variant('plain', '纯环形进度', { value: 68, size: 'xl', colorPalette: 'purple', showValue: false })],
  Status: [variant('success', '成功状态', { colorPalette: 'green', label: '运行正常', size: 'md' }), variant('warning', '警告状态', { colorPalette: 'orange', label: '需要注意', size: 'md' }), variant('error', '错误状态', { colorPalette: 'red', label: '连接中断', size: 'md' }), variant('info', '信息状态', { colorPalette: 'blue', label: '正在同步', size: 'md' })],
  Toast: [variant('success', '成功通知', { type: 'success', title: '保存成功', description: '设计已经安全保存。', closable: true }), variant('error', '错误通知', { type: 'error', title: '保存失败', description: '请检查网络后重试。', closable: true }), variant('loading', '加载通知', { type: 'loading', title: '正在生成', description: 'AI 正在完善页面设计。', closable: false }), variant('info', '信息通知', { type: 'info', title: '新版本可用', description: '组件库目录已更新。', closable: true })],
  Clipboard: [variant('button', '复制按钮', { value: 'https://chakra-ui.com', kind: 'button', label: '复制链接' }), variant('input', '复制输入框', { value: 'npm install @chakra-ui/react', kind: 'input', label: '安装命令' }, { width: 380 }), variant('code', '复制代码片段', { value: '<Button>开始设计</Button>', kind: 'code', label: '复制代码' }, { width: 380, height: 70 })],
  Image: [variant('landscape', '横向图片', { alt: '网站设计预览', fit: 'cover', borderRadius: 'lg', aspect: 'landscape' }, { width: 420, height: 240 }), variant('square', '方形图片', { alt: '产品配图', fit: 'cover', borderRadius: 'xl', aspect: 'square' }, { width: 260, height: 260 }), variant('contain', '完整显示图片', { alt: '品牌插图', fit: 'contain', borderRadius: 'lg', aspect: 'landscape' }, { width: 420, height: 240 })],
  DataList: [variant('horizontal', '水平数据列表', { orientation: 'horizontal', size: 'md' }), variant('vertical', '垂直数据列表', { orientation: 'vertical', size: 'md' }), variant('compact', '紧凑数据列表', { orientation: 'horizontal', size: 'sm' }, { width: 300, height: 140 })],
  Icon: [variant('heart', '爱心图标', { icon: 'heart', size: 'xl', color: 'pink.600' }), variant('sparkle', '闪光图标', { icon: 'sparkle', size: 'xl', color: 'purple.600' }), variant('check', '完成图标', { icon: 'check', size: 'xl', color: 'green.600' }), variant('info', '信息图标', { icon: 'info', size: 'xl', color: 'blue.600' })],
  Marquee: [variant('logos', '品牌横向滚动', { side: 'left', reverse: false, speed: 40, pauseOnInteraction: true, edge: true }), variant('reverse', '反向横向滚动', { side: 'left', reverse: true, speed: 50, pauseOnInteraction: true, edge: true }), variant('vertical', '纵向滚动', { side: 'bottom', reverse: false, speed: 35, pauseOnInteraction: true, edge: true }, { width: 260, height: 220 }), variant('fast', '快速消息滚动', { side: 'left', reverse: false, speed: 90, pauseOnInteraction: true, edge: false })],
  QRCode: [variant('basic', '基础二维码', { value: 'https://chakra-ui.com', size: 160, color: '#111827', overlay: false }), variant('brand', '品牌二维码', { value: 'https://example.com/design', size: 180, color: '#5D50DF', overlay: true }, { width: 210, height: 210 }), variant('compact', '紧凑二维码', { value: 'https://example.com', size: 120, color: '#111827', overlay: false }, { width: 150, height: 150 })],
  Tag: [variant('plain', '基础标签', { variant: 'surface', colorPalette: 'gray', size: 'md', closable: false }), variant('closable', '可关闭标签', { variant: 'subtle', colorPalette: 'blue', size: 'md', closable: true }), variant('solid', '实色标签', { variant: 'solid', colorPalette: 'purple', size: 'md', closable: false }), variant('large', '大号标签', { variant: 'outline', colorPalette: 'green', size: 'lg', closable: true })],
  LocaleProvider: [variant('chinese', '中文语言环境', { locale: 'zh-CN', direction: 'ltr', title: '欢迎使用 Chakra UI' }), variant('english', '英文语言环境', { locale: 'en-US', direction: 'ltr', title: 'Welcome to Chakra UI' }), variant('arabic', '阿拉伯语 RTL', { locale: 'ar-AE', direction: 'rtl', title: 'مرحباً بكم في تشاكرا يو آي' }), variant('japanese', '日文语言环境', { locale: 'ja-JP', direction: 'ltr', title: 'Chakra UI へようこそ' })],
  FormatNumber: [variant('decimal', '小数格式', { value: 1450.45, locale: 'zh-CN', style: 'decimal', maximumFractionDigits: 2 }), variant('currency', '人民币格式', { value: 12999, locale: 'zh-CN', style: 'currency', currency: 'CNY' }), variant('percent', '百分比格式', { value: 0.684, locale: 'zh-CN', style: 'percent', maximumFractionDigits: 1 }), variant('compact', '紧凑数字格式', { value: 2864200, locale: 'zh-CN', style: 'decimal', notation: 'compact' })],
  FormatByte: [variant('standard', '标准文件大小', { value: 1450.45, locale: 'zh-CN', unitSystem: 'decimal', unitDisplay: 'short' }), variant('binary', '二进制文件大小', { value: 10485760, locale: 'zh-CN', unitSystem: 'binary', unitDisplay: 'short' }), variant('long', '完整单位文件大小', { value: 2560000000, locale: 'zh-CN', unitSystem: 'decimal', unitDisplay: 'long' })],
  Checkmark: [variant('checked', '选中勾选标记', { checked: true, indeterminate: false, disabled: false, size: 'md', colorPalette: 'blue' }), variant('empty', '未选勾选标记', { checked: false, indeterminate: false, disabled: false, size: 'md', colorPalette: 'gray' }), variant('mixed', '半选勾选标记', { checked: false, indeterminate: true, disabled: false, size: 'md', colorPalette: 'purple' }), variant('disabled', '禁用勾选标记', { checked: true, indeterminate: false, disabled: true, size: 'md', colorPalette: 'gray' })],
  ClientOnly: [variant('content', '客户端连接状态', { fallback: '正在连接客户端…', kind: 'content' }), variant('time', '本地时间卡片', { fallback: '正在读取本地时间…', kind: 'time' }, { height: 118 }), variant('viewport', '视口指标面板', { fallback: '正在读取视口…', kind: 'viewport' }, { width: 360, height: 128 })],
  EnvironmentProvider: [variant('document', '文档运行环境', { environment: 'document', label: '当前页面 Document' }, { height: 120 }), variant('canvas', '画布浮层环境', { environment: 'canvas', label: '设计画布坐标空间' }, { height: 150 }), variant('iframe', '嵌入页面环境', { environment: 'iframe', label: 'iframe 隔离文档' }, { height: 150 })],
  For: [variant('cards', '循环卡片', { count: 4, kind: 'cards' }), variant('tags', '循环标签', { count: 6, kind: 'tags' }), variant('rows', '循环数据行', { count: 5, kind: 'rows' })],
  Presence: [variant('visible', '常驻同步状态', { kind: 'status', present: true, lazyMount: false, unmountOnExit: false, animation: 'fade' }), variant('toggle', '按需展开详情', { kind: 'details', present: false, lazyMount: true, unmountOnExit: false, animation: 'fade' }, { height: 156 }), variant('scale', '缩放确认面板', { kind: 'confirm', present: false, lazyMount: true, unmountOnExit: true, animation: 'scale' }, { width: 340, height: 178 })],
  Portal: [variant('badge', '传送徽标', { kind: 'badge', placement: 'top-end' }), variant('panel', '传送面板', { kind: 'panel', placement: 'bottom-end' }), variant('message', '传送消息', { kind: 'message', placement: 'top-center' })],
  Radiomark: [variant('checked', '选中单选标记', { checked: true, disabled: false, size: 'md', colorPalette: 'blue' }), variant('empty', '未选单选标记', { checked: false, disabled: false, size: 'md', colorPalette: 'gray' }), variant('disabled', '禁用单选标记', { checked: true, disabled: true, size: 'md', colorPalette: 'gray' }), variant('large', '大号单选标记', { checked: true, disabled: false, size: 'lg', colorPalette: 'purple' })],
  Show: [variant('revealed', '条件已满足', { threshold: 3, initialCount: 4, label: '条件内容已显示' }), variant('hidden', '条件未满足', { threshold: 3, initialCount: 0, label: '点击达到条件后显示' }), variant('immediate', '立即显示', { threshold: 0, initialCount: 0, label: '始终满足显示条件' })],
  SkipNav: [variant('basic', '网站顶部导航跳转', { kind: 'website', label: '跳到主要内容', navLabel: '产品 / 方案 / 价格', contentLabel: '主视觉内容' }), variant('english', '英文产品导航跳转', { kind: 'product', label: 'Skip to content', navLabel: 'Product navigation', contentLabel: 'Product overview' }), variant('dashboard', '控制台侧栏跳转', { kind: 'dashboard', label: '跳过侧边栏', navLabel: '控制台侧边栏', contentLabel: '数据工作区域' }, { width: 460, height: 250 })],
  VisuallyHidden: [variant('notification', '通知无障碍文本', { hiddenText: '3 条未读通知', visibleText: '3', icon: 'bell' }), variant('icon-button', '图标按钮标签', { hiddenText: '打开设置', visibleText: '', icon: 'settings' }), variant('status', '状态补充文本', { hiddenText: '当前状态：运行正常', visibleText: '正常', icon: 'check' })],
  Theme: [variant('dark', '深色主题区域', { appearance: 'dark', colorPalette: 'teal' }), variant('light', '浅色主题区域', { appearance: 'light', colorPalette: 'blue' }), variant('purple', '紫色主题区域', { appearance: 'dark', colorPalette: 'purple' }), variant('green', '绿色浅色主题', { appearance: 'light', colorPalette: 'green' })],
  Textarea: [
    variant('outline', '描边文本域', { variant: 'outline', size: 'md', rows: 4 }),
    variant('subtle', '柔和文本域', { variant: 'subtle', size: 'md', rows: 4 }),
    variant('flushed', '下划线文本域', { variant: 'flushed', size: 'md', rows: 4 })
  ],
  RadioGroup: [
    variant('solid', '实心单选组', { variant: 'solid', size: 'md', orientation: 'horizontal', defaultValue: 'monthly' }),
    variant('outline', '描边单选组', { variant: 'outline', size: 'md', orientation: 'horizontal', defaultValue: 'yearly' }),
    variant('subtle', '柔和单选组', { variant: 'subtle', size: 'md', orientation: 'horizontal', defaultValue: 'enterprise' }),
    variant('vertical', '垂直单选组', { variant: 'solid', size: 'md', orientation: 'vertical', defaultValue: 'monthly' }, { width: 240, height: 130 })
  ],
  Slider: [
    variant('outline', '描边滑块', { defaultValue: 58, min: 0, max: 100, colorPalette: 'blue', variant: 'outline', size: 'md', orientation: 'horizontal' }),
    variant('solid', '实心滑块', { defaultValue: 72, min: 0, max: 100, colorPalette: 'purple', variant: 'solid', size: 'md', orientation: 'horizontal' }),
    variant('small', '紧凑滑块', { defaultValue: 36, min: 0, max: 100, colorPalette: 'green', variant: 'outline', size: 'sm', orientation: 'horizontal' }),
    variant('vertical', '垂直滑块', { defaultValue: 64, min: 0, max: 100, colorPalette: 'orange', variant: 'solid', size: 'md', orientation: 'vertical' }, { width: 70, height: 220 })
  ],
  Fieldset: [
    variant('small', '紧凑联系字段组', { kind: 'contact', legend: '联系信息', helperText: '填写常用联系方式。', size: 'sm', disabled: false }, { height: 220 }),
    variant('medium', '完整个人资料字段组', { kind: 'profile', legend: '个人资料', helperText: '这些信息会展示在个人页面。', size: 'md', disabled: false }, { height: 390 }),
    variant('disabled', '锁定只读字段组', { kind: 'locked', legend: '组织资料', helperText: '由管理员统一维护，当前不可修改。', size: 'md', disabled: true }, { height: 300 })
  ],
  Editable: [
    variant('small', '小号预览文本', { value: '导航标题', placeholder: '输入名称', size: 'sm', activationMode: 'click', defaultEdit: false }, { height: 44 }),
    variant('medium', '正在编辑文本', { value: '可直接修改的页面标题', placeholder: '输入名称', size: 'md', activationMode: 'click', defaultEdit: true }, { width: 340, height: 58 }),
    variant('double-click', '双击编辑文本', { value: '双击修改产品标题', placeholder: '双击输入标题', size: 'lg', activationMode: 'dblclick', defaultEdit: false }, { width: 360, height: 64 })
  ],
  Breadcrumb: [
    variant('plain', '简洁面包屑', { variant: 'plain', size: 'md', separator: '/' }),
    variant('underline', '下划线面包屑', { variant: 'underline', size: 'md', separator: '/' }),
    variant('chevron', '箭头分隔面包屑', { variant: 'plain', size: 'lg', separator: '›' })
  ],
  Pagination: [
    variant('basic', '基础分页', { count: 100, pageSize: 10, defaultPage: 3, siblingCount: 1, variant: 'outline', size: 'sm' }),
    variant('compact', '紧凑分页', { count: 60, pageSize: 10, defaultPage: 2, siblingCount: 0, variant: 'ghost', size: 'xs' }, { width: 280 }),
    variant('many-pages', '多页省略', { count: 500, pageSize: 10, defaultPage: 18, siblingCount: 1, variant: 'outline', size: 'sm' }, { width: 440 }),
    variant('large', '大号分页', { count: 80, pageSize: 10, defaultPage: 4, siblingCount: 1, variant: 'solid', size: 'md' }, { width: 420, height: 54 })
  ],
  Steps: [
    variant('horizontal', '水平步骤', { defaultStep: 1, orientation: 'horizontal', size: 'md', variant: 'solid' }),
    variant('completed', '已完成步骤', { defaultStep: 3, orientation: 'horizontal', size: 'md', variant: 'solid' }),
    variant('subtle', '柔和步骤', { defaultStep: 1, orientation: 'horizontal', size: 'md', variant: 'subtle' }),
    variant('vertical', '垂直步骤', { defaultStep: 1, orientation: 'vertical', size: 'md', variant: 'outline' }, { width: 300, height: 250 })
  ],
  Collapsible: [
    variant('open', '默认展开', { defaultOpen: true }),
    variant('closed', '默认收起', { defaultOpen: false }, { height: 60 })
  ],
  Avatar: [
    variant('subtle', '柔和圆形头像', { size: 'lg', name: 'AI Designer', variant: 'subtle', shape: 'full', colorPalette: 'blue' }),
    variant('solid', '实心圆形头像', { size: 'lg', name: 'Product Owner', variant: 'solid', shape: 'full', colorPalette: 'purple' }),
    variant('outline', '描边头像', { size: 'lg', name: 'Design Team', variant: 'outline', shape: 'full', colorPalette: 'gray' }),
    variant('rounded', '圆角方形头像', { size: 'lg', name: 'Web Studio', variant: 'subtle', shape: 'rounded', colorPalette: 'green' }),
    variant('square', '方形头像', { size: 'lg', name: 'WS', variant: 'solid', shape: 'square', colorPalette: 'orange' })
  ],
  Table: [
    variant('line', '线形表格', { variant: 'line', size: 'md', striped: false, interactive: false, stickyHeader: false, showColumnBorder: false }),
    variant('outline', '描边表格', { variant: 'outline', size: 'md', striped: false, interactive: false, stickyHeader: false, showColumnBorder: true }),
    variant('striped', '斑马纹表格', { variant: 'line', size: 'md', striped: true, interactive: false, stickyHeader: false, showColumnBorder: false }),
    variant('interactive', '悬停交互表格', { variant: 'line', size: 'md', striped: false, interactive: true, stickyHeader: false, showColumnBorder: false }),
    variant('compact', '紧凑表格', { variant: 'outline', size: 'sm', striped: true, interactive: true, stickyHeader: true, showColumnBorder: true })
  ],
  Stat: [
    variant('positive', '增长统计', { label: '本月活跃用户', value: '28,642', change: '+12.5%', direction: 'up', size: 'md' }),
    variant('negative', '下降统计', { label: '平均响应时间', value: '1.82s', change: '-8.4%', direction: 'down', size: 'md' }),
    variant('neutral', '中性统计', { label: '待处理任务', value: '36', change: '与昨日持平', direction: 'neutral', size: 'lg' }, { width: 260, height: 130 })
  ],
  Timeline: [
    variant('solid', '实心时间轴', { variant: 'solid', size: 'md', showLastSeparator: false }),
    variant('subtle', '柔和时间轴', { variant: 'subtle', size: 'md', showLastSeparator: false }),
    variant('outline', '描边时间轴', { variant: 'outline', size: 'md', showLastSeparator: true }),
    variant('plain', '简洁时间轴', { variant: 'plain', size: 'sm', showLastSeparator: false })
  ],
  Spinner: [
    variant('small', '小号加载', { size: 'sm', colorPalette: 'blue' }, { width: 70, height: 60 }),
    variant('medium', '标准加载', { size: 'md', colorPalette: 'blue' }),
    variant('large', '大号加载', { size: 'xl', colorPalette: 'purple' }),
    variant('success', '成功色加载', { size: 'lg', colorPalette: 'green' })
  ],
  EmptyState: [
    variant('project', '空项目', { title: '没有找到项目', description: '创建一个项目后，它会显示在这里。', icon: 'folder', action: '创建项目', size: 'md' }),
    variant('search', '无搜索结果', { title: '没有匹配结果', description: '尝试调整关键词或筛选条件。', icon: 'search', action: '清除筛选', size: 'md' }),
    variant('error', '加载失败', { title: '内容加载失败', description: '请检查网络连接后重试。', icon: 'warning', action: '重新加载', size: 'lg' })
  ],
  Popover: ['bottom', 'top', 'left', 'right'].map((placement) => variant(placement, `${placement} 气泡`, { title: '产品信息', placement })),
  Tooltip: ['top', 'bottom', 'left', 'right'].map((placement) => variant(placement, `${placement} 提示`, { content: '这是 Chakra UI 提示内容', placement })),
  Menu: [
    variant('basic', '基础菜单', { kind: 'basic', items: [{ key: 'edit', label: '编辑' }, { key: 'duplicate', label: '复制' }, { key: 'delete', label: '删除' }] }),
    variant('icons', '带图标菜单', { kind: 'icons', items: [{ key: 'edit', label: '编辑', icon: '✎' }, { key: 'copy', label: '复制', icon: '□' }, { key: 'delete', label: '删除', icon: '×' }] }),
    variant('checkbox', '多选菜单', { kind: 'checkbox', items: [{ key: 'grid', label: '显示网格', checked: true }, { key: 'ruler', label: '显示标尺', checked: false }, { key: 'snap', label: '自动吸附', checked: true }] }),
    variant('grouped', '分组菜单', { kind: 'grouped', items: [{ key: 'profile', label: '个人资料', group: '账户' }, { key: 'settings', label: '设置', group: '账户' }, { key: 'help', label: '帮助中心', group: '支持' }] })
  ],
  Button: [
    { id: 'solid', label: '实心按钮', props: { variant: 'solid', colorPalette: 'blue', size: 'md' }, content: '主要操作' },
    { id: 'subtle', label: '柔和按钮', props: { variant: 'subtle', colorPalette: 'blue', size: 'md' }, content: '柔和操作' },
    { id: 'surface', label: '表面按钮', props: { variant: 'surface', colorPalette: 'blue', size: 'md' }, content: '表面按钮' },
    { id: 'outline', label: '描边按钮', props: { variant: 'outline', colorPalette: 'gray', size: 'md' }, content: '描边按钮' },
    { id: 'ghost', label: '幽灵按钮', props: { variant: 'ghost', colorPalette: 'gray', size: 'md' }, content: '幽灵按钮' },
    { id: 'plain', label: '简洁按钮', props: { variant: 'plain', colorPalette: 'blue', size: 'md' }, content: '简洁按钮' },
    { id: 'danger', label: '危险按钮', props: { variant: 'solid', colorPalette: 'red', size: 'md' }, content: '删除' }
  ],
  Input: [
    { id: 'outline', label: '描边输入框', props: { variant: 'outline', size: 'md' } },
    { id: 'subtle', label: '柔和输入框', props: { variant: 'subtle', size: 'md' } },
    { id: 'flushed', label: '下划线输入框', props: { variant: 'flushed', size: 'md' } }
  ],
  NativeSelect: [
    { id: 'outline', label: '描边选择框', props: { variant: 'outline', size: 'md' } },
    { id: 'subtle', label: '柔和选择框', props: { variant: 'subtle', size: 'md' } },
    { id: 'plain', label: '简洁选择框', props: { variant: 'plain', size: 'md' } },
    { id: 'ghost', label: '透明选择框', props: { variant: 'ghost', size: 'md' } }
  ],
  Checkbox: [
    { id: 'solid', label: '实心多选框', props: { variant: 'solid', size: 'md', colorPalette: 'blue', defaultChecked: true } },
    { id: 'subtle', label: '柔和多选框', props: { variant: 'subtle', size: 'md', colorPalette: 'green', defaultChecked: true } },
    { id: 'outline', label: '描边多选框', props: { variant: 'outline', size: 'md', colorPalette: 'gray', defaultChecked: false } },
    { id: 'large', label: '大号多选框', props: { variant: 'solid', size: 'lg', colorPalette: 'purple', defaultChecked: true } }
  ],
  Switch: [
    { id: 'solid', label: '实心开关', props: { variant: 'solid', size: 'md', colorPalette: 'blue', defaultChecked: true } },
    { id: 'raised', label: '浮起开关', props: { variant: 'raised', size: 'md', colorPalette: 'green', defaultChecked: true } },
    { id: 'large', label: '大号开关', props: { variant: 'solid', size: 'lg', colorPalette: 'purple', defaultChecked: true } },
    { id: 'off', label: '关闭状态', props: { variant: 'solid', size: 'md', colorPalette: 'gray', defaultChecked: false } }
  ],
  Badge: ['solid', 'subtle', 'outline', 'surface', 'plain'].map((value) => variant(value, `${value} 徽标`, { colorPalette: value === 'solid' ? 'blue' : 'purple', variant: value, size: 'sm' })),
  Alert: [
    ...['info', 'success', 'warning', 'error', 'neutral'].map((status) => variant(status, `${status} 提示`, { status, variant: 'subtle', size: 'md', inline: false })),
    variant('solid', '实色提示', { status: 'info', variant: 'solid', size: 'md', inline: false }),
    variant('outline', '描边提示', { status: 'warning', variant: 'outline', size: 'md', inline: false }),
    variant('inline', '行内提示', { status: 'success', variant: 'surface', size: 'sm', inline: true }, { height: 64 })
  ],
  Card: [
    { id: 'elevated', label: '悬浮卡片', props: { variant: 'elevated' } },
    { id: 'outline', label: '描边卡片', props: { variant: 'outline' } },
    { id: 'subtle', label: '柔和卡片', props: { variant: 'subtle' } }
  ],
  Tabs: [
    { id: 'line', label: '线形标签页', props: { variant: 'line', size: 'md', fitted: false, justify: 'start', defaultValue: 'overview' } },
    { id: 'subtle', label: '柔和标签页', props: { variant: 'subtle', size: 'md', fitted: false, justify: 'start', defaultValue: 'overview' } },
    { id: 'enclosed', label: '卡片标签页', props: { variant: 'enclosed', size: 'md', fitted: false, justify: 'start', defaultValue: 'overview' } },
    { id: 'outline', label: '描边标签页', props: { variant: 'outline', size: 'md', fitted: false, justify: 'start', defaultValue: 'overview' } },
    { id: 'plain', label: '简洁标签页', props: { variant: 'plain', size: 'md', fitted: false, justify: 'start', defaultValue: 'overview' } },
    { id: 'fitted', label: '等宽标签页', props: { variant: 'line', size: 'md', fitted: true, justify: 'center', defaultValue: 'overview' } }
  ],
  Accordion: [
    { id: 'outline', label: '描边手风琴', props: { variant: 'outline', size: 'md', multiple: false } },
    { id: 'subtle', label: '柔和手风琴', props: { variant: 'subtle', size: 'md', multiple: false } },
    { id: 'enclosed', label: '封闭手风琴', props: { variant: 'enclosed', size: 'md', multiple: false } },
    { id: 'plain', label: '简洁手风琴', props: { variant: 'plain', size: 'md', multiple: true } }
  ],
  Dialog: [
    { id: 'center', label: '居中对话框', props: { placement: 'center', size: 'md', title: '确认操作' } },
    { id: 'top', label: '顶部对话框', props: { placement: 'top', size: 'md', title: '编辑信息' } },
    { id: 'bottom', label: '底部对话框', props: { placement: 'bottom', size: 'md', title: '快捷操作' } },
    { id: 'large', label: '大型对话框', props: { placement: 'center', size: 'lg', title: '产品详情' }, width: 180 },
    { id: 'scroll-inside', label: '内部滚动对话框', props: { placement: 'center', size: 'lg', scrollBehavior: 'inside', title: '长内容' }, width: 180 },
    { id: 'cover', label: '覆盖式对话框', props: { placement: 'center', size: 'cover', title: '沉浸预览' }, width: 180 }
  ],
  Drawer: [
    { id: 'right', label: '右侧抽屉', props: { placement: 'end', size: 'md', title: '详情面板' } },
    { id: 'left', label: '左侧抽屉', props: { placement: 'start', size: 'md', title: '导航面板' } },
    { id: 'top', label: '顶部抽屉', props: { placement: 'top', size: 'md', title: '筛选条件' } },
    { id: 'bottom', label: '底部抽屉', props: { placement: 'bottom', size: 'md', title: '快捷操作' } },
    { id: 'large', label: '大型右侧抽屉', props: { placement: 'end', size: 'lg', title: '编辑详情' } },
    { id: 'contained', label: '画布内抽屉', props: { placement: 'end', size: 'md', contained: true, title: '局部面板' } },
    { id: 'full', label: '全屏抽屉', props: { placement: 'end', size: 'full', title: '全屏工作区' } }
  ],
  Progress: [
    { id: 'outline', label: '描边进度', props: { value: 68, colorPalette: 'blue', variant: 'outline', shape: 'rounded', size: 'md', striped: false, animated: false } },
    { id: 'subtle', label: '柔和进度', props: { value: 82, colorPalette: 'green', variant: 'subtle', shape: 'rounded', size: 'md', striped: false, animated: false } },
    { id: 'striped', label: '条纹进度', props: { value: 54, colorPalette: 'purple', variant: 'outline', shape: 'full', size: 'md', striped: true, animated: true } },
    { id: 'square', label: '直角进度', props: { value: 42, colorPalette: 'orange', variant: 'outline', shape: 'square', size: 'lg', striped: false, animated: false } }
  ],
  Skeleton: [
    { id: 'text-pulse', label: '脉冲文本骨架', props: { kind: 'text', lines: 3, variant: 'pulse' } },
    { id: 'text-shine', label: '流光文本骨架', props: { kind: 'text', lines: 3, variant: 'shine' } },
    { id: 'card', label: '卡片骨架', props: { kind: 'card', lines: 2, variant: 'shine' }, height: 150 },
    { id: 'avatar', label: '头像骨架', props: { kind: 'avatar', lines: 2, variant: 'pulse' }, height: 90 }
  ]
};

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
  item('OverlayManager', '浮层管理器', '浮层', '▣', 'button', 180, 44, '程序化打开', { kind: 'dialog', title: '设计确认', description: '通过 Overlay Manager 打开的对话框。' }),
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
  item('EnvironmentProvider', '环境提供器', '工具', 'E', 'card', 360, 110, '', { environment: 'document', label: '使用当前页面 Document' }),
  item('For', '循环渲染', '工具', '↻', 'section', 380, 150, '', { count: 4, kind: 'cards' }),
  item('Presence', '显隐过渡', '工具', '◐', 'card', 300, 130, '切换显示', { present: true, lazyMount: false, unmountOnExit: false, animation: 'fade' }),
  item('Portal', '传送门', '工具', '↗', 'button', 160, 44, '显示传送内容', { kind: 'badge', placement: 'top-end' }),
  item('Radiomark', '单选标记', '工具', '◉', 'checkbox', 56, 56, '', { checked: true, disabled: false, size: 'md', colorPalette: 'blue' }),
  item('Show', '条件渲染', '工具', '?', 'card', 320, 140, '', { threshold: 3, initialCount: 4, label: '条件内容已显示' }),
  item('SkipNav', '跳过导航', '工具', '⇥', 'section', 420, 220, '', { label: '跳到主要内容', navLabel: '页面导航', contentLabel: '主要内容' }),
  item('VisuallyHidden', '视觉隐藏', '工具', '◌', 'button', 180, 48, '', { hiddenText: '3 条未读通知', visibleText: '3', icon: 'bell' }),
  item('Theme', '局部主题', '工具', '◒', 'section', 360, 160, '', { appearance: 'dark', colorPalette: 'teal' })
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
