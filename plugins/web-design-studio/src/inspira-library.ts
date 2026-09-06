import { assertCreativeCatalog, createCreativeDefinitions, type CreativeComponentDescriptor, type CreativeFamily } from './creative-library.js';
import { applyUiComponentVariant, createUiLibraryComponent, variantsForUiComponent, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';
import { INSPIRA_OFFICIAL_COMPONENT_VARIANTS } from './inspira-registry.generated.js';
import type { WebDesignJsonValue } from './schema.js';

export const INSPIRA_VERSION = 'registry-2026.09-official';
export const INSPIRA_LICENSE = 'MIT';
export const INSPIRA_CATEGORIES = ['背景', '按钮', '卡片', '光标', '设备模型', 'HTML 画布', '输入与表单', '通用组件', '特效', '客户证言', '文字动画', '可视化'] as const;
export type InspiraCategory = (typeof INSPIRA_CATEGORIES)[number];

const INSPIRA_SOURCE: ReadonlyArray<{ category: InspiraCategory; section: string; slugs: readonly string[] }> = [
  { category: '背景', section: 'backgrounds', slugs: ['aurora-background','black-hole-background','bubbles-bg','cosmic-portal','falling-stars','flickering-grid','interactive-grid-pattern','lamp-effect','liquid-background','neural-background','particle-whirlpool-bg','particles-bg','pattern-background','ribbon-background','ripple','silk-background','singularity-background','snowfall-bg','sparkles','stars-background','stractium-background','tetris','thunderstorm-background','video-text','vortex','warp-background','wavy-background'] },
  { category: '按钮', section: 'buttons', slugs: ['gradient-button','interactive-hover-button','rainbow-button','ripple-button','shimmer-button'] },
  { category: '卡片', section: 'cards', slugs: ['3d-card','apple-card-carousel','card-spotlight','card-stack','cube-carousel','direction-aware-hover','fey-cards','flip-card','floating-card','glare-card'] },
  { category: '光标', section: 'cursors', slugs: ['fluid-cursor','image-trail-cursor','sleek-line-cursor','smooth-cursor','tailed-cursor'] },
  { category: '设备模型', section: 'device-mocks', slugs: ['iphone-mockup','safari-mockup'] },
  { category: 'HTML 画布', section: 'html-in-canvas', slugs: ['html-ascii','html-blaze','html-chromatic','html-cloth','html-drag','html-in-canvas','html-liquid'] },
  { category: '输入与表单', section: 'input-and-forms', slugs: ['balance-slider','color-picker','file-upload','halo-search','input','placeholders-and-vanish-input'] },
  { category: '通用组件', section: 'miscellaneous', slugs: ['animate-grid','animated-circular-progressbar','animated-list','animated-modal','animated-tabs','animated-tooltip','bento-grid','book','circular-gallery','compare','container-scroll','dock','expandable-gallery','float','images-slider','lens','link-preview','marquee','media-text','morphing-tabs','multi-step-loader','parallax-float','path-marquee','photo-gallery','scroll-island','shader-toy','svg-mask','timeline','tracing-beam'] },
  { category: '特效', section: 'special-effects', slugs: ['animated-beam','border-beam','confetti','dither-shader','glow-border','glowing-effect','images-badge','meteors','neon-border','particle-image','progressive-blur','scales','scratch-to-reveal','spring-calendar'] },
  { category: '客户证言', section: 'testimonials', slugs: ['animated-testimonials','design-testimonials','testimonial-slider'] },
  { category: '文字动画', section: 'text-animations', slugs: ['3d-text','blur-reveal','box-reveal','breathing-text','colorful-text','container-text-flip','encrypted-text','flip-words','focus','highlight-text','hyper-text','letter-pullup','letter-swap','line-shadow-text','morphing-text','number-ticker','radiant-text','screw-text','scroll-swap-text','sparkles-text','spinning-text','text-generate-effect','text-glitch','text-highlight','text-hover-effect','text-reveal','text-reveal-card','text-scroll-reveal','typewriter-text','underline-text','variable-letter-text','variable-text'] },
  { category: '可视化', section: 'visualization', slugs: ['bending-gallery','carousal-3d','file-tree','github-globe','globe','icon-cloud','infinite-grid','light-speed','liquid-glass','liquid-logo','logo-cloud','logo-origami','orbit','spline','world-map'] }
];

const FAMILY_ICON: Record<CreativeFamily, string> = {
  card: '◇', device: '▯', background: '✶', text: 'T', progress: '◔', lens: '⌕', pointer: '↖', effect: '✦', media: '▶', comparison: '↔', copy: '⎘', marquee: '⇄', matrix: '⌗', globe: '◉', button: '→', social: '“', bento: '▦', number: '#', list: '☷', beam: '╱', orbit: '◎', dock: '▬', avatars: '●', iconcloud: '◌', reveal: '▨', confetti: '✺', tree: '⌁', terminal: '⌘', image: '▧', timeline: '◖', theme: '◐', chart: '∿', book: '◲', badge: '◉', color: '◐', kbd: '⌘', input: 'I', spinner: '◌', checkbox: '☑', qr: '▦', upload: '↑', tabs: '▤', modal: '▣', gallery: '▦', tooltip: '▱', loader: '…', calendar: '▦', testimonial: '“'
};

const LABELS: Record<string, string> = {
  'file-upload': '文件上传', 'balance-slider': '平衡滑块', 'color-picker': '颜色选择器', 'halo-search': '光环搜索框', input: '动效输入框',
  'placeholders-and-vanish-input': '占位词消散输入框', 'animated-modal': '动画弹窗', 'animated-tabs': '动画标签页', 'morphing-tabs': '变形标签页',
  'multi-step-loader': '多步骤加载器', 'spring-calendar': '弹簧日历', 'animated-testimonials': '动态客户证言', 'design-testimonials': '设计师证言',
  'testimonial-slider': '证言轮播', 'file-tree': '文件树', 'github-globe': 'GitHub 地球', globe: '三维地球', 'world-map': '世界地图', 'icon-cloud': '图标云'
};

function humanize(slug: string): string {
  return slug.split('-').map((part) => part.toUpperCase() === '3D' ? '3D' : `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`).join(' ');
}

function familyFor(section: string, slug: string): CreativeFamily {
  if (section === 'backgrounds') return slug === 'video-text' ? 'text' : 'background';
  if (section === 'buttons') return 'button';
  if (section === 'cards') return slug.includes('carousel') ? 'gallery' : 'card';
  if (section === 'cursors') return 'pointer';
  if (section === 'device-mocks') return 'device';
  if (section === 'html-in-canvas') return slug === 'html-ascii' ? 'matrix' : 'effect';
  if (slug === 'file-upload') return 'upload';
  if (slug.includes('color-picker')) return 'color';
  if (slug.includes('slider') || slug.includes('progress')) return 'progress';
  if (slug.includes('input') || slug.includes('search')) return 'input';
  if (slug.includes('modal')) return 'modal';
  if (slug.includes('tabs')) return 'tabs';
  if (slug.includes('tooltip')) return 'tooltip';
  if (slug.includes('gallery') || slug.includes('carousel') || slug.includes('images-slider')) return 'gallery';
  if (slug.includes('loader')) return 'loader';
  if (slug.includes('calendar')) return 'calendar';
  if (section === 'testimonials') return 'testimonial';
  if (section === 'text-animations') return slug === 'number-ticker' ? 'number' : 'text';
  if (slug.includes('beam') || slug === 'light-speed') return 'beam';
  if (slug.includes('border') || slug.includes('glowing')) return 'card';
  if (slug.includes('confetti')) return 'confetti';
  if (slug.includes('blur') || slug === 'lens') return 'lens';
  if (slug.includes('reveal') || slug.includes('scratch')) return 'reveal';
  if (slug.includes('list')) return 'list';
  if (slug.includes('bento')) return 'bento';
  if (slug === 'book') return 'book';
  if (slug === 'compare') return 'comparison';
  if (slug.includes('dock') || slug.includes('island')) return 'dock';
  if (slug.includes('marquee') || slug.includes('logo-cloud')) return 'marquee';
  if (slug.includes('timeline')) return 'timeline';
  if (slug === 'file-tree') return 'tree';
  if (slug.includes('globe') || slug.includes('map')) return 'globe';
  if (slug.includes('icon-cloud') || slug.includes('logo-origami')) return 'iconcloud';
  if (slug === 'orbit') return 'orbit';
  if (slug.includes('media') || slug.includes('scroll')) return 'media';
  if (slug.includes('mask') || slug.includes('particle-image')) return 'image';
  return 'effect';
}

export const INSPIRA_BACKGROUND_SLUGS = new Set(INSPIRA_SOURCE.find((group) => group.section === 'backgrounds')?.slugs ?? []);

function propsFor(family: CreativeFamily, slug: string): Record<string, WebDesignJsonValue> {
  if (slug === 'gradient-button') return { sourceComponent: slug, bgColor: '#ffffff' };
  if (family === 'tabs') return { items: ['Overview', 'Motion', 'Accessibility'], activeTab: 'Overview' };
  if (family === 'gallery') return { items: ['Editorial', 'Product', 'People', 'Architecture'], activeIndex: 0 };
  if (family === 'testimonial') return { items: ['The editor preserves every detail.', 'AI and human iteration finally feel natural.', 'Responsive design stays predictable.'] };
  if (family === 'upload') return { accept: 'image/*,.pdf', multiple: true, maxSizeMb: 10, files: [] };
  if (family === 'loader') return { items: ['Analysing brief', 'Composing layout', 'Polishing interaction'], activeStep: 1 };
  if (family === 'calendar') return { selectedDay: 18, events: ['Design review', 'Launch check'] };
  if (family === 'list' || family === 'tree') return { items: ['app', 'components', 'assets', 'design.json'] };
  if (family === 'marquee') return { items: ['Nuxt', 'Vue', 'Motion', 'WebGL'] };
  if (family === 'input') return { placeholder: '描述你想设计的网站…' };
  return { sourceComponent: slug };
}

const INSPIRA_DESCRIPTORS: CreativeComponentDescriptor<InspiraCategory>[] = INSPIRA_SOURCE.flatMap(({ category, section, slugs }) => slugs.map((slug) => {
  const family = familyFor(section, slug);
  return {
    slug,
    label: LABELS[slug] ?? humanize(slug),
    category,
    family,
    icon: FAMILY_ICON[family],
    content: LABELS[slug] ?? humanize(slug),
    props: propsFor(family, slug)
  };
}));

export const INSPIRA_COMPONENT_SLUGS = INSPIRA_SOURCE.flatMap((group) => group.slugs);
assertCreativeCatalog('inspira', INSPIRA_DESCRIPTORS);
export const INSPIRA_COMPONENTS = createCreativeDefinitions(INSPIRA_DESCRIPTORS, 'https://inspira-ui.com/docs/en/components/');
export const INSPIRA_COMPONENT_VARIANTS: Record<string, UiComponentVariant[]> = Object.fromEntries(
  Object.entries(INSPIRA_OFFICIAL_COMPONENT_VARIANTS).map(([componentId, variants]) => [componentId, variants.map((variant) => ({ ...variant }))])
);

for (const component of INSPIRA_COMPONENTS) {
  const slug = component.docsUrl?.split('/').at(-1) ?? '';
  const source = INSPIRA_SOURCE.find((group) => (group.slugs as readonly string[]).includes(slug));
  if (source) component.docsUrl = `https://inspira-ui.com/docs/en/components/${source.section}/${slug}`;
}

export const INSPIRA_LIBRARY: UiLibraryCatalog<InspiraCategory> = {
  id: 'inspira', displayName: 'Inspira UI', shortName: 'Inspira', version: INSPIRA_VERSION, brandMark: 'I',
  categories: INSPIRA_CATEGORIES, components: INSPIRA_COMPONENTS, variants: INSPIRA_COMPONENT_VARIANTS,
  license: INSPIRA_LICENSE, sourceUrl: 'https://github.com/rahulv-official/inspira-ui', licenseUrl: 'https://github.com/rahulv-official/inspira-ui/blob/main/LICENSE'
};

export function createInspiraComponent(definitionId: string, x: number, y: number) { return createUiLibraryComponent(INSPIRA_LIBRARY, definitionId, x, y); }
export function variantsForInspiraComponent(definitionId: string) { return variantsForUiComponent(INSPIRA_LIBRARY, definitionId); }
export function applyInspiraComponentVariant(component: Parameters<typeof applyUiComponentVariant>[1], variantId: string) { return applyUiComponentVariant(INSPIRA_LIBRARY, component, variantId); }
