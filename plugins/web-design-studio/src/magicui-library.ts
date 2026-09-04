import { assertCreativeCatalog, createCreativeDefinitions, createCreativeVariants, type CreativeComponentDescriptor } from './creative-library.js';
import { applyUiComponentVariant, createUiLibraryComponent, variantsForUiComponent, type UiLibraryCatalog } from './ui-library.js';

export const MAGICUI_VERSION = 'registry-2026.09';
export const MAGICUI_LICENSE = 'MIT';
export const MAGICUI_CATEGORIES = ['展示组件', '交互效果', '视觉特效', '文字动画', '设备模型', '按钮', '背景'] as const;
export type MagicUiCategory = (typeof MAGICUI_CATEGORIES)[number];

const MAGICUI_DESCRIPTORS = [
  { slug: 'magic-card', label: '魔法聚光卡片', category: '展示组件', family: 'card', icon: '✦', content: '鼠标移动时呈现聚光与渐变边缘' },
  { slug: 'hero-video-dialog', label: '主视觉视频弹窗', category: '展示组件', family: 'media', icon: '▶', content: '观看产品设计演示' },
  { slug: 'code-comparison', label: '代码差异比较', category: '展示组件', family: 'comparison', icon: '⟷', content: '拖动查看重构前后的代码差异' },
  { slug: 'marquee', label: '无限跑马灯', category: '展示组件', family: 'marquee', icon: '↔', content: '客户品牌与产品能力持续滚动' },
  { slug: 'globe', label: '交互式地球', category: '展示组件', family: 'globe', icon: '◉', content: '连接世界各地的团队与用户', width: 460, height: 390 },
  { slug: 'tweet-card', label: '推文展示卡片', category: '展示组件', family: 'social', icon: '𝕏', content: '刚刚用 AI 完成了一整个网站设计。' },
  { slug: 'client-tweet-card', label: '客户端推文卡片', category: '展示组件', family: 'social', icon: '𝕏', content: '实时加载并展示社交内容。' },
  { slug: 'bento-grid', label: 'Bento 功能宫格', category: '展示组件', family: 'bento', icon: '▦', content: '用错落卡片展示产品核心能力', width: 560, height: 350 },
  { slug: 'animated-list', label: '动画通知列表', category: '展示组件', family: 'list', icon: '☷', content: '实时展示保存、协作与发布动态' },
  { slug: 'dock', label: '悬浮程序坞', category: '展示组件', family: 'dock', icon: '▰', content: '常用设计工具快速入口' },
  { slug: 'avatar-circles', label: '头像堆叠群组', category: '展示组件', family: 'avatars', icon: '●', content: '12 位成员正在共同设计' },
  { slug: 'icon-cloud', label: '三维图标云', category: '展示组件', family: 'iconcloud', icon: '◌', content: 'React · Vue · Figma · AI' },
  { slug: 'file-tree', label: '项目文件树', category: '展示组件', family: 'tree', icon: '⌁', content: '网站项目资源结构', props: { items: ['app', 'components', 'assets', 'package.json'] } },
  { slug: 'terminal', label: '命令终端', category: '展示组件', family: 'terminal', icon: '⌘', content: '正在生成响应式网站…' },
  { slug: 'dotted-map', label: '点阵世界地图', category: '展示组件', family: 'globe', icon: '∷', content: '用点阵与连线展示全球业务节点' },
  { slug: 'backlight', label: '媒体背光', category: '展示组件', family: 'media', icon: '◐', content: '根据图像与视频色彩生成环境背光' },

  { slug: 'lens', label: '图片放大镜', category: '交互效果', family: 'lens', icon: '⌕', content: '悬停查看设计细节' },
  { slug: 'pointer', label: '自定义指针', category: '交互效果', family: 'pointer', icon: '↖', content: '跟随鼠标的品牌化指针' },
  { slug: 'smooth-cursor', label: '平滑光标', category: '交互效果', family: 'pointer', icon: '⌁', content: '带惯性轨迹的顺滑光标' },
  { slug: 'progressive-blur', label: '渐进模糊', category: '交互效果', family: 'lens', icon: '◒', content: '内容边缘逐渐模糊过渡' },
  { slug: 'scroll-progress', label: '滚动进度', category: '交互效果', family: 'progress', icon: '━', content: '页面阅读进度 68%' },
  { slug: 'confetti', label: '礼花庆祝', category: '交互效果', family: 'confetti', icon: '✺', content: '发布成功！' },
  { slug: 'cool-mode', label: '点击彩蛋模式', category: '交互效果', family: 'confetti', icon: '★', content: '点击触发品牌粒子彩蛋' },
  { slug: 'animated-theme-toggler', label: '动画主题切换', category: '交互效果', family: 'theme', icon: '◐', content: '切换明亮与深色主题' },

  { slug: 'neon-gradient-card', label: '霓虹渐变卡片', category: '视觉特效', family: 'card', icon: '◇', content: '流动的霓虹渐变边框与光晕' },
  { slug: 'glare-hover', label: '悬浮眩光', category: '视觉特效', family: 'card', icon: '◒', content: '指针经过时一束眩光横扫内容' },
  { slug: 'meteors', label: '流星特效', category: '视觉特效', family: 'effect', icon: '☄', content: '夜空中划过的动态流星' },
  { slug: 'particles', label: '粒子背景', category: '视觉特效', family: 'effect', icon: '⁙', content: '轻盈漂浮的空间粒子' },
  { slug: 'ripple', label: '同心波纹', category: '视觉特效', family: 'effect', icon: '◎', content: '从中心持续扩散的波纹' },
  { slug: 'border-beam', label: '边框流光', category: '视觉特效', family: 'beam', icon: '▱', content: '沿容器边缘运动的高光' },
  { slug: 'animated-beam', label: '节点连接光束', category: '视觉特效', family: 'beam', icon: '↝', content: '连接多个产品节点的动画光束' },
  { slug: 'orbiting-circles', label: '环绕圆轨', category: '视觉特效', family: 'orbit', icon: '◉', content: '图标围绕核心能力持续旋转' },
  { slug: 'shine-border', label: '闪耀描边', category: '视觉特效', family: 'card', icon: '▱', content: '多色高光沿边框流动' },
  { slug: 'animated-circular-progress-bar', label: '动画环形进度', category: '视觉特效', family: 'progress', icon: '◔', content: '项目完成度 78%' },
  { slug: 'glyph-matrix', label: '字符矩阵', category: '视觉特效', family: 'matrix', icon: '⌗', content: '由字符组成的动态数据矩阵' },
  { slug: 'light-rays', label: '光线幕布', category: '视觉特效', family: 'beam', icon: '╱', content: '多束柔和光线从上方穿过页面' },
  { slug: 'warp-background', label: '时空扭曲背景', category: '视觉特效', family: 'background', icon: '✧', content: '向视点汇聚的动态透视网格' },

  { slug: 'line-shadow-text', label: '线性阴影文字', category: '文字动画', family: 'text', icon: 'T', content: 'Build remarkable products' },
  { slug: 'aurora-text', label: '极光渐变文字', category: '文字动画', family: 'text', icon: 'A', content: 'Design without limits' },
  { slug: 'morphing-text', label: '变形轮播文字', category: '文字动画', family: 'text', icon: 'M', content: '设计 · 构建 · 发布' },
  { slug: 'number-ticker', label: '数字滚动器', category: '文字动画', family: 'number', icon: '#', content: '128,420' },
  { slug: 'animated-shiny-text', label: '流光文字', category: '文字动画', family: 'text', icon: '✦', content: 'AI 正在为你设计网站' },
  { slug: 'text-reveal', label: '滚动文字揭示', category: '文字动画', family: 'text', icon: 'T', content: '每一次滚动，都揭示新的产品故事' },
  { slug: 'hyper-text', label: '字符扰动文字', category: '文字动画', family: 'text', icon: 'H', content: 'HYPER INTERFACE' },
  { slug: 'animated-gradient-text', label: '动画渐变文字', category: '文字动画', family: 'text', icon: 'G', content: 'A new way to create' },
  { slug: 'word-rotate', label: '词语轮播', category: '文字动画', family: 'text', icon: '↻', content: '快速 · 好看 · 可编辑' },
  { slug: 'typing-animation', label: '打字机文字', category: '文字动画', family: 'text', icon: '⌨', content: 'Tell AI what you want to build…' },
  { slug: 'sparkles-text', label: '闪光文字', category: '文字动画', family: 'text', icon: '✧', content: 'Create something magical' },
  { slug: 'spinning-text', label: '环形旋转文字', category: '文字动画', family: 'text', icon: '◌', content: 'DESIGN · BUILD · SHIP ·' },
  { slug: 'text-3d-flip', label: '3D 翻转文字', category: '文字动画', family: 'text', icon: '⇵', content: 'Future of web design' },
  { slug: 'comic-text', label: '漫画标题文字', category: '文字动画', family: 'text', icon: 'B', content: 'WOW! DESIGN!' },
  { slug: 'kinetic-text', label: '动力排版文字', category: '文字动画', family: 'text', icon: 'K', content: 'MOVE WITH PURPOSE' },
  { slug: 'text-animate', label: '多模式文字动画', category: '文字动画', family: 'text', icon: 'T', content: 'Animate every word with purpose' },
  { slug: 'scroll-based-velocity', label: '滚动速度文字', category: '文字动画', family: 'text', icon: '↠', content: 'CREATIVE DEVELOPMENT —' },
  { slug: 'blur-fade', label: '模糊淡入', category: '文字动画', family: 'text', icon: '◒', content: '从模糊中清晰出现' },
  { slug: 'video-text', label: '视频填充文字', category: '文字动画', family: 'text', icon: '▶', content: 'MOTION' },
  { slug: 'highlighter', label: '手绘高亮文字', category: '文字动画', family: 'text', icon: '▰', content: '让关键价值真正被看见' },
  { slug: 'dia-text-reveal', label: '对角文字揭示', category: '文字动画', family: 'text', icon: '◩', content: '文字沿对角切面逐步揭示' },

  { slug: 'android', label: 'Android 手机模型', category: '设备模型', family: 'device', icon: '▯', content: 'Android 产品预览' },
  { slug: 'safari', label: 'Safari 浏览器模型', category: '设备模型', family: 'device', icon: '▭', content: '桌面网站真实浏览器预览', width: 520, height: 330 },
  { slug: 'iphone', label: 'iPhone 模型', category: '设备模型', family: 'device', icon: '▯', content: '移动端产品预览', width: 260, height: 460 },
  { slug: 'pixel-image', label: '像素化图片', category: '设备模型', family: 'image', icon: '▦', content: '由像素块逐步还原的产品图片' },

  { slug: 'shimmer-button', label: '微光按钮', category: '按钮', family: 'button', icon: '✦', content: '开始设计' },
  { slug: 'shiny-button', label: '闪亮按钮', category: '按钮', family: 'button', icon: '◇', content: '立即体验' },
  { slug: 'animated-subscribe-button', label: '订阅状态按钮', category: '按钮', family: 'button', icon: '✓', content: '订阅更新' },
  { slug: 'pulsating-button', label: '脉冲按钮', category: '按钮', family: 'button', icon: '◉', content: '查看在线演示' },
  { slug: 'ripple-button', label: '水波按钮', category: '按钮', family: 'button', icon: '◎', content: '创建新项目' },
  { slug: 'rainbow-button', label: '彩虹按钮', category: '按钮', family: 'button', icon: '◒', content: '生成网站' },
  { slug: 'interactive-hover-button', label: '悬浮箭头按钮', category: '按钮', family: 'button', icon: '→', content: '探索更多' },

  { slug: 'grid-pattern', label: '基础网格背景', category: '背景', family: 'background', icon: '▦', content: '精确、克制的设计网格' },
  { slug: 'striped-pattern', label: '条纹纹理背景', category: '背景', family: 'background', icon: '▥', content: '斜向条纹营造层次' },
  { slug: 'interactive-grid-pattern', label: '交互网格背景', category: '背景', family: 'background', icon: '⌗', content: '鼠标经过时点亮网格单元' },
  { slug: 'dot-pattern', label: '点阵背景', category: '背景', family: 'background', icon: '⁙', content: '轻量点阵衬托主体内容' },
  { slug: 'flickering-grid', label: '闪烁网格背景', category: '背景', family: 'background', icon: '▦', content: '随机闪烁的数据网格' },
  { slug: 'animated-grid-pattern', label: '动画网格图案', category: '背景', family: 'background', icon: '▧', content: '网格单元依次流动点亮' },
  { slug: 'retro-grid', label: '复古透视网格', category: '背景', family: 'background', icon: '⌁', content: '具有纵深感的复古地平线' },
  { slug: 'hexagon-pattern', label: '六边形网格', category: '背景', family: 'background', icon: '⬡', content: '可调间距与描边的六边形纹理' },
  { slug: 'noise-texture', label: '噪点纹理', category: '背景', family: 'background', icon: '∷', content: '为界面叠加细腻的胶片颗粒质感' }
] as const satisfies readonly CreativeComponentDescriptor<MagicUiCategory>[];

assertCreativeCatalog('magicui', MAGICUI_DESCRIPTORS);
export const MAGICUI_COMPONENTS = createCreativeDefinitions(MAGICUI_DESCRIPTORS, 'https://magicui.design/docs/components/');
export const MAGICUI_COMPONENT_VARIANTS = createCreativeVariants(MAGICUI_DESCRIPTORS);

export const MAGICUI_LIBRARY: UiLibraryCatalog<MagicUiCategory> = {
  id: 'magicui', displayName: 'Magic UI', shortName: 'Magic', version: MAGICUI_VERSION, brandMark: 'M',
  categories: MAGICUI_CATEGORIES, components: MAGICUI_COMPONENTS, variants: MAGICUI_COMPONENT_VARIANTS,
  license: MAGICUI_LICENSE, sourceUrl: 'https://github.com/magicuidesign/magicui', licenseUrl: 'https://github.com/magicuidesign/magicui/blob/main/LICENSE.md'
};

export function createMagicUiComponent(definitionId: string, x: number, y: number) { return createUiLibraryComponent(MAGICUI_LIBRARY, definitionId, x, y); }
export function variantsForMagicUiComponent(definitionId: string) { return variantsForUiComponent(MAGICUI_LIBRARY, definitionId); }
export function applyMagicUiComponentVariant(component: Parameters<typeof applyUiComponentVariant>[1], variantId: string) { return applyUiComponentVariant(MAGICUI_LIBRARY, component, variantId); }
