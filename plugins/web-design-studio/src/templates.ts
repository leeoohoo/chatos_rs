import { DEFAULT_WEB_DESIGN_BREAKPOINTS, DEFAULT_WEB_DESIGN_TOKENS, type WebComponentType, type WebDesignComponent, type WebDesignDocument } from './schema.js';
import { scaleFrameForBreakpoint } from './editor-model.js';

const antdPlaceholderImage = 'https://images.unsplash.com/photo-1550745165-9bc0b252726f?auto=format&fit=crop&w=900&q=80';

function randomId(): string {
  return globalThis.crypto.randomUUID().slice(0, 8);
}

export function componentDefaults(type: WebComponentType, x: number, y: number): WebDesignComponent {
  const id = `${type}-${randomId()}`;
  const base = { id, type, x, y, zIndex: 1, annotations: [] };
  switch (type) {
    case 'section':
      return { ...base, name: '区块', width: 680, height: 220, content: '', style: { background: '#F5F5F7', borderRadius: 24, borderColor: '#E5E5EA', borderWidth: 1 } };
    case 'text':
      return { ...base, name: '文本', width: 360, height: 72, content: '一段新的文本', style: { color: '#1F2937', fontSize: 28, fontWeight: 700, textAlign: 'left' } };
    case 'heading':
      return { ...base, name: '标题', width: 480, height: 88, content: '清晰有力的页面标题', style: { color: '#1D1D1F', fontSize: 42, fontWeight: 800, textAlign: 'left' } };
    case 'button':
      return { ...base, name: '按钮', width: 150, height: 48, content: '立即开始', style: { background: '#007AFF', color: '#FFFFFF', borderRadius: 12, fontSize: 16, fontWeight: 650, textAlign: 'center' } };
    case 'link':
      return { ...base, name: '链接', width: 150, height: 40, content: '了解更多 →', style: { color: '#007AFF', fontSize: 16, fontWeight: 650, textAlign: 'left' } };
    case 'image':
      return { ...base, name: '图片', width: 300, height: 200, content: '', style: { background: 'linear-gradient(135deg, #5AC8FA 0%, #64D2FF 50%, #007AFF 100%)', borderRadius: 18 } };
    case 'icon':
      return { ...base, name: '图标', width: 48, height: 48, content: '✦', style: { background: '#EAF3FF', color: '#007AFF', borderRadius: 12, fontSize: 24, fontWeight: 700, textAlign: 'center' } };
    case 'logo':
      return { ...base, name: 'Logo', width: 150, height: 48, content: 'NOVA', style: { color: '#1D1D1F', fontSize: 24, fontWeight: 800, textAlign: 'left' } };
    case 'card':
      return { ...base, name: '卡片', width: 260, height: 180, content: '功能卡片\n在这里说明产品价值。', style: { background: '#FFFFFF', color: '#111827', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 18, fontSize: 18, fontWeight: 650, shadow: '0 14px 35px rgba(31, 41, 55, .10)' } };
    case 'input':
      return { ...base, name: '输入框', width: 300, height: 48, content: '请输入内容', style: { background: '#FFFFFF', color: '#6B7280', borderColor: '#D1D5DB', borderWidth: 1, borderRadius: 12, fontSize: 15 } };
    case 'textarea':
      return { ...base, name: '多行输入', width: 320, height: 120, content: '请输入详细内容', style: { background: '#FFFFFF', color: '#6B7280', borderColor: '#D1D5DB', borderWidth: 1, borderRadius: 12, fontSize: 15 } };
    case 'select':
      return { ...base, name: '下拉选择', width: 260, height: 48, content: '请选择\n选项一\n选项二', style: { background: '#FFFFFF', color: '#374151', borderColor: '#D1D5DB', borderWidth: 1, borderRadius: 12, fontSize: 15 } };
    case 'checkbox':
      return { ...base, name: '复选框', width: 220, height: 40, content: '我同意服务条款', style: { color: '#374151', fontSize: 15, fontWeight: 500 } };
    case 'switch':
      return { ...base, name: '开关', width: 180, height: 40, content: '启用通知', style: { color: '#374151', fontSize: 15, fontWeight: 500 } };
    case 'divider':
      return { ...base, name: '分隔线', width: 420, height: 16, content: '', style: { borderColor: '#E5E7EB', borderWidth: 1 } };
    case 'badge':
      return { ...base, name: '徽章', width: 88, height: 30, content: 'NEW', style: { background: '#EAF3FF', color: '#007AFF', borderRadius: 999, fontSize: 12, fontWeight: 750, textAlign: 'center' } };
    case 'avatar':
      return { ...base, name: '头像', width: 64, height: 64, content: 'AI', style: { background: 'linear-gradient(135deg,#5AC8FA,#007AFF)', color: '#FFFFFF', borderRadius: 999, fontSize: 20, fontWeight: 750, textAlign: 'center' } };
    case 'list':
      return { ...base, name: '列表', width: 320, height: 150, content: '快速搭建\n响应式设计\nAI 协作修改', style: { background: '#FFFFFF', color: '#374151', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 14, fontSize: 16 } };
    case 'table':
      return { ...base, name: '表格', width: 520, height: 210, content: '套餐|价格|状态\n基础版|¥99|可用\n专业版|¥299|推荐', style: { background: '#FFFFFF', color: '#1F2937', borderColor: '#E5E7EB', borderWidth: 1, borderRadius: 12, fontSize: 14 } };
    case 'video':
      return { ...base, name: '视频', width: 420, height: 236, content: '', style: { background: '#1D1D1F', color: '#FFFFFF', borderRadius: 18, fontSize: 18, fontWeight: 700, textAlign: 'center' } };
  }
}

export function createLandingPage(title = 'AI 产品落地页'): WebDesignDocument {
  const now = new Date().toISOString();
  const hero = componentDefaults('section', 60, 60);
  hero.id = 'hero-section';
  hero.name = 'Hero 区域';
  hero.width = 1080;
  hero.height = 500;
  hero.style = { background: 'linear-gradient(145deg, #F5F5F7 0%, #EAF3FF 100%)', borderRadius: 32 };

  const heading = componentDefaults('text', 120, 125);
  heading.id = 'hero-heading';
  heading.name = '主标题';
  heading.width = 560;
  heading.height = 150;
  heading.zIndex = 3;
  heading.content = '把想法变成\n真正可编辑的网站';
  heading.style = { color: '#1D1D1F', fontSize: 52, fontWeight: 800, textAlign: 'left' };

  const copy = componentDefaults('text', 120, 295);
  copy.id = 'hero-copy';
  copy.name = '副标题';
  copy.width = 500;
  copy.height = 80;
  copy.zIndex = 3;
  copy.content = '让 AI 生成页面，再像设计工具一样拖动、缩放、批注并继续迭代。';
  copy.style = { color: '#6E6E73', fontSize: 19, fontWeight: 450, textAlign: 'left' };

  const button = componentDefaults('button', 120, 405);
  button.id = 'hero-primary-action';
  button.name = '主按钮';
  button.width = 170;
  button.zIndex = 4;

  const image = componentDefaults('image', 760, 115);
  image.id = 'hero-image';
  image.name = 'Hero 图片';
  image.width = 330;
  image.height = 360;
  image.zIndex = 2;

  hero.layout = { mode: 'free', gap: 16, padding: 20, align: 'start' };
  heading.library = { name: 'antd', version: '6.6.2', component: 'Typography', variant: 'title', props: { level: 1 } };
  copy.library = { name: 'antd', version: '6.6.2', component: 'Typography', variant: 'paragraph', props: {} };
  button.library = { name: 'antd', version: '6.6.2', component: 'Button', variant: 'primary', props: { color: 'primary', variant: 'solid', size: 'large', danger: false } };
  image.library = { name: 'antd', version: '6.6.2', component: 'Image', variant: 'basic', props: { src: antdPlaceholderImage, preview: false } };
  heading.parentId = hero.id;
  copy.parentId = hero.id;
  button.parentId = hero.id;
  image.parentId = hero.id;

  const cardA = componentDefaults('card', 100, 630);
  cardA.id = 'feature-ai';
  cardA.content = 'AI 共同设计\n对选中组件提出修改要求。';
  const cardB = componentDefaults('card', 390, 630);
  cardB.id = 'feature-visual';
  cardB.content = '可视化调整\n直接拖动并改变组件大小。';
  const cardC = componentDefaults('card', 680, 630);
  cardC.id = 'feature-code';
  cardC.content = '结构化交付\n后续可导出网站实现代码。';
  for (const [card, title] of [[cardA, 'AI 共同设计'], [cardB, '可视化调整'], [cardC, '结构化交付']] as const) {
    card.library = { name: 'antd', version: '6.6.2', component: 'Card', variant: 'default', props: { title, bordered: true } };
  }

  const components = [hero, heading, copy, button, image, cardA, cardB, cardC];
  for (const component of components) {
    component.pageId = 'home';
    component.responsive = {
      tablet: scaleFrameForBreakpoint(component, DEFAULT_WEB_DESIGN_BREAKPOINTS.desktop, DEFAULT_WEB_DESIGN_BREAKPOINTS.tablet),
      mobile: scaleFrameForBreakpoint(component, DEFAULT_WEB_DESIGN_BREAKPOINTS.desktop, DEFAULT_WEB_DESIGN_BREAKPOINTS.mobile)
    };
  }
  hero.responsive!.mobile = { x: 16, y: 16, width: 358, height: 720 };
  heading.responsive!.mobile = { x: 32, y: 58, width: 326, height: 180, style: { fontSize: 39 } };
  copy.responsive!.mobile = { x: 32, y: 255, width: 326, height: 110, style: { fontSize: 17 } };
  button.responsive!.mobile = { x: 32, y: 390, width: 180, height: 50 };
  image.responsive!.mobile = { x: 32, y: 475, width: 326, height: 220 };
  cardA.responsive!.mobile = { x: 32, y: 770, width: 326, height: 170 };
  cardB.responsive!.mobile = { x: 32, y: 965, width: 326, height: 170 };
  cardC.responsive!.mobile = { x: 32, y: 1160, width: 326, height: 170 };

  return {
    schemaVersion: 1,
    documentId: `website-${randomId()}`,
    revision: 0,
    title,
    description: 'Web Design Studio 默认落地页模板。',
    createdAt: now,
    updatedAt: now,
    viewport: { width: 1200, height: 940, background: '#F8FAFC' },
    breakpoints: {
      desktop: { ...DEFAULT_WEB_DESIGN_BREAKPOINTS.desktop },
      tablet: { ...DEFAULT_WEB_DESIGN_BREAKPOINTS.tablet },
      mobile: { width: 390, height: 1380 }
    },
    pages: [{ id: 'home', name: '首页', slug: '/' }],
    assets: [],
    tokens: structuredClone(DEFAULT_WEB_DESIGN_TOKENS),
    symbols: [],
    components,
    requests: []
  };
}

export function createBlankWebsite(title = '未命名网站'): WebDesignDocument {
  const now = new Date().toISOString();
  return {
    schemaVersion: 1,
    documentId: `website-${randomId()}`,
    revision: 0,
    title,
    description: '空白网站设计。',
    createdAt: now,
    updatedAt: now,
    viewport: { width: 1200, height: 940, background: '#FFFFFF' },
    breakpoints: structuredClone(DEFAULT_WEB_DESIGN_BREAKPOINTS),
    pages: [{ id: 'home', name: '首页', slug: '/' }],
    assets: [],
    tokens: structuredClone(DEFAULT_WEB_DESIGN_TOKENS),
    symbols: [],
    components: [],
    requests: []
  };
}
