import type { WebComponentType, WebDesignJsonValue, WebDesignLibraryName } from './schema.js';
import { defineUiComponent, type UiComponentDefinition, type UiComponentVariant } from './ui-library.js';

export type CreativeFamily =
  | 'card' | 'device' | 'background' | 'text' | 'progress' | 'lens' | 'pointer' | 'effect'
  | 'media' | 'comparison' | 'copy' | 'marquee' | 'matrix' | 'globe' | 'button' | 'social'
  | 'bento' | 'number' | 'list' | 'beam' | 'orbit' | 'dock' | 'avatars' | 'iconcloud'
  | 'reveal' | 'confetti' | 'tree' | 'terminal' | 'image' | 'timeline' | 'theme'
  | 'chart' | 'book' | 'badge' | 'color' | 'kbd' | 'input' | 'spinner' | 'checkbox' | 'qr'
  | 'upload' | 'tabs' | 'modal' | 'gallery' | 'tooltip' | 'loader' | 'calendar' | 'testimonial';

export interface CreativeComponentDescriptor<TCategory extends string = string> {
  slug: string;
  label: string;
  category: TCategory;
  family: CreativeFamily;
  icon: string;
  width?: number;
  height?: number;
  content?: string;
  props?: Record<string, WebDesignJsonValue>;
  variants?: readonly UiComponentVariant[];
  /**
   * Only list modes that represent a deliberately designed, visibly distinct
   * example for this specific component. A component is no longer expanded to
   * three cosmetic presets merely because the shared renderer supports them.
   */
  showcaseModes?: readonly ('signature' | 'immersive' | 'editorial')[];
}

const BASE_TYPE_BY_FAMILY: Record<CreativeFamily, WebComponentType> = {
  card: 'card', device: 'card', background: 'section', text: 'heading', progress: 'card', lens: 'image', pointer: 'section', effect: 'section',
  media: 'video', comparison: 'card', copy: 'card', marquee: 'section', matrix: 'section', globe: 'card', button: 'button', social: 'card',
  bento: 'section', number: 'heading', list: 'list', beam: 'section', orbit: 'section', dock: 'card', avatars: 'avatar', iconcloud: 'section',
  reveal: 'card', confetti: 'section', tree: 'list', terminal: 'card', image: 'image', timeline: 'list', theme: 'switch', chart: 'card', book: 'card',
  badge: 'badge', color: 'card', kbd: 'card', input: 'input', spinner: 'card', checkbox: 'checkbox', qr: 'image',
  upload: 'input', tabs: 'section', modal: 'card', gallery: 'section', tooltip: 'card', loader: 'card', calendar: 'card', testimonial: 'card'
};

const SIZE_BY_FAMILY: Record<CreativeFamily, [number, number]> = {
  card: [380, 230], device: [420, 300], background: [520, 260], text: [440, 150], progress: [280, 210], lens: [380, 250], pointer: [420, 240], effect: [460, 250],
  media: [480, 280], comparison: [500, 280], copy: [430, 160], marquee: [520, 150], matrix: [430, 250], globe: [430, 360], button: [230, 70], social: [400, 230],
  bento: [520, 330], number: [300, 120], list: [400, 280], beam: [500, 280], orbit: [400, 340], dock: [420, 110], avatars: [300, 90], iconcloud: [390, 330],
  reveal: [420, 240], confetti: [440, 250], tree: [380, 290], terminal: [480, 300], image: [420, 280], timeline: [520, 280], theme: [190, 70], chart: [560, 310], book: [420, 300],
  badge: [220, 72], color: [380, 220], kbd: [360, 120], input: [380, 100], spinner: [260, 130], checkbox: [280, 80], qr: [260, 280],
  upload: [460, 300], tabs: [500, 280], modal: [480, 320], gallery: [520, 330], tooltip: [360, 180], loader: [420, 250], calendar: [420, 320], testimonial: [480, 270]
};

const MODE_LABELS: Record<CreativeFamily, [string, string, string]> = {
  card: ['聚光卡片', '深色信息卡', '分栏卡片'], device: ['明亮设备', '深色设备', '紧凑设备'], background: ['柔光背景', '暗色背景', '高对比背景'],
  text: ['主视觉文字', '分层文字', '编辑式文字'], progress: ['圆形进度', '横向进度', '分段进度'], lens: ['圆形放大镜', '跟随聚焦', '分屏细节'],
  pointer: ['光标跟随', '目标吸附', '协作指针'], effect: ['柔和特效', '深色特效', '高能特效'], media: ['视频封面', '剧院模式', '内联播放'],
  comparison: ['左右比较', '代码差异', '上下比较'], copy: ['命令复制', '代码复制', '成功状态'], marquee: ['品牌跑马灯', '双向跑马灯', '卡片跑马灯'],
  matrix: ['字符矩阵', '数据矩阵', '霓虹矩阵'], globe: ['全球网络', '数据地球', '深空地球'], button: ['高亮按钮', '图标按钮', '宽幅按钮'],
  social: ['内容卡片', '深色卡片', '紧凑卡片'], bento: ['功能宫格', '数据宫格', '图文宫格'], number: ['大号计数', '指标计数', '分组数字'],
  list: ['通知列表', '活动列表', '紧凑列表'], beam: ['水平连接', '辐射连接', '流程连接'], orbit: ['单轨环绕', '双轨环绕', '网络环绕'],
  dock: ['应用程序坞', '工具程序坞', '悬浮程序坞'], avatars: ['成员头像', '堆叠头像', '在线头像'], iconcloud: ['技术图标云', '品牌图标云', '球形图标云'],
  reveal: ['滑入揭示', '遮罩揭示', '分步揭示'], confetti: ['庆祝礼花', '彩带喷射', '轻量粒子'], tree: ['项目文件树', '代码文件树', '资源目录树'],
  terminal: ['命令终端', '部署终端', 'AI 终端'], image: ['像素图像', '产品图像', '编辑图像'], timeline: ['产品时间线', '发布路线图', '交互时间线'],
  theme: ['明暗切换', '图标切换', '分段切换'], chart: ['市场折线图', '面积趋势图', '极简走势图'], book: ['透视书本', '展开书本', '封面书本'],
  badge: ['状态徽章', '数字徽章', '图标徽章'], color: ['色板选择器', '渐变选择器', '品牌色选择器'], kbd: ['快捷键组合', '命令序列', '键盘提示'],
  input: ['标签输入框', '聚焦输入框', '错误输入框'], spinner: ['环形加载', '条形加载', '带文字加载'], checkbox: ['未选择', '已选择', '多选任务'], qr: ['基础二维码', '品牌二维码', '分享二维码'],
  upload: ['拖放上传', '文件队列', '媒体上传'], tabs: ['线性标签页', '空间导航', '编辑式分页'], modal: ['居中弹窗', '沉浸式对话框', '编辑式面板'],
  gallery: ['图片画廊', '沉浸式轮播', '杂志式图集'], tooltip: ['基础提示', '信息浮层', '带指标提示'], loader: ['步骤加载', '全屏加载', '进度任务'],
  calendar: ['月历日程', '活动日历', '编辑式日程'], testimonial: ['客户评价', '视频证言', '杂志式评论']
};

export function creativeComponentId(slug: string): string {
  return slug.split('-').map((part) => {
    if (!part) return '';
    if (/^3d$/i.test(part)) return 'ThreeD';
    if (/^[0-9]/.test(part)) return `N${part}`;
    return `${part[0].toUpperCase()}${part.slice(1)}`;
  }).join('');
}

export function createCreativeDefinitions<TCategory extends string>(
  descriptors: readonly CreativeComponentDescriptor<TCategory>[],
  docsBaseUrl: string
): UiComponentDefinition<TCategory>[] {
  return descriptors.map((descriptor) => {
    const id = creativeComponentId(descriptor.slug);
    const [defaultWidth, defaultHeight] = SIZE_BY_FAMILY[descriptor.family];
    return {
      ...defineUiComponent(
        id,
        descriptor.label,
        descriptor.category,
        descriptor.icon,
        BASE_TYPE_BY_FAMILY[descriptor.family],
        descriptor.width ?? defaultWidth,
        descriptor.height ?? defaultHeight,
        descriptor.content ?? descriptor.label,
        {
          family: descriptor.family,
          componentSlug: descriptor.slug,
          title: descriptor.label,
          description: `${descriptor.label} 的可编辑演示`,
          accent: '#7C3AED',
          items: ['设计', '动效', '交互'],
          values: [32, 48, 41, 68, 57, 82],
          ...descriptor.props
        },
        [descriptor.slug, descriptor.family, '动画', '创意', '营销页']
      ),
      docsUrl: `${docsBaseUrl}${descriptor.slug}`
    };
  });
}

export function createCreativeVariants<TCategory extends string>(
  descriptors: readonly CreativeComponentDescriptor<TCategory>[]
): Record<string, UiComponentVariant[]> {
  return Object.fromEntries(descriptors.map((descriptor) => {
    if (descriptor.variants?.length) return [creativeComponentId(descriptor.slug), descriptor.variants.map((variant) => ({ ...variant, props: { ...variant.props } }))];
    const [first, second, third] = MODE_LABELS[descriptor.family];
    const [width, height] = SIZE_BY_FAMILY[descriptor.family];
    const variants: Record<'signature' | 'immersive' | 'editorial', UiComponentVariant> = {
      signature: { id: 'signature', label: first, props: { mode: 'signature', tone: 'light', motion: true } },
      immersive: { id: 'immersive', label: second, props: { mode: 'immersive', tone: 'dark', motion: true }, width: descriptor.width ?? width, height: Math.max(descriptor.height ?? height, height) },
      editorial: { id: 'editorial', label: third, props: { mode: 'editorial', tone: 'vivid', motion: false }, width: Math.max(180, (descriptor.width ?? width) - 30), height: Math.max(64, (descriptor.height ?? height) - 20) }
    };
    const modes = descriptor.showcaseModes?.length ? descriptor.showcaseModes : ['signature'] as const;
    return [creativeComponentId(descriptor.slug), modes.map((mode) => variants[mode])];
  }));
}

export function assertCreativeCatalog(libraryId: WebDesignLibraryName, descriptors: readonly CreativeComponentDescriptor[]): void {
  const ids = descriptors.map((descriptor) => creativeComponentId(descriptor.slug));
  if (new Set(ids).size !== ids.length) throw new Error(`${libraryId} contains duplicate component IDs.`);
}
