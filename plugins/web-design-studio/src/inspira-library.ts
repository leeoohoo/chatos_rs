import { assertCreativeCatalog, createCreativeDefinitions, createCreativeVariants, type CreativeComponentDescriptor, type CreativeFamily } from './creative-library.js';
import { applyUiComponentVariant, createUiLibraryComponent, variantsForUiComponent, type UiComponentVariant, type UiLibraryCatalog } from './ui-library.js';
import type { WebDesignJsonValue } from './schema.js';

export const INSPIRA_VERSION = 'docs-2026.09';
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

const backgroundVariant = (id: string, label: string, props: Record<string, WebDesignJsonValue>): UiComponentVariant => ({ id, label, props });

const INSPIRA_BACKGROUND_VARIANTS: Record<string, UiComponentVariant[]> = {
  'aurora-background': [backgroundVariant('radial', '径向极光', { radialGradient: true }), backgroundVariant('curtain', '幕布极光', { radialGradient: false })],
  'black-hole-background': [backgroundVariant('tunnel', '黑洞隧道', { strokeColor: '#737373', numberOfLines: 50, numberOfDiscs: 50, particleColor: '#ffffff' }), backgroundVariant('dense', '高密度引力场', { strokeColor: '#8b5cf6', numberOfLines: 72, numberOfDiscs: 64, particleColor: '#c4b5fd' })],
  'bubbles-bg': [backgroundVariant('clear', '清晰气泡', { blur: 0 }), backgroundVariant('soft', '柔焦气泡', { blur: 8 })],
  'cosmic-portal': [backgroundVariant('violet', '紫色星际门户', { portalComplexity: 4, crystalCount: 12, primaryColor: '#9b59b6', secondaryColor: '#3498db', accentColor: '#e74c3c', vortexColor: '#2ecc71', rotationSpeed: 0.3, bloomStrength: 1.2 }), backgroundVariant('blue', '蓝色能量门户', { portalComplexity: 5, crystalCount: 8, primaryColor: '#2563eb', secondaryColor: '#22d3ee', accentColor: '#f59e0b', vortexColor: '#14b8a6', rotationSpeed: 0.45, bloomStrength: 1.5 }), backgroundVariant('dense', '高复杂度门户', { portalComplexity: 7, crystalCount: 18, primaryColor: '#7c3aed', secondaryColor: '#ec4899', accentColor: '#fb7185', vortexColor: '#38bdf8', rotationSpeed: 0.2, bloomStrength: 1.8 })],
  'falling-stars': [backgroundVariant('shower', '流星雨', { color: '#ffffff', count: 200 }), backgroundVariant('gold', '金色流星', { color: '#fbbf24', count: 120 })],
  'flickering-grid': [backgroundVariant('subtle', '细密微光网格', { squareSize: 4, gridGap: 6, flickerChance: 0.3, color: '#6366f1', maxOpacity: 0.2 }), backgroundVariant('bold', '高对比闪烁网格', { squareSize: 8, gridGap: 5, flickerChance: 0.55, color: '#22d3ee', maxOpacity: 0.55 })],
  'interactive-grid-pattern': [backgroundVariant('mono', '单色交互网格', { cellWidth: 40, cellHeight: 40, columns: 12, rows: 8, squareColor: '#94a3b8' }), backgroundVariant('colored', '彩色交互网格', { cellWidth: 34, cellHeight: 34, columns: 14, rows: 9, squareColor: '#8b5cf6' })],
  'lamp-effect': [backgroundVariant('focused', '聚焦灯光', { delay: 0.5, duration: 0.8, beamWidth: 46 }), backgroundVariant('wide', '宽幅灯幕', { delay: 0.2, duration: 1.2, beamWidth: 72 })],
  'liquid-background': [backgroundVariant('violet', '紫蓝液态流体', { primaryColor: '#7c3aed', secondaryColor: '#06b6d4', speed: 1 }), backgroundVariant('sunset', '日落液态流体', { primaryColor: '#f97316', secondaryColor: '#ec4899', speed: 0.7 })],
  'neural-background': [backgroundVariant('cyan', '青色神经流', { hue: 200, saturation: 0.8, chroma: 0.6 }), backgroundVariant('violet', '紫色神经流', { hue: 275, saturation: 0.85, chroma: 0.7 }), backgroundVariant('warm', '暖色神经流', { hue: 18, saturation: 0.9, chroma: 0.62 })],
  'particle-whirlpool-bg': [backgroundVariant('clean', '清晰粒子旋涡', { blur: 0, particleCount: 2000 }), backgroundVariant('soft', '柔焦粒子旋涡', { blur: 5, particleCount: 1600 }), backgroundVariant('dense', '高密度粒子旋涡', { blur: 1, particleCount: 3200 })],
  'particles-bg': [backgroundVariant('balanced', '标准粒子场', { color: '#ffffff', quantity: 100, staticity: 50, ease: 50 }), backgroundVariant('dense', '高密度粒子场', { color: '#a78bfa', quantity: 180, staticity: 36, ease: 40 }), backgroundVariant('reactive', '高响应粒子场', { color: '#22d3ee', quantity: 90, staticity: 18, ease: 18 })],
  'pattern-background': [backgroundVariant('grid', '基础网格', { animate: false, direction: 'top', pattern: 'grid', size: 'md', mask: 'ellipse', speed: 10000 }), backgroundVariant('small-grid', '细密网格', { animate: false, direction: 'top', pattern: 'grid', size: 'sm', mask: 'ellipse', speed: 10000 }), backgroundVariant('dot', '点阵图案', { animate: false, direction: 'top', pattern: 'dot', size: 'md', mask: 'ellipse', speed: 10000 }), backgroundVariant('large-dot', '大号点阵', { animate: false, direction: 'top', pattern: 'dot', size: 'lg', mask: 'ellipse-top', speed: 10000 }), backgroundVariant('animated', '移动图案', { animate: true, direction: 'top-right', pattern: 'grid', size: 'md', mask: 'ellipse', speed: 5000 })],
  'ribbon-background': [backgroundVariant('classic', '经典层叠丝带', { colors: ['#355070', '#6d597a', '#b56576', '#e56b6f', '#eaac8b'], backgroundColor: '#282828', transparent: false, enableShadows: true, angle: 0, speed: 1 }), backgroundVariant('angled', '倾斜彩色丝带', { colors: ['#312e81', '#7c3aed', '#db2777', '#f97316', '#facc15'], backgroundColor: '#09090b', transparent: false, enableShadows: true, angle: -12, speed: 1.25 }), backgroundVariant('transparent', '透明轻量丝带', { colors: ['#38bdf8', '#818cf8', '#c084fc', '#e879f9', '#22d3ee'], backgroundColor: 'transparent', transparent: true, enableShadows: false, angle: 8, speed: 0.65 })],
  ripple: [backgroundVariant('rings', '标准圆形涟漪', { baseCircleSize: 210, baseCircleOpacity: 0.24, spaceBetweenCircle: 70, numberOfCircles: 7, waveSpeed: 80, shape: 'circle' }), backgroundVariant('squared', '方形涟漪', { baseCircleSize: 180, baseCircleOpacity: 0.22, spaceBetweenCircle: 55, numberOfCircles: 7, waveSpeed: 80, shape: 'square' }), backgroundVariant('lines', '线框涟漪', { baseCircleSize: 190, baseCircleOpacity: 0.13, spaceBetweenCircle: 46, numberOfCircles: 9, waveSpeed: 70, shape: 'lines' }), backgroundVariant('blob', '有机形态涟漪', { baseCircleSize: 200, baseCircleOpacity: 0.2, spaceBetweenCircle: 60, numberOfCircles: 6, waveSpeed: 95, shape: 'blob' })],
  'silk-background': [backgroundVariant('violet', '紫色丝绸', { hue: 300, saturation: 0.5, brightness: 1, speed: 1 }), backgroundVariant('ocean', '海洋丝绸', { hue: 205, saturation: 0.72, brightness: 0.9, speed: 0.75 }), backgroundVariant('gold', '金色丝绸', { hue: 38, saturation: 0.78, brightness: 1.1, speed: 1.25 })],
  'singularity-background': [backgroundVariant('neutral', '中性奇点', { hue: 0, saturation: 1, brightness: 1, speed: 1, mouseSensitivity: 0.5, damping: 1 }), backgroundVariant('blue', '蓝色奇点', { hue: 220, saturation: 0.85, brightness: 1.1, speed: 0.8, mouseSensitivity: 1, damping: 0.8 }), backgroundVariant('red', '红色奇点', { hue: 350, saturation: 0.9, brightness: 0.9, speed: 1.3, mouseSensitivity: 0.7, damping: 0.65 })],
  'snowfall-bg': [backgroundVariant('gentle', '轻柔降雪', { color: '#ffffff', quantity: 80, speed: 0.65, minRadius: 1, maxRadius: 3 }), backgroundVariant('heavy', '密集降雪', { color: '#e0f2fe', quantity: 160, speed: 1.2, minRadius: 1, maxRadius: 4 }), backgroundVariant('gold', '金色飘雪', { color: '#fde68a', quantity: 100, speed: 0.8, minRadius: 1, maxRadius: 3 })],
  sparkles: [backgroundVariant('blue', '蓝色闪光场', { background: '#0d47a1', particleColor: '#ffffff', minSize: 1, maxSize: 3, speed: 4, particleDensity: 120 }), backgroundVariant('transparent', '透明闪光层', { background: 'transparent', particleColor: '#a78bfa', minSize: 1, maxSize: 4, speed: 2, particleDensity: 80 })],
  'stars-background': [backgroundVariant('deep-space', '深空星层', { factor: 0.05, speed: 50, starColor: '#ffffff' }), backgroundVariant('violet', '紫色视差星层', { factor: 0.1, speed: 35, starColor: '#c4b5fd' })],
  'stractium-background': [backgroundVariant('mono', '单色有机分形', { hue: 0, saturation: 1, brightness: 1, speed: 1, mouseSensitivity: 0.5, damping: 1 }), backgroundVariant('cyan', '青色有机分形', { hue: 190, saturation: 0.82, brightness: 1.1, speed: 0.75, mouseSensitivity: 0.7, damping: 0.85 }), backgroundVariant('acid', '荧光有机分形', { hue: 105, saturation: 1, brightness: 1.2, speed: 1.3, mouseSensitivity: 0.9, damping: 0.65 })],
  tetris: [backgroundVariant('classic', '经典方块墙', { base: 10, squareColor: '#8b5cf6' }), backgroundVariant('dense', '密集方块墙', { base: 14, squareColor: '#22d3ee' })],
  'thunderstorm-background': [backgroundVariant('storm', '灰蓝雷暴', { hue: 220, saturation: 0.42, brightness: 0.9, speed: 1, mouseSensitivity: 0.5, damping: 1 }), backgroundVariant('violet', '紫色雷暴', { hue: 270, saturation: 0.7, brightness: 1, speed: 1.15, mouseSensitivity: 0.8, damping: 0.8 }), backgroundVariant('violent', '高强度雷暴', { hue: 205, saturation: 0.5, brightness: 1.25, speed: 1.6, mouseSensitivity: 1.1, damping: 0.6 })],
  'video-text': [backgroundVariant('hero', '主视觉视频文字', { fontSize: 120, fontWeight: 800, autoPlay: true, muted: true, loop: true, preload: 'auto' }), backgroundVariant('compact', '紧凑视频文字', { fontSize: 72, fontWeight: 700, autoPlay: true, muted: true, loop: true, preload: 'metadata' })],
  vortex: [backgroundVariant('blue', '蓝色粒子漩涡', { particleCount: 700, rangeY: 100, baseHue: 220, baseSpeed: 0, rangeSpeed: 1.5, baseRadius: 1, rangeRadius: 2, backgroundColor: '#000000' }), backgroundVariant('magenta', '洋红粒子漩涡', { particleCount: 1000, rangeY: 130, baseHue: 305, baseSpeed: 0.2, rangeSpeed: 2, baseRadius: 1, rangeRadius: 2.5, backgroundColor: '#090014' })],
  'warp-background': [backgroundVariant('balanced', '标准透视跃迁', { perspective: 100, beamsPerSide: 3, beamSize: 5, beamDelayMax: 3, beamDelayMin: 0, beamDuration: 3, gridColor: '#475569' }), backgroundVariant('deep', '深透视跃迁', { perspective: 180, beamsPerSide: 5, beamSize: 3, beamDelayMax: 4, beamDelayMin: 0.5, beamDuration: 4, gridColor: '#6366f1' }), backgroundVariant('fast', '高速跃迁', { perspective: 80, beamsPerSide: 7, beamSize: 6, beamDelayMax: 1.5, beamDelayMin: 0, beamDuration: 1.8, gridColor: '#22d3ee' })],
  'wavy-background': [backgroundVariant('ocean', '海洋波浪', { colors: ['#38bdf8', '#818cf8', '#c084fc', '#e879f9', '#22d3ee'], waveWidth: 50, backgroundFill: '#000000', blur: 10, speed: 'fast', waveOpacity: 0.5 }), backgroundVariant('sunset', '日落波浪', { colors: ['#f97316', '#fb7185', '#e879f9', '#8b5cf6', '#312e81'], waveWidth: 42, backgroundFill: '#180b24', blur: 8, speed: 'slow', waveOpacity: 0.65 }), backgroundVariant('thin', '细线波浪', { colors: ['#ffffff', '#94a3b8', '#38bdf8'], waveWidth: 24, backgroundFill: '#020617', blur: 2, speed: 'fast', waveOpacity: 0.38 })]
};

export const INSPIRA_BACKGROUND_SLUGS = new Set(Object.keys(INSPIRA_BACKGROUND_VARIANTS));

function propsFor(family: CreativeFamily, slug: string): Record<string, WebDesignJsonValue> {
  if (INSPIRA_BACKGROUND_VARIANTS[slug]) return { sourceComponent: slug, ...INSPIRA_BACKGROUND_VARIANTS[slug][0].props };
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
    props: propsFor(family, slug),
    variants: INSPIRA_BACKGROUND_VARIANTS[slug]
  };
}));

export const INSPIRA_COMPONENT_SLUGS = INSPIRA_SOURCE.flatMap((group) => group.slugs);
assertCreativeCatalog('inspira', INSPIRA_DESCRIPTORS);
export const INSPIRA_COMPONENTS = createCreativeDefinitions(INSPIRA_DESCRIPTORS, 'https://inspira-ui.com/docs/en/components/');
export const INSPIRA_COMPONENT_VARIANTS = createCreativeVariants(INSPIRA_DESCRIPTORS);

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
