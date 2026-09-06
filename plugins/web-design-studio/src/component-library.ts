import { breakpointFor, componentsForPage, resolveComponent } from './editor-model.js';
import { pageIdForComponent, type WebComponentStyle, type WebComponentType, type WebDesignComponent, type WebDesignDevice, type WebDesignDocument, type WebDesignLibraryBinding } from './schema.js';
import { componentDefaults } from './templates.js';

export type WebDesignBlockPresetId =
  | 'announcement' | 'navbar' | 'navbar-minimal'
  | 'hero' | 'hero-centered' | 'hero-product'
  | 'logo-cloud' | 'brand-marquee' | 'stats'
  | 'showcase' | 'bento' | 'features' | 'steps' | 'integrations'
  | 'testimonials' | 'case-studies' | 'gallery' | 'blog' | 'team'
  | 'pricing' | 'comparison' | 'faq' | 'contact' | 'newsletter' | 'cta'
  | 'spotlight' | 'footer' | 'footer-minimal';
export type WebDesignPageTemplateId = 'saas' | 'ai-product' | 'developer' | 'launch' | 'business' | 'creative' | 'portfolio' | 'mobile-app';

export interface WebDesignBlockPreset {
  id: WebDesignBlockPresetId;
  category: '导航' | '首屏' | '品牌信任' | '产品介绍' | '内容展示' | '转化' | '视觉效果' | '页脚';
  name: string;
  description: string;
  icon: string;
  keywords: string[];
}

export interface WebDesignPageTemplate {
  id: WebDesignPageTemplateId;
  name: string;
  description: string;
  icon: string;
  category: string;
  palette: [string, string, string];
  blocks: WebDesignBlockPresetId[];
}

export const WEB_DESIGN_BLOCK_PRESETS: WebDesignBlockPreset[] = [
  { id: 'announcement', category: '导航', name: '公告横幅', description: '发布活动、新功能或版本信息', icon: '⌁', keywords: ['公告', 'announcement', '活动', '更新'] },
  { id: 'navbar', category: '导航', name: '产品导航', description: '品牌、导航链接和主要行动按钮', icon: '☰', keywords: ['导航', 'navbar', 'header', '菜单'] },
  { id: 'navbar-minimal', category: '导航', name: '极简导航', description: '适合品牌与作品集的克制导航', icon: '—', keywords: ['导航', 'minimal', '品牌', '作品集'] },
  { id: 'hero', category: '首屏', name: '分栏首屏', description: '文案、双按钮与产品主视觉', icon: '◩', keywords: ['hero', '首屏', '分栏', '产品'] },
  { id: 'hero-centered', category: '首屏', name: '居中首屏', description: '大标题、渐变光效与悬浮数据卡', icon: '✦', keywords: ['hero', '首屏', '居中', '渐变'] },
  { id: 'hero-product', category: '首屏', name: '产品界面首屏', description: '以大型产品界面预览作为视觉中心', icon: '▣', keywords: ['hero', 'dashboard', '产品截图', '界面'] },
  { id: 'logo-cloud', category: '品牌信任', name: '客户品牌墙', description: '展示合作品牌与客户信任', icon: '◇', keywords: ['logo', '品牌墙', '客户', '信任'] },
  { id: 'brand-marquee', category: '品牌信任', name: '品牌跑马灯', description: '高密度横向品牌与能力标签', icon: '↔', keywords: ['marquee', '品牌', '滚动', '标签'] },
  { id: 'stats', category: '品牌信任', name: '关键数据', description: '突出用户量、效率和增长指标', icon: '#', keywords: ['数据', 'stats', '指标', '增长'] },
  { id: 'showcase', category: '产品介绍', name: '产品聚光展示', description: '大幅产品预览与浮层信息卡', icon: '◫', keywords: ['showcase', '产品展示', '截图', '预览'] },
  { id: 'bento', category: '产品介绍', name: 'Bento 特性墙', description: '不等宽卡片组成的现代能力矩阵', icon: '▦', keywords: ['bento', '功能', '卡片', '特性'] },
  { id: 'features', category: '产品介绍', name: '三列功能介绍', description: '清晰介绍三项核心能力', icon: '▥', keywords: ['功能', 'features', '卡片'] },
  { id: 'steps', category: '产品介绍', name: '使用流程', description: '三步流程与连续视觉引导', icon: '①', keywords: ['步骤', 'steps', '流程', 'how it works'] },
  { id: 'integrations', category: '产品介绍', name: '集成生态', description: '展示连接工具、平台和开放能力', icon: '⌘', keywords: ['集成', '生态', 'integrations', '平台'] },
  { id: 'testimonials', category: '内容展示', name: '用户评价', description: '重点评价与用户身份信息', icon: '❞', keywords: ['评价', 'testimonial', '口碑', '客户'] },
  { id: 'case-studies', category: '内容展示', name: '客户案例', description: '图文案例卡与成果数据', icon: '▤', keywords: ['案例', 'case study', '客户', '成果'] },
  { id: 'gallery', category: '内容展示', name: '视觉画廊', description: '适合品牌、空间和创意作品展示', icon: '▧', keywords: ['画廊', 'gallery', '作品', '图片'] },
  { id: 'blog', category: '内容展示', name: '内容精选', description: '文章、洞察和更新内容卡片', icon: '¶', keywords: ['博客', 'blog', '文章', '内容'] },
  { id: 'team', category: '内容展示', name: '团队介绍', description: '成员头像、角色与品牌文化', icon: '◎', keywords: ['团队', 'team', '成员', '关于'] },
  { id: 'pricing', category: '转化', name: '价格方案', description: '三档产品价格与推荐方案', icon: '¥', keywords: ['价格', 'pricing', '套餐'] },
  { id: 'comparison', category: '转化', name: '方案对比', description: '直观比较版本能力与权益', icon: '≡', keywords: ['比较', 'comparison', '方案', '表格'] },
  { id: 'faq', category: '转化', name: '常见问题', description: '购买决策前的重点问答', icon: '?', keywords: ['faq', '问题', '帮助'] },
  { id: 'contact', category: '转化', name: '联系表单', description: '销售线索与项目需求收集', icon: '✉', keywords: ['联系', 'contact', '表单'] },
  { id: 'newsletter', category: '转化', name: '邮件订阅', description: '简洁的内容订阅与更新入口', icon: '@', keywords: ['邮件', 'newsletter', '订阅', '更新'] },
  { id: 'cta', category: '转化', name: '行动号召', description: '用于页面结尾的强转化区域', icon: '→', keywords: ['cta', '转化', '行动', '注册'] },
  { id: 'spotlight', category: '视觉效果', name: '渐变聚光舞台', description: '光晕、网格与悬浮信息组成的视觉段落', icon: '◉', keywords: ['光效', 'spotlight', '渐变', '视觉'] },
  { id: 'footer', category: '页脚', name: '多列页脚', description: '品牌、产品、资源与法律链接', icon: '▔', keywords: ['页脚', 'footer', '版权'] },
  { id: 'footer-minimal', category: '页脚', name: '极简页脚', description: '适合作品集与品牌网站的简洁收尾', icon: '＿', keywords: ['页脚', 'minimal', '版权', '社交'] }
];

export const WEB_DESIGN_PAGE_TEMPLATES: WebDesignPageTemplate[] = [
  { id: 'saas', category: '产品', name: 'SaaS 产品站', description: '完整产品叙事、能力展示、价格与转化路径', icon: 'S', palette: ['#0B1020', '#635BFF', '#EEF2FF'], blocks: ['announcement', 'navbar', 'hero-product', 'logo-cloud', 'bento', 'showcase', 'integrations', 'testimonials', 'pricing', 'faq', 'cta', 'footer'] },
  { id: 'ai-product', category: '产品', name: 'AI 产品站', description: '渐变首屏、智能能力矩阵、案例与增长数据', icon: 'AI', palette: ['#111827', '#7C3AED', '#22D3EE'], blocks: ['announcement', 'navbar-minimal', 'hero-centered', 'brand-marquee', 'spotlight', 'bento', 'stats', 'case-studies', 'testimonials', 'pricing', 'cta', 'footer'] },
  { id: 'developer', category: '技术', name: '开发者工具', description: '产品界面、集成生态、技术流程与开发者口碑', icon: '</>', palette: ['#09090B', '#22C55E', '#E4E4E7'], blocks: ['navbar-minimal', 'hero-product', 'brand-marquee', 'integrations', 'steps', 'showcase', 'stats', 'testimonials', 'comparison', 'faq', 'newsletter', 'footer'] },
  { id: 'launch', category: '营销', name: '产品发布页', description: '适合新品发布、等待名单和活动转化', icon: 'L', palette: ['#172554', '#2563EB', '#DBEAFE'], blocks: ['announcement', 'navbar', 'hero-centered', 'showcase', 'features', 'stats', 'testimonials', 'newsletter', 'cta', 'footer-minimal'] },
  { id: 'business', category: '企业', name: '企业服务站', description: '品牌可信度、服务能力、团队和销售线索', icon: 'B', palette: ['#1C1917', '#B45309', '#FEF3C7'], blocks: ['navbar', 'hero', 'logo-cloud', 'stats', 'features', 'case-studies', 'steps', 'team', 'faq', 'contact', 'footer'] },
  { id: 'creative', category: '品牌', name: '创意工作室', description: '大胆首屏、作品画廊、案例与品牌观点', icon: '✦', palette: ['#18181B', '#F43F5E', '#FDE047'], blocks: ['navbar-minimal', 'hero-centered', 'brand-marquee', 'gallery', 'case-studies', 'spotlight', 'testimonials', 'contact', 'footer-minimal'] },
  { id: 'portfolio', category: '个人', name: '个人作品集', description: '个人定位、精选项目、经历与联系入口', icon: 'P', palette: ['#FAFAF9', '#0F766E', '#CCFBF1'], blocks: ['navbar-minimal', 'hero', 'stats', 'gallery', 'case-studies', 'testimonials', 'blog', 'contact', 'footer-minimal'] },
  { id: 'mobile-app', category: '产品', name: '移动应用', description: '应用界面展示、亮点功能、评价和下载转化', icon: '▯', palette: ['#0F172A', '#06B6D4', '#CFFAFE'], blocks: ['announcement', 'navbar', 'hero', 'logo-cloud', 'showcase', 'features', 'steps', 'testimonials', 'pricing', 'faq', 'cta', 'footer-minimal'] }
];

type PresetPart = {
  key: string;
  type: WebComponentType;
  name: string;
  frame: [number, number, number, number];
  mobile: [number, number, number, number, boolean?];
  content?: string;
  style?: WebComponentStyle;
  mobileStyle?: WebComponentStyle;
  parent?: string;
};

const whiteSection: WebComponentStyle = { background: '#FFFFFF', borderColor: '#E5E5EA', borderWidth: 1, borderRadius: 24 };
const darkHeading: WebComponentStyle = { color: '#1D1D1F', fontSize: 38, fontWeight: 800, textAlign: 'left' };
const antdPlaceholderImage = 'https://images.unsplash.com/photo-1550745165-9bc0b252726f?auto=format&fit=crop&w=900&q=80';

function antDesignBinding(part: PresetPart): WebDesignLibraryBinding | undefined {
  const propsForContentList = () => part.content?.split('\n').filter(Boolean) ?? [];
  switch (part.type) {
    case 'heading': return { name: 'antd', version: '6.6.2', component: 'Typography', variant: 'title', props: { level: 2 } };
    case 'text': return { name: 'antd', version: '6.6.2', component: 'Typography', variant: 'paragraph', props: {} };
    case 'logo': return { name: 'antd', version: '6.6.2', component: 'Typography', variant: 'title', props: { level: 3 } };
    case 'button': return { name: 'antd', version: '6.6.2', component: 'Button', variant: part.style?.background === '#FFFFFF' ? 'default' : 'primary', props: { color: part.style?.background === '#FFFFFF' ? 'default' : 'primary', variant: part.style?.background === '#FFFFFF' ? 'outlined' : 'solid', size: 'large', danger: false } };
    case 'link': return { name: 'antd', version: '6.6.2', component: 'Button', variant: 'link', props: { color: 'primary', variant: 'link' } };
    case 'card': return { name: 'antd', version: '6.6.2', component: 'Card', variant: 'default', props: { bordered: true } };
    case 'input': return { name: 'antd', version: '6.6.2', component: 'Input', variant: 'outlined', props: { variant: 'outlined', allowClear: true } };
    case 'textarea': return { name: 'antd', version: '6.6.2', component: 'Input', variant: 'textarea', props: { variant: 'outlined', autoSize: { minRows: 4, maxRows: 8 } } };
    case 'select': return { name: 'antd', version: '6.6.2', component: 'Select', variant: 'outlined', props: { variant: 'outlined', options: propsForContentList().map((label) => ({ label, value: label })) } };
    case 'badge': return { name: 'antd', version: '6.6.2', component: 'Tag', variant: 'blue', props: { color: 'blue' } };
    case 'avatar': return { name: 'antd', version: '6.6.2', component: 'Avatar', variant: 'circle', props: { shape: 'circle', size: 48 } };
    case 'list': return { name: 'antd', version: '6.6.2', component: 'List', variant: 'basic', props: { bordered: false, dataSource: propsForContentList() } };
    case 'image': return { name: 'antd', version: '6.6.2', component: 'Image', variant: 'basic', props: { src: part.content || antdPlaceholderImage, preview: false } };
    default: return undefined;
  }
}

function presetParts(id: WebDesignBlockPresetId): PresetPart[] {
  switch (id) {
    case 'announcement':
      return [
        { key: 'root', type: 'section', name: '公告横幅', frame: [60, 0, 1080, 52], mobile: [16, 0, 358, 58], style: { background: 'linear-gradient(90deg,#111827,#312E81,#111827)', borderRadius: 16 } },
        { key: 'badge', type: 'badge', name: '公告标签', frame: [80, 10, 86, 32], mobile: [28, 13, 72, 32], content: 'NEW', style: { background: '#FFFFFF', color: '#4338CA', borderRadius: 999, fontSize: 12, fontWeight: 800, textAlign: 'center' }, parent: 'root' },
        { key: 'copy', type: 'text', name: '公告内容', frame: [182, 10, 720, 32], mobile: [112, 13, 200, 32], content: '全新 AI 工作流现已开放体验', style: { color: '#FFFFFF', fontSize: 14, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'link', type: 'link', name: '公告链接', frame: [920, 10, 190, 32], mobile: [313, 13, 45, 32], content: '了解更多 →', style: { color: '#C7D2FE', fontSize: 14, fontWeight: 700, textAlign: 'right' }, parent: 'root' }
      ];
    case 'navbar':
      return [
        { key: 'root', type: 'section', name: '网站导航栏', frame: [60, 0, 1080, 76], mobile: [16, 0, 358, 68], style: { ...whiteSection, borderRadius: 18 } },
        { key: 'logo', type: 'logo', name: '品牌 Logo', frame: [88, 14, 150, 48], mobile: [32, 10, 130, 48], content: 'NOVA', parent: 'root' },
        { key: 'home', type: 'link', name: '首页链接', frame: [520, 18, 80, 40], mobile: [180, 14, 1, 1, true], content: '首页', style: { color: '#4B465D', fontSize: 15, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'features', type: 'link', name: '功能链接', frame: [610, 18, 80, 40], mobile: [181, 14, 1, 1, true], content: '功能', style: { color: '#4B465D', fontSize: 15, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'pricing', type: 'link', name: '价格链接', frame: [700, 18, 80, 40], mobile: [182, 14, 1, 1, true], content: '价格', style: { color: '#4B465D', fontSize: 15, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'action', type: 'button', name: '导航行动按钮', frame: [930, 14, 180, 48], mobile: [232, 10, 126, 48], content: '免费开始', parent: 'root' }
      ];
    case 'navbar-minimal':
      return [
        { key: 'root', type: 'section', name: '极简导航', frame: [60, 0, 1080, 72], mobile: [16, 0, 358, 66], style: { background: 'rgba(255,255,255,.9)', borderColor: '#E7E5E4', borderWidth: 1, borderRadius: 999, shadow: '0 14px 40px rgba(15,23,42,.08)' } },
        { key: 'logo', type: 'logo', name: '品牌标志', frame: [88, 12, 180, 48], mobile: [32, 9, 150, 48], content: 'STUDIO / 26', style: { color: '#18181B', fontSize: 18, fontWeight: 900 }, parent: 'root' },
        { key: 'work', type: 'link', name: '作品链接', frame: [710, 16, 90, 40], mobile: [188, 13, 1, 1, true], content: '作品', style: { color: '#52525B', fontSize: 14, fontWeight: 650, textAlign: 'center' }, parent: 'root' },
        { key: 'about', type: 'link', name: '介绍链接', frame: [805, 16, 90, 40], mobile: [189, 13, 1, 1, true], content: '关于', style: { color: '#52525B', fontSize: 14, fontWeight: 650, textAlign: 'center' }, parent: 'root' },
        { key: 'action', type: 'button', name: '联系按钮', frame: [930, 12, 160, 48], mobile: [230, 9, 128, 48], content: '开始合作 ↗', style: { background: '#18181B', color: '#FFFFFF', borderRadius: 999, fontSize: 14, fontWeight: 700 }, parent: 'root' }
      ];
    case 'hero':
      return [
        { key: 'root', type: 'section', name: 'Hero 首屏', frame: [60, 0, 1080, 520], mobile: [16, 0, 358, 760], style: { background: 'linear-gradient(145deg,#F5F5F7 0%,#EAF3FF 100%)', borderRadius: 32 } },
        { key: 'badge', type: 'badge', name: '首屏徽章', frame: [110, 62, 150, 32], mobile: [32, 40, 150, 32], content: 'AI 原生产品', parent: 'root' },
        { key: 'heading', type: 'heading', name: '首屏标题', frame: [110, 112, 540, 150], mobile: [32, 92, 326, 170], content: '把创意变成\n真正可用的产品', mobileStyle: { fontSize: 40 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '首屏说明', frame: [110, 280, 520, 86], mobile: [32, 280, 326, 105], content: '从设计、内容到交付，让 AI 和团队在同一个可编辑画布中持续迭代。', style: { color: '#6E6E73', fontSize: 19, fontWeight: 450 }, mobileStyle: { fontSize: 17 }, parent: 'root' },
        { key: 'primary', type: 'button', name: '首屏主按钮', frame: [110, 398, 176, 52], mobile: [32, 415, 156, 52], content: '立即体验', parent: 'root' },
        { key: 'secondary', type: 'button', name: '首屏次按钮', frame: [302, 398, 176, 52], mobile: [202, 415, 156, 52], content: '查看案例', style: { background: '#FFFFFF', color: '#007AFF', borderColor: '#D1D1D6', borderWidth: 1, borderRadius: 12, fontSize: 16, fontWeight: 650 }, parent: 'root' },
        { key: 'visual', type: 'image', name: '首屏主视觉', frame: [720, 70, 350, 380], mobile: [32, 505, 326, 215], parent: 'root' }
      ];
    case 'hero-centered':
      return [
        { key: 'root', type: 'section', name: '居中渐变首屏', frame: [60, 0, 1080, 650], mobile: [16, 0, 358, 720], style: { background: 'radial-gradient(circle at 50% 18%,rgba(139,92,246,.34),transparent 36%),linear-gradient(160deg,#09090B 0%,#18113A 52%,#081B2A 100%)', borderRadius: 36, shadow: 'inset 0 0 0 1px rgba(255,255,255,.09)' } },
        { key: 'badge', type: 'badge', name: '首屏状态', frame: [450, 62, 300, 34], mobile: [72, 46, 246, 34], content: '✦ 由想法直接抵达可用产品', style: { background: 'rgba(255,255,255,.1)', color: '#DDD6FE', borderColor: 'rgba(255,255,255,.18)', borderWidth: 1, borderRadius: 999, fontSize: 13, fontWeight: 700, textAlign: 'center' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '居中首屏标题', frame: [220, 126, 760, 150], mobile: [32, 112, 326, 185], content: '把复杂能力，变成\n令人着迷的体验', style: { color: '#FFFFFF', fontSize: 62, fontWeight: 900, textAlign: 'center' }, mobileStyle: { fontSize: 40 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '居中首屏说明', frame: [330, 300, 540, 70], mobile: [48, 316, 294, 95], content: '一个为 AI 时代打造的设计工作台，让团队更快形成共识、验证想法并持续交付。', style: { color: '#A1A1AA', fontSize: 18, fontWeight: 450, textAlign: 'center' }, mobileStyle: { fontSize: 16 }, parent: 'root' },
        { key: 'primary', type: 'button', name: '主要行动', frame: [410, 398, 180, 54], mobile: [42, 438, 146, 52], content: '开始创造', style: { background: '#FFFFFF', color: '#18181B', borderRadius: 999, fontSize: 16, fontWeight: 750 }, parent: 'root' },
        { key: 'secondary', type: 'button', name: '次要行动', frame: [610, 398, 180, 54], mobile: [202, 438, 146, 52], content: '观看演示', style: { background: 'rgba(255,255,255,.08)', color: '#FFFFFF', borderColor: 'rgba(255,255,255,.22)', borderWidth: 1, borderRadius: 999, fontSize: 16, fontWeight: 700 }, parent: 'root' },
        { key: 'metric-one', type: 'card', name: '悬浮数据一', frame: [155, 510, 260, 90], mobile: [32, 536, 154, 120], content: '12×\n更快完成设计', style: { background: 'rgba(255,255,255,.08)', color: '#FFFFFF', borderColor: 'rgba(255,255,255,.15)', borderWidth: 1, borderRadius: 20, fontSize: 18, fontWeight: 750 }, parent: 'root' },
        { key: 'metric-two', type: 'card', name: '悬浮数据二', frame: [470, 490, 260, 110], mobile: [204, 516, 154, 140], content: '98%\n布局保持一致', style: { background: 'linear-gradient(145deg,#7C3AED,#2563EB)', color: '#FFFFFF', borderRadius: 22, fontSize: 20, fontWeight: 800, shadow: '0 22px 50px rgba(79,70,229,.35)' }, parent: 'root' },
        { key: 'metric-three', type: 'card', name: '悬浮数据三', frame: [785, 510, 260, 90], mobile: [120, 674, 150, 1, true], content: '24/7\nAI 共同创作', style: { background: 'rgba(255,255,255,.08)', color: '#FFFFFF', borderColor: 'rgba(255,255,255,.15)', borderWidth: 1, borderRadius: 20, fontSize: 18, fontWeight: 750 }, parent: 'root' }
      ];
    case 'hero-product':
      return [
        { key: 'root', type: 'section', name: '产品界面首屏', frame: [60, 0, 1080, 760], mobile: [16, 0, 358, 820], style: { background: 'linear-gradient(180deg,#F8FAFF 0%,#EEF2FF 68%,#FFFFFF 100%)', borderRadius: 36 } },
        { key: 'badge', type: 'badge', name: '产品版本标签', frame: [485, 55, 230, 32], mobile: [80, 40, 230, 32], content: 'PRODUCT DESIGN / 2026', style: { background: '#E0E7FF', color: '#4338CA', borderRadius: 999, fontSize: 12, fontWeight: 800, textAlign: 'center' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '产品首屏标题', frame: [220, 110, 760, 130], mobile: [32, 92, 326, 150], content: '设计、协作与交付\n都在一个画布里', style: { color: '#111827', fontSize: 54, fontWeight: 900, textAlign: 'center' }, mobileStyle: { fontSize: 37 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '产品首屏说明', frame: [330, 258, 540, 62], mobile: [46, 262, 298, 85], content: '从第一条 AI 指令到像素级调整，让设计真正成为团队共享的产品语言。', style: { color: '#64748B', fontSize: 18, fontWeight: 450, textAlign: 'center' }, mobileStyle: { fontSize: 16 }, parent: 'root' },
        { key: 'action', type: 'button', name: '产品首屏按钮', frame: [500, 344, 200, 54], mobile: [102, 372, 186, 52], content: '免费开始设计', style: { background: '#4F46E5', color: '#FFFFFF', borderRadius: 999, fontSize: 16, fontWeight: 750, shadow: '0 12px 28px rgba(79,70,229,.28)' }, parent: 'root' },
        { key: 'visual', type: 'image', name: '大型产品界面', frame: [145, 440, 910, 270], mobile: [32, 460, 326, 290], content: 'https://images.unsplash.com/photo-1551288049-bebda4e38f71?auto=format&fit=crop&w=1600&q=85', style: { borderRadius: 20, shadow: '0 28px 70px rgba(30,41,59,.22)' }, parent: 'root' }
      ];
    case 'logo-cloud':
      return [
        { key: 'root', type: 'section', name: '客户品牌墙', frame: [60, 0, 1080, 210], mobile: [16, 0, 358, 330], style: { background: '#FFFFFF', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 24 } },
        { key: 'label', type: 'text', name: '品牌墙说明', frame: [300, 32, 600, 28], mobile: [48, 28, 294, 42], content: '受到全球创新团队的信任', style: { color: '#9CA3AF', fontSize: 13, fontWeight: 700, textAlign: 'center' }, parent: 'root' },
        { key: 'one', type: 'logo', name: '客户品牌一', frame: [110, 100, 150, 48], mobile: [32, 92, 145, 48], content: 'APEX', style: { color: '#111827', fontSize: 21, fontWeight: 900, textAlign: 'center' }, parent: 'root' },
        { key: 'two', type: 'logo', name: '客户品牌二', frame: [315, 100, 150, 48], mobile: [213, 92, 145, 48], content: 'LUMA', style: { color: '#475569', fontSize: 21, fontWeight: 850, textAlign: 'center' }, parent: 'root' },
        { key: 'three', type: 'logo', name: '客户品牌三', frame: [525, 100, 150, 48], mobile: [32, 168, 145, 48], content: 'NORTH', style: { color: '#111827', fontSize: 21, fontWeight: 900, textAlign: 'center' }, parent: 'root' },
        { key: 'four', type: 'logo', name: '客户品牌四', frame: [735, 100, 150, 48], mobile: [213, 168, 145, 48], content: 'ARC°', style: { color: '#475569', fontSize: 21, fontWeight: 850, textAlign: 'center' }, parent: 'root' },
        { key: 'five', type: 'logo', name: '客户品牌五', frame: [940, 100, 150, 48], mobile: [123, 244, 145, 48], content: 'KIN', style: { color: '#111827', fontSize: 21, fontWeight: 900, textAlign: 'center' }, parent: 'root' }
      ];
    case 'brand-marquee':
      return [
        { key: 'root', type: 'section', name: '品牌跑马灯', frame: [0, 0, 1200, 128], mobile: [0, 0, 390, 184], style: { background: '#18181B', borderRadius: 0 } },
        { key: 'top', type: 'text', name: '跑马灯上行', frame: [20, 18, 1160, 38], mobile: [12, 20, 366, 58], content: 'DESIGN SYSTEM  ✦  AI WORKFLOW  ✦  RESPONSIVE  ✦  PROTOTYPING  ✦  HUMAN CRAFT', style: { color: '#FAFAFA', fontSize: 22, fontWeight: 850, textAlign: 'center' }, mobileStyle: { fontSize: 17 }, parent: 'root' },
        { key: 'bottom', type: 'text', name: '跑马灯下行', frame: [20, 74, 1160, 36], mobile: [12, 100, 366, 60], content: 'APPLE STYLE  —  ANT DESIGN  —  CHAKRA UI  —  SHADCN/UI  —  OPEN CANVAS', style: { color: '#A1A1AA', fontSize: 17, fontWeight: 700, textAlign: 'center' }, mobileStyle: { fontSize: 15 }, parent: 'root' }
      ];
    case 'stats':
      return [
        { key: 'root', type: 'section', name: '关键数据区', frame: [60, 0, 1080, 300], mobile: [16, 0, 358, 590], style: { background: '#0F172A', borderRadius: 30 } },
        { key: 'heading', type: 'heading', name: '数据区标题', frame: [105, 44, 420, 82], mobile: [32, 34, 326, 88], content: '真实结果，\n不是漂亮口号', style: { color: '#FFFFFF', fontSize: 34, fontWeight: 850 }, mobileStyle: { fontSize: 30 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '数据区说明', frame: [105, 150, 410, 78], mobile: [32, 132, 326, 72], content: '把复杂工作流变得更快、更清晰，也更容易被整个团队采用。', style: { color: '#94A3B8', fontSize: 16, fontWeight: 450 }, parent: 'root' },
        { key: 'one', type: 'card', name: '效率指标', frame: [575, 52, 145, 176], mobile: [32, 230, 154, 145], content: '12×\n设计效率', style: { background: '#1E293B', color: '#67E8F9', borderRadius: 20, fontSize: 23, fontWeight: 850 }, parent: 'root' },
        { key: 'two', type: 'card', name: '一致性指标', frame: [745, 52, 145, 176], mobile: [204, 230, 154, 145], content: '98%\n还原一致', style: { background: '#164E63', color: '#ECFEFF', borderRadius: 20, fontSize: 23, fontWeight: 850 }, parent: 'root' },
        { key: 'three', type: 'card', name: '用户指标', frame: [915, 52, 145, 176], mobile: [32, 395, 326, 145], content: '24K+\n团队正在共同创造', style: { background: 'linear-gradient(145deg,#0891B2,#2563EB)', color: '#FFFFFF', borderRadius: 20, fontSize: 23, fontWeight: 850, shadow: '0 18px 40px rgba(8,145,178,.25)' }, parent: 'root' }
      ];
    case 'showcase':
      return [
        { key: 'root', type: 'section', name: '产品聚光展示', frame: [60, 0, 1080, 680], mobile: [16, 0, 358, 830], style: { background: 'linear-gradient(160deg,#ECFEFF,#EEF2FF 55%,#FAF5FF)', borderRadius: 34 } },
        { key: 'eyebrow', type: 'badge', name: '展示标签', frame: [90, 54, 150, 30], mobile: [32, 38, 150, 30], content: 'LIVE CANVAS', style: { background: '#FFFFFF', color: '#4F46E5', borderRadius: 999, fontSize: 11, fontWeight: 800, textAlign: 'center' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '展示标题', frame: [90, 102, 560, 105], mobile: [32, 88, 326, 120], content: '每一个细节，\n都可以继续编辑', style: { color: '#111827', fontSize: 44, fontWeight: 900 }, mobileStyle: { fontSize: 34 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '展示说明', frame: [90, 225, 500, 80], mobile: [32, 222, 326, 90], content: '组件、布局、内容和响应式规则保持结构化，让人和 AI 都能准确理解当前设计。', style: { color: '#64748B', fontSize: 17, fontWeight: 450 }, parent: 'root' },
        { key: 'visual', type: 'image', name: '产品展示图', frame: [90, 345, 900, 280], mobile: [32, 338, 326, 310], content: 'https://images.unsplash.com/photo-1559028012-481c04fa702d?auto=format&fit=crop&w=1600&q=85', style: { borderRadius: 22, shadow: '0 28px 70px rgba(67,56,202,.2)' }, parent: 'root' },
        { key: 'float', type: 'card', name: '浮层信息卡', frame: [770, 245, 240, 150], mobile: [70, 670, 250, 120], content: '响应式状态\nDesktop · Tablet · Mobile\n全部保持可编辑', style: { background: 'rgba(255,255,255,.92)', color: '#111827', borderColor: '#E0E7FF', borderWidth: 1, borderRadius: 18, fontSize: 15, fontWeight: 700, shadow: '0 18px 45px rgba(30,41,59,.18)' }, parent: 'root' }
      ];
    case 'bento':
      return [
        { key: 'root', type: 'section', name: 'Bento 特性墙', frame: [60, 0, 1080, 690], mobile: [16, 0, 358, 1110], style: { background: '#F8FAFC', borderRadius: 32 } },
        { key: 'heading', type: 'heading', name: 'Bento 标题', frame: [100, 50, 700, 82], mobile: [32, 38, 326, 105], content: '一套系统，覆盖完整设计过程', style: { color: '#0F172A', fontSize: 40, fontWeight: 900 }, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'large', type: 'card', name: 'Bento 主卡片', frame: [100, 160, 510, 460], mobile: [32, 164, 326, 300], content: '✦ AI 共同设计\n\n选择整个页面或某个具体组件，让 AI 在不破坏其他内容的前提下继续设计。', style: { background: 'linear-gradient(145deg,#312E81,#4F46E5)', color: '#FFFFFF', borderRadius: 26, fontSize: 23, fontWeight: 800, shadow: '0 24px 54px rgba(79,70,229,.24)' }, parent: 'root' },
        { key: 'top', type: 'card', name: 'Bento 响应式卡片', frame: [640, 160, 360, 210], mobile: [32, 486, 326, 190], content: '▰ 响应式画布\n\n从 390px 到 8K，检查并独立调整每个断点。', style: { background: '#FFFFFF', color: '#0F172A', borderColor: '#E2E8F0', borderWidth: 1, borderRadius: 24, fontSize: 18, fontWeight: 750 }, parent: 'root' },
        { key: 'bottom-left', type: 'card', name: 'Bento 组件卡片', frame: [640, 395, 170, 225], mobile: [32, 698, 154, 330], content: '72+\nAnt Design\n\n114+\nChakra UI', style: { background: '#0F172A', color: '#F8FAFC', borderRadius: 22, fontSize: 17, fontWeight: 800 }, parent: 'root' },
        { key: 'bottom-right', type: 'card', name: 'Bento 交互卡片', frame: [830, 395, 170, 225], mobile: [204, 698, 154, 330], content: '⌁\n真实交互\n\n抽屉、弹窗、选择器和页面跳转', style: { background: '#CCFBF1', color: '#115E59', borderRadius: 22, fontSize: 17, fontWeight: 800 }, parent: 'root' }
      ];
    case 'features':
      return [
        { key: 'root', type: 'section', name: '功能介绍区', frame: [60, 0, 1080, 430], mobile: [16, 0, 358, 760], style: { background: '#F8FAFC', borderRadius: 28 } },
        { key: 'heading', type: 'heading', name: '功能区标题', frame: [100, 46, 620, 72], mobile: [32, 36, 326, 88], content: '为真实工作流而设计', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'one', type: 'card', name: '功能卡片一', frame: [100, 145, 300, 220], mobile: [32, 142, 326, 170], content: '✦  AI 共同设计\n从一句需求开始，持续修改具体组件。', parent: 'root' },
        { key: 'two', type: 'card', name: '功能卡片二', frame: [450, 145, 300, 220], mobile: [32, 330, 326, 170], content: '▣  可视化编辑\n拖动、缩放、布局和响应式调整都可直接完成。', parent: 'root' },
        { key: 'three', type: 'card', name: '功能卡片三', frame: [800, 145, 300, 220], mobile: [32, 518, 326, 170], content: '⌘  代码交付\n导出 HTML、React 或 Vue，继续进入开发流程。', parent: 'root' }
      ];
    case 'steps':
      return [
        { key: 'root', type: 'section', name: '使用流程区', frame: [60, 0, 1080, 470], mobile: [16, 0, 358, 810], style: { background: '#FFFFFF', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 30 } },
        { key: 'label', type: 'badge', name: '流程标签', frame: [90, 48, 130, 30], mobile: [32, 36, 130, 30], content: 'HOW IT WORKS', style: { background: '#ECFDF5', color: '#047857', borderRadius: 999, fontSize: 11, fontWeight: 800, textAlign: 'center' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '流程标题', frame: [90, 92, 620, 72], mobile: [32, 84, 326, 90], content: '从想法到完成，只需三步', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'one', type: 'card', name: '第一步', frame: [90, 205, 290, 190], mobile: [32, 196, 326, 170], content: '01\n描述目标\n\n告诉 AI 网站面向谁、要解决什么问题。', style: { background: '#F8FAFC', color: '#0F172A', borderRadius: 22, fontSize: 17, fontWeight: 750 }, parent: 'root' },
        { key: 'two', type: 'card', name: '第二步', frame: [455, 185, 290, 210], mobile: [32, 388, 326, 170], content: '02\n共同设计\n\n拖动、调整、批注，让 AI 精确修改选中区域。', style: { background: '#0F766E', color: '#FFFFFF', borderRadius: 22, fontSize: 17, fontWeight: 750, shadow: '0 20px 40px rgba(15,118,110,.2)' }, parent: 'root' },
        { key: 'three', type: 'card', name: '第三步', frame: [820, 205, 290, 190], mobile: [32, 580, 326, 170], content: '03\n验证体验\n\n检查响应式与真实交互，保存完整设计结果。', style: { background: '#F8FAFC', color: '#0F172A', borderRadius: 22, fontSize: 17, fontWeight: 750 }, parent: 'root' }
      ];
    case 'integrations':
      return [
        { key: 'root', type: 'section', name: '集成生态区', frame: [60, 0, 1080, 500], mobile: [16, 0, 358, 720], style: { background: 'radial-gradient(circle at center,#EDE9FE 0%,#F8FAFC 44%,#FFFFFF 75%)', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 32 } },
        { key: 'heading', type: 'heading', name: '集成标题', frame: [300, 48, 600, 72], mobile: [32, 38, 326, 100], content: '连接你已经在使用的工具', style: { color: '#111827', fontSize: 38, fontWeight: 900, textAlign: 'center' }, mobileStyle: { fontSize: 31 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '集成说明', frame: [330, 128, 540, 54], mobile: [44, 144, 302, 76], content: '用开放的数据结构贯穿设计、内容、评审和开发工作流。', style: { color: '#6B7280', fontSize: 16, fontWeight: 450, textAlign: 'center' }, parent: 'root' },
        { key: 'center', type: 'card', name: '中心产品', frame: [480, 225, 240, 190], mobile: [78, 256, 234, 180], content: 'W\nWEB DESIGN\nSTUDIO', style: { background: 'linear-gradient(145deg,#6366F1,#7C3AED)', color: '#FFFFFF', borderRadius: 32, fontSize: 22, fontWeight: 900, textAlign: 'center', shadow: '0 24px 55px rgba(99,102,241,.3)' }, parent: 'root' },
        { key: 'one', type: 'card', name: 'React 集成', frame: [120, 210, 160, 96], mobile: [32, 466, 154, 92], content: '⚛ React', style: { background: '#FFFFFF', color: '#0369A1', borderRadius: 20, fontSize: 17, fontWeight: 800, shadow: '0 12px 30px rgba(15,23,42,.08)' }, parent: 'root' },
        { key: 'two', type: 'card', name: 'Vue 集成', frame: [290, 330, 160, 96], mobile: [204, 466, 154, 92], content: '◆ Vue', style: { background: '#FFFFFF', color: '#047857', borderRadius: 20, fontSize: 17, fontWeight: 800, shadow: '0 12px 30px rgba(15,23,42,.08)' }, parent: 'root' },
        { key: 'three', type: 'card', name: 'Figma 集成', frame: [750, 330, 160, 96], mobile: [32, 578, 154, 92], content: '◉ Figma', style: { background: '#FFFFFF', color: '#C2410C', borderRadius: 20, fontSize: 17, fontWeight: 800, shadow: '0 12px 30px rgba(15,23,42,.08)' }, parent: 'root' },
        { key: 'four', type: 'card', name: 'API 集成', frame: [920, 210, 160, 96], mobile: [204, 578, 154, 92], content: '{ } API', style: { background: '#FFFFFF', color: '#7C3AED', borderRadius: 20, fontSize: 17, fontWeight: 800, shadow: '0 12px 30px rgba(15,23,42,.08)' }, parent: 'root' }
      ];
    case 'testimonials':
      return [
        { key: 'root', type: 'section', name: '用户评价区', frame: [60, 0, 1080, 520], mobile: [16, 0, 358, 850], style: { background: '#18181B', borderRadius: 32 } },
        { key: 'heading', type: 'heading', name: '评价标题', frame: [95, 48, 700, 82], mobile: [32, 38, 326, 105], content: '被认真做产品的人选择', style: { color: '#FFFFFF', fontSize: 40, fontWeight: 900 }, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'featured', type: 'card', name: '重点评价', frame: [95, 165, 520, 285], mobile: [32, 166, 326, 250], content: '“第一次感觉 AI 真正理解了设计上下文。它没有重做整个页面，而是准确修改了我们选中的部分。”\n\n林默 · 产品设计负责人', style: { background: 'linear-gradient(145deg,#312E81,#6D28D9)', color: '#FFFFFF', borderRadius: 26, fontSize: 22, fontWeight: 700, shadow: '0 24px 50px rgba(109,40,217,.25)' }, parent: 'root' },
        { key: 'one', type: 'card', name: '用户评价二', frame: [645, 165, 340, 128], mobile: [32, 438, 326, 160], content: '“设计保存后完全没有漂移，这让我们终于敢把它用于真实项目。”\n\n陈屿 · 创始人', style: { background: '#27272A', color: '#F4F4F5', borderColor: '#3F3F46', borderWidth: 1, borderRadius: 22, fontSize: 16, fontWeight: 650 }, parent: 'root' },
        { key: 'two', type: 'card', name: '用户评价三', frame: [645, 318, 340, 132], mobile: [32, 620, 326, 160], content: '“组件库、响应式和人工微调可以共存，设计效率提升非常明显。”\n\n周行 · 前端负责人', style: { background: '#27272A', color: '#F4F4F5', borderColor: '#3F3F46', borderWidth: 1, borderRadius: 22, fontSize: 16, fontWeight: 650 }, parent: 'root' }
      ];
    case 'case-studies':
      return [
        { key: 'root', type: 'section', name: '客户案例区', frame: [60, 0, 1080, 660], mobile: [16, 0, 358, 1120], style: { background: '#FFFFFF', borderRadius: 30 } },
        { key: 'label', type: 'badge', name: '案例标签', frame: [90, 44, 120, 30], mobile: [32, 34, 120, 30], content: 'CASE STUDIES', style: { background: '#FEF3C7', color: '#92400E', borderRadius: 999, fontSize: 11, fontWeight: 800, textAlign: 'center' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '案例标题', frame: [90, 92, 650, 82], mobile: [32, 82, 326, 108], content: '看看不同团队如何创造价值', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'one-image', type: 'image', name: '案例一图片', frame: [90, 205, 470, 250], mobile: [32, 212, 326, 220], content: 'https://images.unsplash.com/photo-1551434678-e076c223a692?auto=format&fit=crop&w=1200&q=85', style: { borderRadius: 22 }, parent: 'root' },
        { key: 'one', type: 'card', name: '案例一信息', frame: [90, 475, 470, 120], mobile: [32, 450, 326, 150], content: 'NORTH / 协作平台\n上线时间缩短 62% · 转化提升 31%', style: { background: '#F8FAFC', color: '#0F172A', borderRadius: 18, fontSize: 18, fontWeight: 750 }, parent: 'root' },
        { key: 'two-image', type: 'image', name: '案例二图片', frame: [590, 205, 430, 250], mobile: [32, 626, 326, 220], content: 'https://images.unsplash.com/photo-1497366754035-f200968a6e72?auto=format&fit=crop&w=1200&q=85', style: { borderRadius: 22 }, parent: 'root' },
        { key: 'two', type: 'card', name: '案例二信息', frame: [590, 475, 430, 120], mobile: [32, 864, 326, 150], content: 'ARC / 创意工作室\n设计评审效率提升 3.4 倍', style: { background: '#18181B', color: '#FFFFFF', borderRadius: 18, fontSize: 18, fontWeight: 750 }, parent: 'root' }
      ];
    case 'gallery':
      return [
        { key: 'root', type: 'section', name: '视觉画廊', frame: [60, 0, 1080, 720], mobile: [16, 0, 358, 1280], style: { background: '#F5F5F4', borderRadius: 30 } },
        { key: 'heading', type: 'heading', name: '画廊标题', frame: [90, 48, 680, 82], mobile: [32, 38, 326, 110], content: '精选项目 / 2026', style: { color: '#1C1917', fontSize: 42, fontWeight: 900 }, mobileStyle: { fontSize: 33 }, parent: 'root' },
        { key: 'one', type: 'image', name: '画廊图片一', frame: [90, 160, 560, 310], mobile: [32, 168, 326, 280], content: 'https://images.unsplash.com/photo-1618005182384-a83a8bd57fbe?auto=format&fit=crop&w=1200&q=85', style: { borderRadius: 22 }, parent: 'root' },
        { key: 'one-copy', type: 'text', name: '作品一说明', frame: [90, 486, 560, 48], mobile: [32, 464, 326, 55], content: 'AURORA — 数字品牌体验', style: { color: '#1C1917', fontSize: 17, fontWeight: 750 }, parent: 'root' },
        { key: 'two', type: 'image', name: '画廊图片二', frame: [680, 160, 310, 200], mobile: [32, 548, 326, 250], content: 'https://images.unsplash.com/photo-1614850523459-c2f4c699c52e?auto=format&fit=crop&w=900&q=85', style: { borderRadius: 22 }, parent: 'root' },
        { key: 'two-copy', type: 'text', name: '作品二说明', frame: [680, 376, 310, 48], mobile: [32, 814, 326, 55], content: 'KINETIC — 视觉系统', style: { color: '#1C1917', fontSize: 16, fontWeight: 750 }, parent: 'root' },
        { key: 'three', type: 'image', name: '画廊图片三', frame: [680, 455, 310, 190], mobile: [32, 898, 326, 250], content: 'https://images.unsplash.com/photo-1634017839464-5c339ebe3cb4?auto=format&fit=crop&w=900&q=85', style: { borderRadius: 22 }, parent: 'root' },
        { key: 'three-copy', type: 'text', name: '作品三说明', frame: [680, 656, 310, 34], mobile: [32, 1164, 326, 55], content: 'FORM / 空间与界面', style: { color: '#1C1917', fontSize: 16, fontWeight: 750 }, parent: 'root' }
      ];
    case 'blog':
      return [
        { key: 'root', type: 'section', name: '内容精选区', frame: [60, 0, 1080, 570], mobile: [16, 0, 358, 1120], style: { background: '#FFFFFF', borderRadius: 30 } },
        { key: 'heading', type: 'heading', name: '内容标题', frame: [90, 44, 600, 76], mobile: [32, 34, 326, 92], content: '观点与产品更新', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'more', type: 'link', name: '查看全部文章', frame: [840, 54, 170, 42], mobile: [188, 36, 170, 42], content: '查看全部文章 →', style: { color: '#2563EB', fontSize: 14, fontWeight: 700, textAlign: 'right' }, parent: 'root' },
        { key: 'one-image', type: 'image', name: '文章一封面', frame: [90, 155, 290, 190], mobile: [32, 150, 326, 210], content: 'https://images.unsplash.com/photo-1516321318423-f06f85e504b3?auto=format&fit=crop&w=900&q=85', style: { borderRadius: 18 }, parent: 'root' },
        { key: 'one', type: 'card', name: '文章一卡片', frame: [90, 360, 290, 140], mobile: [32, 376, 326, 150], content: '设计系统\nAI 如何参与真实的设计评审', style: { background: '#F8FAFC', color: '#0F172A', borderRadius: 18, fontSize: 17, fontWeight: 750 }, parent: 'root' },
        { key: 'two-image', type: 'image', name: '文章二封面', frame: [455, 155, 290, 190], mobile: [32, 548, 326, 210], content: 'https://images.unsplash.com/photo-1552664730-d307ca884978?auto=format&fit=crop&w=900&q=85', style: { borderRadius: 18 }, parent: 'root' },
        { key: 'two', type: 'card', name: '文章二卡片', frame: [455, 360, 290, 140], mobile: [32, 774, 326, 150], content: '协作\n让设计决策保留完整上下文', style: { background: '#F8FAFC', color: '#0F172A', borderRadius: 18, fontSize: 17, fontWeight: 750 }, parent: 'root' },
        { key: 'three', type: 'card', name: '文章三卡片', frame: [820, 155, 290, 345], mobile: [32, 946, 326, 130], content: '产品更新 / 06\n\n响应式约束\n多页面设计\n项目工作区\n组件内部编辑\n\n阅读更新日志 →', style: { background: '#111827', color: '#FFFFFF', borderRadius: 22, fontSize: 17, fontWeight: 750 }, parent: 'root' }
      ];
    case 'team':
      return [
        { key: 'root', type: 'section', name: '团队介绍区', frame: [60, 0, 1080, 520], mobile: [16, 0, 358, 1040], style: { background: '#FFF7ED', borderRadius: 32 } },
        { key: 'heading', type: 'heading', name: '团队标题', frame: [90, 48, 680, 82], mobile: [32, 38, 326, 106], content: '小团队，也能创造大产品', style: { color: '#431407', fontSize: 40, fontWeight: 900 }, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '团队说明', frame: [90, 132, 650, 58], mobile: [32, 154, 326, 82], content: '设计、工程与产品在同一上下文里工作，不再把时间浪费在重复解释上。', style: { color: '#9A3412', fontSize: 17, fontWeight: 450 }, parent: 'root' },
        { key: 'one', type: 'card', name: '团队成员一', frame: [90, 230, 285, 220], mobile: [32, 266, 326, 210], content: 'LM\n林默\nProduct & Design', style: { background: '#FFFFFF', color: '#431407', borderRadius: 24, fontSize: 18, fontWeight: 800 }, parent: 'root' },
        { key: 'two', type: 'card', name: '团队成员二', frame: [455, 230, 285, 220], mobile: [32, 498, 326, 210], content: 'CY\n陈屿\nEngineering', style: { background: '#EA580C', color: '#FFFFFF', borderRadius: 24, fontSize: 18, fontWeight: 800, shadow: '0 18px 40px rgba(234,88,12,.2)' }, parent: 'root' },
        { key: 'three', type: 'card', name: '团队成员三', frame: [820, 230, 285, 220], mobile: [32, 730, 326, 210], content: 'ZX\n周行\nAI Systems', style: { background: '#FFFFFF', color: '#431407', borderRadius: 24, fontSize: 18, fontWeight: 800 }, parent: 'root' }
      ];
    case 'pricing':
      return [
        { key: 'root', type: 'section', name: '价格方案区', frame: [60, 0, 1080, 500], mobile: [16, 0, 358, 980], style: { background: '#FFFFFF', borderRadius: 28 } },
        { key: 'heading', type: 'heading', name: '价格区标题', frame: [100, 42, 620, 72], mobile: [32, 34, 326, 88], content: '选择适合你的方案', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'starter', type: 'card', name: '基础版价格卡', frame: [100, 140, 300, 290], mobile: [32, 140, 326, 240], content: '基础版\n¥99 / 月\n\n适合个人项目\n基础组件与导出', parent: 'root' },
        { key: 'pro', type: 'card', name: '专业版价格卡', frame: [450, 125, 300, 320], mobile: [32, 398, 326, 250], content: '专业版 · 推荐\n¥299 / 月\n\n完整组件与 AI 协作\n团队项目和版本管理', style: { background: '#007AFF', color: '#FFFFFF', borderRadius: 20, fontSize: 19, fontWeight: 700, shadow: '0 18px 40px rgba(0,122,255,.24)' }, parent: 'root' },
        { key: 'team', type: 'card', name: '团队版价格卡', frame: [800, 140, 300, 290], mobile: [32, 666, 326, 240], content: '团队版\n联系销售\n\n权限与审计\n定制组件和集成', parent: 'root' }
      ];
    case 'comparison':
      return [
        { key: 'root', type: 'section', name: '方案对比区', frame: [60, 0, 1080, 590], mobile: [16, 0, 358, 880], style: { background: '#FFFFFF', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 30 } },
        { key: 'heading', type: 'heading', name: '方案对比标题', frame: [90, 46, 650, 76], mobile: [32, 36, 326, 100], content: '选择真正适合团队的方式', style: darkHeading, mobileStyle: { fontSize: 31 }, parent: 'root' },
        { key: 'labels', type: 'card', name: '能力名称', frame: [90, 155, 270, 350], mobile: [32, 160, 118, 620], content: '能力\n\n项目管理\n响应式画布\n完整组件库\nAI 组件修改\n设计版本保护\n团队权限', style: { background: '#F8FAFC', color: '#475569', borderRadius: 20, fontSize: 15, fontWeight: 700 }, parent: 'root' },
        { key: 'starter', type: 'card', name: '个人方案对比', frame: [380, 155, 270, 350], mobile: [158, 160, 94, 620], content: '个人\n\n✓\n✓\n基础\n✓\n✓\n—', style: { background: '#FFFFFF', color: '#0F172A', borderColor: '#E2E8F0', borderWidth: 1, borderRadius: 20, fontSize: 15, fontWeight: 750, textAlign: 'center' }, parent: 'root' },
        { key: 'pro', type: 'card', name: '团队方案对比', frame: [670, 135, 320, 390], mobile: [260, 140, 98, 660], content: '团队 · 推荐\n\n✓\n✓\n全部\n✓\n✓\n✓', style: { background: '#111827', color: '#FFFFFF', borderRadius: 22, fontSize: 16, fontWeight: 800, textAlign: 'center', shadow: '0 22px 48px rgba(15,23,42,.22)' }, parent: 'root' },
        { key: 'action', type: 'button', name: '对比行动按钮', frame: [785, 535, 205, 48], mobile: [94, 816, 202, 48], content: '开始团队试用', style: { background: '#2563EB', color: '#FFFFFF', borderRadius: 999, fontSize: 15, fontWeight: 750 }, parent: 'root' }
      ];
    case 'faq':
      return [
        { key: 'root', type: 'section', name: '常见问题区', frame: [160, 0, 880, 530], mobile: [16, 0, 358, 690], style: { background: '#FFFFFF', borderRadius: 28 } },
        { key: 'heading', type: 'heading', name: 'FAQ 标题', frame: [210, 42, 560, 70], mobile: [32, 34, 326, 80], content: '常见问题', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'q1', type: 'card', name: '问题一', frame: [210, 135, 780, 72], mobile: [32, 132, 326, 105], content: '可以直接导出代码吗？\n可以，支持 HTML、React 和 Vue。', parent: 'root' },
        { key: 'q2', type: 'card', name: '问题二', frame: [210, 225, 780, 72], mobile: [32, 253, 326, 105], content: '可以针对某个组件让 AI 修改吗？\n可以，批注和 AI 请求都绑定稳定组件 ID。', parent: 'root' },
        { key: 'q3', type: 'card', name: '问题三', frame: [210, 315, 780, 72], mobile: [32, 374, 326, 105], content: '支持手机和平板吗？\n每个组件都有独立的响应式覆盖。', parent: 'root' },
        { key: 'q4', type: 'card', name: '问题四', frame: [210, 405, 780, 72], mobile: [32, 495, 326, 105], content: '团队修改会互相覆盖吗？\n文档使用 revision 乐观锁保护并发编辑。', parent: 'root' }
      ];
    case 'contact':
      return [
        { key: 'root', type: 'section', name: '联系表单区', frame: [160, 0, 880, 560], mobile: [16, 0, 358, 690], style: { background: 'linear-gradient(145deg,#1D1D1F,#3A3A3C)', borderRadius: 30 } },
        { key: 'heading', type: 'heading', name: '联系标题', frame: [210, 52, 360, 105], mobile: [32, 42, 326, 90], content: '聊聊你的\n下一个产品', style: { color: '#FFFFFF', fontSize: 38, fontWeight: 850 }, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '联系说明', frame: [210, 175, 340, 90], mobile: [32, 145, 326, 75], content: '留下需求和联系方式，我们会尽快回复你。', style: { color: '#D1D1D6', fontSize: 17, fontWeight: 450 }, parent: 'root' },
        { key: 'name', type: 'input', name: '姓名输入框', frame: [610, 58, 330, 52], mobile: [32, 245, 326, 52], content: '你的姓名', parent: 'root' },
        { key: 'email', type: 'input', name: '邮箱输入框', frame: [610, 128, 330, 52], mobile: [32, 315, 326, 52], content: '工作邮箱', parent: 'root' },
        { key: 'message', type: 'textarea', name: '需求输入框', frame: [610, 198, 330, 180], mobile: [32, 385, 326, 155], content: '简单描述你的需求', parent: 'root' },
        { key: 'submit', type: 'button', name: '提交按钮', frame: [610, 398, 330, 54], mobile: [32, 562, 326, 54], content: '发送需求', parent: 'root' }
      ];
    case 'newsletter':
      return [
        { key: 'root', type: 'section', name: '邮件订阅区', frame: [160, 0, 880, 300], mobile: [16, 0, 358, 470], style: { background: 'linear-gradient(145deg,#ECFCCB,#CCFBF1)', borderRadius: 32 } },
        { key: 'badge', type: 'badge', name: '订阅标签', frame: [210, 45, 120, 30], mobile: [32, 34, 120, 30], content: 'WEEKLY NOTE', style: { background: '#FFFFFF', color: '#047857', borderRadius: 999, fontSize: 11, fontWeight: 800, textAlign: 'center' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '订阅标题', frame: [210, 92, 500, 80], mobile: [32, 82, 326, 108], content: '每周一封，关于设计与产品', style: { color: '#064E3B', fontSize: 34, fontWeight: 900 }, mobileStyle: { fontSize: 29 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '订阅说明', frame: [210, 178, 460, 58], mobile: [32, 202, 326, 72], content: '不追热点，只分享可以直接用于真实工作的经验。', style: { color: '#047857', fontSize: 16, fontWeight: 500 }, parent: 'root' },
        { key: 'email', type: 'input', name: '订阅邮箱', frame: [690, 95, 280, 52], mobile: [32, 300, 326, 52], content: 'name@company.com', parent: 'root' },
        { key: 'submit', type: 'button', name: '订阅按钮', frame: [690, 165, 280, 52], mobile: [32, 370, 326, 52], content: '订阅更新 →', style: { background: '#065F46', color: '#FFFFFF', borderRadius: 12, fontSize: 15, fontWeight: 750 }, parent: 'root' }
      ];
    case 'cta':
      return [
        { key: 'root', type: 'section', name: '行动号召区', frame: [60, 0, 1080, 390], mobile: [16, 0, 358, 560], style: { background: 'radial-gradient(circle at 18% 20%,rgba(34,211,238,.32),transparent 34%),radial-gradient(circle at 84% 78%,rgba(168,85,247,.35),transparent 34%),#111827', borderRadius: 34 } },
        { key: 'heading', type: 'heading', name: '行动号召标题', frame: [220, 70, 760, 110], mobile: [32, 54, 326, 145], content: '准备好创造下一代产品体验了吗？', style: { color: '#FFFFFF', fontSize: 46, fontWeight: 900, textAlign: 'center' }, mobileStyle: { fontSize: 34 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '行动号召说明', frame: [330, 190, 540, 55], mobile: [44, 214, 302, 82], content: '从一个空白画布开始，或让 AI 帮你生成第一版完整设计。', style: { color: '#CBD5E1', fontSize: 17, fontWeight: 450, textAlign: 'center' }, parent: 'root' },
        { key: 'primary', type: 'button', name: '行动号召主按钮', frame: [390, 275, 200, 56], mobile: [42, 330, 146, 54], content: '免费开始', style: { background: '#FFFFFF', color: '#111827', borderRadius: 999, fontSize: 16, fontWeight: 800 }, parent: 'root' },
        { key: 'secondary', type: 'button', name: '行动号召次按钮', frame: [610, 275, 200, 56], mobile: [202, 330, 146, 54], content: '预约演示', style: { background: 'rgba(255,255,255,.08)', color: '#FFFFFF', borderColor: 'rgba(255,255,255,.25)', borderWidth: 1, borderRadius: 999, fontSize: 16, fontWeight: 750 }, parent: 'root' },
        { key: 'note', type: 'text', name: '行动号召补充', frame: [390, 346, 420, 30], mobile: [52, 416, 286, 60], content: '无需信用卡 · 随时可以导出设计', style: { color: '#94A3B8', fontSize: 13, fontWeight: 600, textAlign: 'center' }, parent: 'root' }
      ];
    case 'spotlight':
      return [
        { key: 'root', type: 'section', name: '渐变聚光舞台', frame: [60, 0, 1080, 600], mobile: [16, 0, 358, 820], style: { background: 'linear-gradient(rgba(255,255,255,.055) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,.055) 1px,transparent 1px),radial-gradient(circle at 50% 30%,#6D28D9 0%,#1E1B4B 42%,#09090B 78%)', borderRadius: 36 } },
        { key: 'orb', type: 'section', name: '聚光圆形', frame: [445, 85, 310, 310], mobile: [70, 74, 250, 250], style: { background: 'radial-gradient(circle at 36% 30%,#FFFFFF 0%,#A78BFA 18%,#7C3AED 43%,#312E81 72%,#111827 100%)', borderRadius: 999, shadow: '0 0 90px rgba(167,139,250,.48)' }, parent: 'root' },
        { key: 'heading', type: 'heading', name: '聚光标题', frame: [250, 360, 700, 92], mobile: [32, 346, 326, 120], content: '视觉不是装饰，而是产品的一部分', style: { color: '#FFFFFF', fontSize: 39, fontWeight: 900, textAlign: 'center' }, mobileStyle: { fontSize: 31 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '聚光说明', frame: [330, 460, 540, 58], mobile: [42, 486, 306, 85], content: '用层次、光影、留白和动效建立记忆点，同时保持内容清晰。', style: { color: '#C4B5FD', fontSize: 17, fontWeight: 500, textAlign: 'center' }, parent: 'root' },
        { key: 'left', type: 'card', name: '左侧浮卡', frame: [95, 175, 220, 120], mobile: [32, 620, 154, 140], content: 'TYPOGRAPHY\n清晰的层级', style: { background: 'rgba(255,255,255,.08)', color: '#FFFFFF', borderColor: 'rgba(255,255,255,.15)', borderWidth: 1, borderRadius: 20, fontSize: 15, fontWeight: 750 }, parent: 'root' },
        { key: 'right', type: 'card', name: '右侧浮卡', frame: [885, 175, 220, 120], mobile: [204, 620, 154, 140], content: 'MOTION\n有目的的动效', style: { background: 'rgba(255,255,255,.08)', color: '#FFFFFF', borderColor: 'rgba(255,255,255,.15)', borderWidth: 1, borderRadius: 20, fontSize: 15, fontWeight: 750 }, parent: 'root' }
      ];
    case 'footer':
      return [
        { key: 'root', type: 'section', name: '网站页脚', frame: [60, 0, 1080, 240], mobile: [16, 0, 358, 380], style: { background: '#1D1D1F', borderRadius: 24 } },
        { key: 'logo', type: 'logo', name: '页脚 Logo', frame: [105, 45, 180, 52], mobile: [32, 38, 160, 48], content: 'NOVA', style: { color: '#FFFFFF', fontSize: 26, fontWeight: 900 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '页脚说明', frame: [105, 105, 360, 65], mobile: [32, 98, 326, 72], content: '让每一个产品想法，都能更快成为现实。', style: { color: '#AEAEB2', fontSize: 15, fontWeight: 450 }, parent: 'root' },
        { key: 'links', type: 'list', name: '页脚链接', frame: [640, 42, 200, 130], mobile: [32, 190, 150, 130], content: '产品\n功能\n价格\n更新日志', style: { color: '#E5E5EA', fontSize: 15, fontWeight: 550 }, parent: 'root' },
        { key: 'legal', type: 'list', name: '法律链接', frame: [880, 42, 180, 130], mobile: [205, 190, 150, 130], content: '关于我们\n隐私政策\n服务条款', style: { color: '#E5E5EA', fontSize: 15, fontWeight: 550 }, parent: 'root' },
        { key: 'copyright', type: 'text', name: '版权信息', frame: [105, 188, 500, 32], mobile: [32, 330, 326, 32], content: '© 2026 NOVA. All rights reserved.', style: { color: '#8E8E93', fontSize: 13, fontWeight: 450 }, parent: 'root' }
      ];
    case 'footer-minimal':
      return [
        { key: 'root', type: 'section', name: '极简页脚', frame: [60, 0, 1080, 150], mobile: [16, 0, 358, 260], style: { background: '#FAFAF9', borderColor: '#E7E5E4', borderWidth: 1, borderRadius: 24 } },
        { key: 'logo', type: 'logo', name: '页脚品牌', frame: [90, 35, 220, 46], mobile: [32, 28, 220, 46], content: 'STUDIO / 26', style: { color: '#1C1917', fontSize: 18, fontWeight: 900 }, parent: 'root' },
        { key: 'links', type: 'text', name: '社交链接', frame: [650, 35, 360, 46], mobile: [32, 98, 326, 52], content: 'Instagram   Behance   LinkedIn   ↗', style: { color: '#57534E', fontSize: 14, fontWeight: 650, textAlign: 'right' }, mobileStyle: { textAlign: 'left' }, parent: 'root' },
        { key: 'copyright', type: 'text', name: '极简版权信息', frame: [90, 96, 920, 30], mobile: [32, 178, 326, 48], content: '© 2026 Studio. Crafted with care in Shanghai.', style: { color: '#A8A29E', fontSize: 12, fontWeight: 500 }, parent: 'root' }
      ];
  }
}

function insertionOrigin(document: WebDesignDocument, pageId: string, device: WebDesignDevice): number {
  const frames = componentsForPage(document, pageId).map((component) => resolveComponent(component, device)).filter((frame) => !frame.hidden);
  return frames.length ? Math.max(...frames.map((frame) => frame.y + frame.height)) + 40 : 40;
}

export function createBlockPreset(document: WebDesignDocument, pageId: string, presetId: WebDesignBlockPresetId): { components: WebDesignComponent[]; rootIds: string[] } {
  const parts = presetParts(presetId);
  const idMap = new Map(parts.map((part) => [part.key, `${presetId}-${part.key}-${globalThis.crypto.randomUUID().slice(0, 8)}`]));
  const desktopOrigin = insertionOrigin(document, pageId, 'desktop');
  const tabletOrigin = insertionOrigin(document, pageId, 'tablet');
  const mobileOrigin = insertionOrigin(document, pageId, 'mobile');
  const desktopWidth = breakpointFor(document, 'desktop').width;
  const tabletWidth = breakpointFor(document, 'tablet').width;
  const desktopScale = desktopWidth / 1200;
  const tabletScale = tabletWidth / 1200;
  const maxZ = Math.max(0, ...componentsForPage(document, pageId).map((component) => component.zIndex));
  const components = parts.map((part, index) => {
    const component = componentDefaults(part.type, part.frame[0] * desktopScale, desktopOrigin + part.frame[1]);
    component.id = idMap.get(part.key)!;
    component.name = part.name;
    component.pageId = pageId;
    component.parentId = part.parent ? idMap.get(part.parent) : undefined;
    component.x = Math.round(part.frame[0] * desktopScale);
    component.y = Math.round(desktopOrigin + part.frame[1]);
    component.width = Math.round(part.frame[2] * desktopScale);
    component.height = part.frame[3];
    component.zIndex = maxZ + index + 1;
    if (part.content !== undefined) component.content = part.content;
    if (part.style) component.style = { ...component.style, ...part.style };
    component.library = antDesignBinding(part);
    component.responsive = {
      tablet: {
        x: Math.round(part.frame[0] * tabletScale),
        y: Math.round(tabletOrigin + part.frame[1] * .82),
        width: Math.max(16, Math.round(part.frame[2] * tabletScale)),
        height: part.frame[3],
        style: part.mobileStyle
      },
      mobile: {
        x: part.mobile[0], y: mobileOrigin + part.mobile[1], width: Math.max(16, part.mobile[2]), height: Math.max(16, part.mobile[3]),
        hidden: part.mobile[4] === true, style: part.mobileStyle
      }
    };
    return component;
  });
  return { components, rootIds: parts.filter((part) => !part.parent).map((part) => idMap.get(part.key)!) };
}

export function createPageTemplate(document: WebDesignDocument, pageId: string, templateId: WebDesignPageTemplateId): { components: WebDesignComponent[]; rootIds: string[] } {
  const template = WEB_DESIGN_PAGE_TEMPLATES.find((candidate) => candidate.id === templateId);
  if (!template) throw new Error(`Page template not found: ${templateId}`);
  let working = { ...document, components: document.components.filter((component) => pageIdForComponent(document, component) !== pageId) };
  const components: WebDesignComponent[] = [];
  const rootIds: string[] = [];
  for (const blockId of template.blocks) {
    const block = createBlockPreset(working, pageId, blockId);
    components.push(...block.components);
    rootIds.push(...block.rootIds);
    working = { ...working, components: [...working.components, ...block.components] };
  }
  return { components, rootIds };
}
