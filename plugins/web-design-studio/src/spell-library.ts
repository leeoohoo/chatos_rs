import { assertCreativeCatalog, createCreativeDefinitions, createCreativeVariants, type CreativeComponentDescriptor } from './creative-library.js';
import { applyUiComponentVariant, createUiLibraryComponent, variantsForUiComponent, type UiLibraryCatalog } from './ui-library.js';

export const SPELL_VERSION = 'registry-2026.09';
export const SPELL_LICENSE = 'MIT';
export const SPELL_CATEGORIES = ['文字与排版', '按钮与表单', '数据与反馈', '卡片与媒体', '视觉特效'] as const;
export type SpellCategory = (typeof SPELL_CATEGORIES)[number];

const SPELL_DESCRIPTORS = [
  { slug: 'randomized-text', label: '随机字符文字', category: '文字与排版', family: 'text', icon: 'R', content: 'Randomized creative type' },
  { slug: 'animated-gradient', label: '动态渐变文字', category: '文字与排版', family: 'text', icon: 'A', content: 'Design in every direction' },
  { slug: 'special-text', label: '特效标题', category: '文字与排版', family: 'text', icon: 'S', content: 'A special way to create' },
  { slug: 'blur-reveal', label: '模糊揭示文字', category: '文字与排版', family: 'text', icon: '◒', content: '从模糊中逐字清晰出现' },
  { slug: 'text-marquee', label: '文字跑马灯', category: '文字与排版', family: 'marquee', icon: '↔', content: 'DESIGN · BUILD · SHIP ·' },
  { slug: 'highlighted-text', label: '强调高亮文字', category: '文字与排版', family: 'text', icon: 'H', content: '让最重要的信息被看见' },
  { slug: 'words-stagger', label: '词语错落动画', category: '文字与排版', family: 'text', icon: 'W', content: 'Every word moves with purpose' },
  { slug: 'slide-up-text', label: '上滑文字', category: '文字与排版', family: 'text', icon: '↑', content: 'Ideas rise into products' },
  { slug: 'gradient-wave-text', label: '渐变波浪文字', category: '文字与排版', family: 'text', icon: '≈', content: 'Creative energy in motion' },
  { slug: 'shimmer-text', label: '微光文字', category: '文字与排版', family: 'text', icon: '✦', content: 'AI is composing your website' },
  { slug: 'signature', label: '签名线条', category: '文字与排版', family: 'text', icon: '〜', content: 'Designed by human + AI' },

  { slug: 'flow-button', label: '流动按钮', category: '按钮与表单', family: 'button', icon: '→', content: '继续设计' },
  { slug: 'copy-button', label: '复制按钮', category: '按钮与表单', family: 'copy', icon: '⎘', content: 'npx shadcn@latest add @spell/chart' },
  { slug: 'exploding-input', label: '爆裂特效输入框', category: '按钮与表单', family: 'input', icon: '✦', content: '输入一个大胆的想法…' },
  { slug: 'label-input', label: '标签输入框', category: '按钮与表单', family: 'input', icon: 'I', content: 'designer@example.com' },
  { slug: 'animated-checkbox', label: '动画复选框', category: '按钮与表单', family: 'checkbox', icon: '☑', content: '启用 AI 设计建议' },
  { slug: 'rich-button', label: '富内容按钮', category: '按钮与表单', family: 'button', icon: '✦', content: '用 AI 生成页面' },
  { slug: 'pop-button', label: '弹跳按钮', category: '按钮与表单', family: 'button', icon: '↗', content: '打开在线预览' },
  { slug: 'color-selector', label: '颜色选择器', category: '按钮与表单', family: 'color', icon: '◐', content: '品牌主色' },
  { slug: 'kbd', label: '键盘快捷键', category: '按钮与表单', family: 'kbd', icon: '⌘', content: '打开命令面板', props: { keys: ['⌘', 'K'] } },

  { slug: 'chart', label: '交互式图表', category: '数据与反馈', family: 'chart', icon: '∿', content: '产品增长趋势', props: { values: [28, 45, 37, 72, 61, 88] }, width: 560, height: 320 },
  { slug: 'badge', label: '状态徽章', category: '数据与反馈', family: 'badge', icon: '◉', content: '正在协作' },
  { slug: 'bars-spinner', label: '柱状加载器', category: '数据与反馈', family: 'spinner', icon: '≡', content: '正在构建组件…' },
  { slug: 'spinner', label: '旋转加载器', category: '数据与反馈', family: 'spinner', icon: '◌', content: '正在同步设计…' },
  { slug: 'qr-code', label: '二维码', category: '数据与反馈', family: 'qr', icon: '▦', content: '移动端预览' },

  { slug: 'tweet', label: '社交内容卡', category: '卡片与媒体', family: 'social', icon: '𝕏', content: '今天用 AI 完成了一套真正可编辑的网站设计。' },
  { slug: 'fallback-avatar', label: '备用头像', category: '卡片与媒体', family: 'avatars', icon: '●', content: '团队成员头像' },
  { slug: 'perspective-book', label: '透视书本', category: '卡片与媒体', family: 'book', icon: '◲', content: 'The interface design handbook' },
  { slug: 'spotify-card', label: '音乐媒体卡', category: '卡片与媒体', family: 'social', icon: '♫', content: 'Focus Flow — Design Sessions' },
  { slug: 'tilt-card', label: '3D 倾斜卡片', category: '卡片与媒体', family: 'card', icon: '◇', content: '指针移动时产生立体倾斜' },

  { slug: 'light-rays', label: '空间光线', category: '视觉特效', family: 'beam', icon: '╱', content: '穿过画布的体积光束' },
  { slug: 'logos-carousel', label: '品牌轮播', category: '视觉特效', family: 'marquee', icon: '◈', content: '合作伙伴品牌轮播' },
  { slug: 'marquee', label: '内容跑马灯', category: '视觉特效', family: 'marquee', icon: '↔', content: '项目、用户与产品亮点循环展示' }
] as const satisfies readonly CreativeComponentDescriptor<SpellCategory>[];

assertCreativeCatalog('spell', SPELL_DESCRIPTORS);
export const SPELL_COMPONENTS = createCreativeDefinitions(SPELL_DESCRIPTORS, 'https://spell.sh/docs/');
export const SPELL_COMPONENT_VARIANTS = createCreativeVariants(SPELL_DESCRIPTORS);

export const SPELL_LIBRARY: UiLibraryCatalog<SpellCategory> = {
  id: 'spell', displayName: 'Spell UI', shortName: 'Spell', version: SPELL_VERSION, brandMark: 'S',
  categories: SPELL_CATEGORIES, components: SPELL_COMPONENTS, variants: SPELL_COMPONENT_VARIANTS,
  license: SPELL_LICENSE, sourceUrl: 'https://github.com/xxtomm/spell-ui', licenseUrl: 'https://github.com/xxtomm/spell-ui/blob/main/LICENSE'
};

export function createSpellComponent(definitionId: string, x: number, y: number) { return createUiLibraryComponent(SPELL_LIBRARY, definitionId, x, y); }
export function variantsForSpellComponent(definitionId: string) { return variantsForUiComponent(SPELL_LIBRARY, definitionId); }
export function applySpellComponentVariant(component: Parameters<typeof applyUiComponentVariant>[1], variantId: string) { return applyUiComponentVariant(SPELL_LIBRARY, component, variantId); }
