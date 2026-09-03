import { breakpointFor, componentsForPage, resolveComponent } from './editor-model.js';
import { pageIdForComponent, type WebComponentStyle, type WebComponentType, type WebDesignComponent, type WebDesignDevice, type WebDesignDocument, type WebDesignLibraryBinding } from './schema.js';
import { componentDefaults } from './templates.js';

export type WebDesignBlockPresetId = 'navbar' | 'hero' | 'features' | 'pricing' | 'faq' | 'contact' | 'footer';
export type WebDesignPageTemplateId = 'saas' | 'launch' | 'business';

export interface WebDesignBlockPreset {
  id: WebDesignBlockPresetId;
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
  blocks: WebDesignBlockPresetId[];
}

export const WEB_DESIGN_BLOCK_PRESETS: WebDesignBlockPreset[] = [
  { id: 'navbar', name: '导航栏', description: 'Logo、导航链接和行动按钮', icon: '☰', keywords: ['导航', 'navbar', 'header', '菜单'] },
  { id: 'hero', name: 'Hero 首屏', description: '标题、介绍、双按钮和主视觉', icon: '✦', keywords: ['hero', '首屏', '横幅', '营销'] },
  { id: 'features', name: '功能介绍', description: '标题与三列功能卡片', icon: '▦', keywords: ['功能', 'features', '卡片'] },
  { id: 'pricing', name: '价格方案', description: '三档产品价格卡片', icon: '¥', keywords: ['价格', 'pricing', '套餐'] },
  { id: 'faq', name: '常见问题', description: '可编辑的 FAQ 问答列表', icon: '?', keywords: ['faq', '问题', '帮助'] },
  { id: 'contact', name: '联系表单', description: '姓名、邮箱、留言和提交按钮', icon: '✉', keywords: ['联系', 'contact', '表单'] },
  { id: 'footer', name: '网站页脚', description: '品牌、说明和常用链接', icon: '▔', keywords: ['页脚', 'footer', '版权'] }
];

export const WEB_DESIGN_PAGE_TEMPLATES: WebDesignPageTemplate[] = [
  { id: 'saas', name: 'SaaS 产品官网', description: '完整导航、首屏、功能、定价、FAQ、联系和页脚', icon: 'S', blocks: ['navbar', 'hero', 'features', 'pricing', 'faq', 'contact', 'footer'] },
  { id: 'launch', name: '产品发布页', description: '适合新产品发布和活动转化的精简长页', icon: 'L', blocks: ['navbar', 'hero', 'features', 'contact', 'footer'] },
  { id: 'business', name: '企业服务官网', description: '品牌介绍、服务能力、FAQ 和销售线索表单', icon: 'B', blocks: ['navbar', 'hero', 'features', 'faq', 'contact', 'footer'] }
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
    case 'heading': return { name: 'antd', version: '6.2.2', component: 'Typography', variant: 'title', props: { level: 2 } };
    case 'text': return { name: 'antd', version: '6.2.2', component: 'Typography', variant: 'paragraph', props: {} };
    case 'logo': return { name: 'antd', version: '6.2.2', component: 'Typography', variant: 'title', props: { level: 3 } };
    case 'button': return { name: 'antd', version: '6.2.2', component: 'Button', variant: part.style?.background === '#FFFFFF' ? 'default' : 'primary', props: { type: part.style?.background === '#FFFFFF' ? 'default' : 'primary', size: 'large', danger: false } };
    case 'link': return { name: 'antd', version: '6.2.2', component: 'Button', variant: 'link', props: { type: 'link' } };
    case 'card': return { name: 'antd', version: '6.2.2', component: 'Card', variant: 'default', props: { bordered: true } };
    case 'input': return { name: 'antd', version: '6.2.2', component: 'Input', variant: 'outlined', props: { variant: 'outlined', allowClear: true } };
    case 'textarea': return { name: 'antd', version: '6.2.2', component: 'Input', variant: 'textarea', props: { variant: 'outlined', autoSize: { minRows: 4, maxRows: 8 } } };
    case 'select': return { name: 'antd', version: '6.2.2', component: 'Select', variant: 'outlined', props: { variant: 'outlined', options: propsForContentList().map((label) => ({ label, value: label })) } };
    case 'badge': return { name: 'antd', version: '6.2.2', component: 'Tag', variant: 'blue', props: { color: 'blue' } };
    case 'avatar': return { name: 'antd', version: '6.2.2', component: 'Avatar', variant: 'circle', props: { shape: 'circle', size: 48 } };
    case 'list': return { name: 'antd', version: '6.2.2', component: 'List', variant: 'default', props: { bordered: false, dataSource: propsForContentList() } };
    case 'image': return { name: 'antd', version: '6.2.2', component: 'Image', variant: 'default', props: { src: antdPlaceholderImage, preview: false } };
    default: return undefined;
  }
}

function presetParts(id: WebDesignBlockPresetId): PresetPart[] {
  switch (id) {
    case 'navbar':
      return [
        { key: 'root', type: 'section', name: '网站导航栏', frame: [60, 0, 1080, 76], mobile: [16, 0, 358, 68], style: { ...whiteSection, borderRadius: 18 } },
        { key: 'logo', type: 'logo', name: '品牌 Logo', frame: [88, 14, 150, 48], mobile: [32, 10, 130, 48], content: 'NOVA', parent: 'root' },
        { key: 'home', type: 'link', name: '首页链接', frame: [520, 18, 80, 40], mobile: [180, 14, 1, 1, true], content: '首页', style: { color: '#4B465D', fontSize: 15, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'features', type: 'link', name: '功能链接', frame: [610, 18, 80, 40], mobile: [181, 14, 1, 1, true], content: '功能', style: { color: '#4B465D', fontSize: 15, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'pricing', type: 'link', name: '价格链接', frame: [700, 18, 80, 40], mobile: [182, 14, 1, 1, true], content: '价格', style: { color: '#4B465D', fontSize: 15, fontWeight: 600, textAlign: 'center' }, parent: 'root' },
        { key: 'action', type: 'button', name: '导航行动按钮', frame: [930, 14, 180, 48], mobile: [232, 10, 126, 48], content: '免费开始', parent: 'root' }
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
    case 'features':
      return [
        { key: 'root', type: 'section', name: '功能介绍区', frame: [60, 0, 1080, 430], mobile: [16, 0, 358, 760], style: { background: '#F8FAFC', borderRadius: 28 } },
        { key: 'heading', type: 'heading', name: '功能区标题', frame: [100, 46, 620, 72], mobile: [32, 36, 326, 88], content: '为真实工作流而设计', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'one', type: 'card', name: '功能卡片一', frame: [100, 145, 300, 220], mobile: [32, 142, 326, 170], content: '✦  AI 共同设计\n从一句需求开始，持续修改具体组件。', parent: 'root' },
        { key: 'two', type: 'card', name: '功能卡片二', frame: [450, 145, 300, 220], mobile: [32, 330, 326, 170], content: '▣  可视化编辑\n拖动、缩放、布局和响应式调整都可直接完成。', parent: 'root' },
        { key: 'three', type: 'card', name: '功能卡片三', frame: [800, 145, 300, 220], mobile: [32, 518, 326, 170], content: '⌘  代码交付\n导出 HTML、React 或 Vue，继续进入开发流程。', parent: 'root' }
      ];
    case 'pricing':
      return [
        { key: 'root', type: 'section', name: '价格方案区', frame: [60, 0, 1080, 500], mobile: [16, 0, 358, 980], style: { background: '#FFFFFF', borderRadius: 28 } },
        { key: 'heading', type: 'heading', name: '价格区标题', frame: [100, 42, 620, 72], mobile: [32, 34, 326, 88], content: '选择适合你的方案', style: darkHeading, mobileStyle: { fontSize: 32 }, parent: 'root' },
        { key: 'starter', type: 'card', name: '基础版价格卡', frame: [100, 140, 300, 290], mobile: [32, 140, 326, 240], content: '基础版\n¥99 / 月\n\n适合个人项目\n基础组件与导出', parent: 'root' },
        { key: 'pro', type: 'card', name: '专业版价格卡', frame: [450, 125, 300, 320], mobile: [32, 398, 326, 250], content: '专业版 · 推荐\n¥299 / 月\n\n完整组件与 AI 协作\n团队项目和版本管理', style: { background: '#007AFF', color: '#FFFFFF', borderRadius: 20, fontSize: 19, fontWeight: 700, shadow: '0 18px 40px rgba(0,122,255,.24)' }, parent: 'root' },
        { key: 'team', type: 'card', name: '团队版价格卡', frame: [800, 140, 300, 290], mobile: [32, 666, 326, 240], content: '团队版\n联系销售\n\n权限与审计\n定制组件和集成', parent: 'root' }
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
    case 'footer':
      return [
        { key: 'root', type: 'section', name: '网站页脚', frame: [60, 0, 1080, 240], mobile: [16, 0, 358, 380], style: { background: '#1D1D1F', borderRadius: 24 } },
        { key: 'logo', type: 'logo', name: '页脚 Logo', frame: [105, 45, 180, 52], mobile: [32, 38, 160, 48], content: 'NOVA', style: { color: '#FFFFFF', fontSize: 26, fontWeight: 900 }, parent: 'root' },
        { key: 'copy', type: 'text', name: '页脚说明', frame: [105, 105, 360, 65], mobile: [32, 98, 326, 72], content: '让每一个产品想法，都能更快成为现实。', style: { color: '#AEAEB2', fontSize: 15, fontWeight: 450 }, parent: 'root' },
        { key: 'links', type: 'list', name: '页脚链接', frame: [640, 42, 200, 130], mobile: [32, 190, 150, 130], content: '产品\n功能\n价格\n更新日志', style: { color: '#E5E5EA', fontSize: 15, fontWeight: 550 }, parent: 'root' },
        { key: 'legal', type: 'list', name: '法律链接', frame: [880, 42, 180, 130], mobile: [205, 190, 150, 130], content: '关于我们\n隐私政策\n服务条款', style: { color: '#E5E5EA', fontSize: 15, fontWeight: 550 }, parent: 'root' },
        { key: 'copyright', type: 'text', name: '版权信息', frame: [105, 188, 500, 32], mobile: [32, 330, 326, 32], content: '© 2026 NOVA. All rights reserved.', style: { color: '#8E8E93', fontSize: 13, fontWeight: 450 }, parent: 'root' }
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
