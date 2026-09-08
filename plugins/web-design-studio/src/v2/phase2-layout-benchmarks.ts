import {
  assertSceneDocument,
  createBlankSceneDocument,
  createSceneNodeBase,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneFrameNode,
  type SceneMediaNode,
  type SceneNode,
  type SceneResponsiveNodeOverride,
  type SceneTextNode
} from './scene-schema.js';
import { V2_BASELINE_VIEWPORTS, V2_WEBSITE_BENCHMARKS } from './phase0-baseline.js';

export type Phase2LayoutPattern =
  | 'split-product-hero'
  | 'commerce-catalog'
  | 'editorial-asymmetry'
  | 'agency-mosaic'
  | 'portfolio-index'
  | 'conference-schedule'
  | 'hospitality-gallery'
  | 'healthcare-service-path'
  | 'education-curriculum'
  | 'nonprofit-impact-story'
  | 'property-floorplans'
  | 'developer-docs-shell';

export interface Phase2LayoutBenchmark {
  benchmarkId: string;
  pattern: Phase2LayoutPattern;
  document: SceneDocument;
  rootNodeId: string;
  contentNodeId: string;
  structuralSignature: string;
}

type AutoOptions = {
  sizingX?: 'fixed' | 'hug' | 'fill';
  sizingY?: 'fixed' | 'hug' | 'fill';
  width?: number;
  height?: number;
  maxWidth?: number;
  minWidth?: number;
  padding?: number | { top: number; right: number; bottom: number; left: number };
  rowGap?: number;
  columnGap?: number;
  wrap?: boolean;
  align?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
  justify?: 'start' | 'center' | 'end' | 'between' | 'around' | 'evenly';
  role?: string;
  background?: string;
};

type GridOptions = AutoOptions & {
  columns: string[];
  rows?: string[];
  autoFlow?: 'row' | 'column' | 'dense';
};

type TextOptions = {
  sizingX?: 'fixed' | 'hug' | 'fill';
  maxWidth?: number;
  fontSize?: number;
  fontWeight?: number;
  lineHeight?: number;
  role?: string;
  color?: string;
  align?: 'left' | 'center' | 'right' | 'justify';
};

type BenchmarkParts = {
  children: SceneNode[];
  tablet?: SceneResponsiveNodeOverride[];
  mobile?: SceneResponsiveNodeOverride[];
};

const patterns: Record<string, Phase2LayoutPattern> = {
  'saas-product': 'split-product-hero',
  'consumer-commerce': 'commerce-catalog',
  'editorial-magazine': 'editorial-asymmetry',
  'creative-agency': 'agency-mosaic',
  'personal-portfolio': 'portfolio-index',
  'conference-event': 'conference-schedule',
  'hospitality-restaurant': 'hospitality-gallery',
  'healthcare-service': 'healthcare-service-path',
  'education-course': 'education-curriculum',
  'nonprofit-campaign': 'nonprofit-impact-story',
  'real-estate-property': 'property-floorplans',
  'developer-platform': 'developer-docs-shell'
};

function padding(value: AutoOptions['padding'] = 0): { top: number; right: number; bottom: number; left: number } {
  return typeof value === 'number' ? { top: value, right: value, bottom: value, left: value } : value;
}

function frame(id: string, direction: 'horizontal' | 'vertical', children: SceneNode[], options: AutoOptions = {}): SceneFrameNode {
  const base = createSceneNodeBase('frame', id, { x: 0, y: 0, width: options.width ?? 1200, height: options.height ?? 100 });
  return {
    ...base,
    type: 'frame',
    id,
    name: id,
    role: options.role,
    layout: {
      ...base.layout,
      mode: 'auto',
      direction,
      wrap: options.wrap ?? false,
      padding: padding(options.padding),
      gap: { row: options.rowGap ?? 24, column: options.columnGap ?? 24 },
      alignItems: options.align ?? 'start',
      justifyContent: options.justify ?? 'start',
      sizingX: options.sizingX ?? 'fill',
      sizingY: options.sizingY ?? 'hug',
      minWidth: options.minWidth,
      maxWidth: options.maxWidth,
      position: 'flow',
      clipContent: false
    },
    appearance: {
      ...base.appearance,
      fills: options.background ? [{ type: 'solid', visible: true, opacity: 1, color: options.background }] : []
    },
    children
  };
}

function grid(id: string, children: SceneNode[], options: GridOptions): SceneFrameNode {
  const result = frame(id, 'vertical', children, options);
  result.layout.mode = 'grid';
  delete result.layout.direction;
  delete result.layout.wrap;
  result.layout.grid = {
    columns: options.columns,
    rows: options.rows ?? [],
    autoFlow: options.autoFlow ?? 'row'
  };
  return result;
}

function text(id: string, content: string, options: TextOptions = {}): SceneTextNode {
  const base = createSceneNodeBase('text', id, { x: 0, y: 0, width: 360, height: 24 }, 'ai');
  return {
    ...base,
    type: 'text',
    id,
    name: id,
    role: options.role,
    content,
    layout: {
      ...base.layout,
      sizingX: options.sizingX ?? 'fill',
      sizingY: 'hug',
      maxWidth: options.maxWidth,
      position: 'flow'
    },
    appearance: {
      ...base.appearance,
      fills: [{ type: 'solid', visible: true, opacity: 1, color: options.color ?? '#18202a' }],
      typography: {
        fontFamily: 'Inter',
        fontSize: options.fontSize ?? 16,
        fontWeight: options.fontWeight ?? 400,
        lineHeight: options.lineHeight ?? 1.5,
        letterSpacing: 0,
        textAlign: options.align ?? 'left'
      }
    }
  };
}

function media(id: string, ratio: [number, number] = [16, 10]): SceneMediaNode {
  const base = createSceneNodeBase('media', id, { x: 0, y: 0, width: ratio[0] * 80, height: ratio[1] * 80 }, 'ai');
  return {
    ...base,
    type: 'media',
    id,
    name: id,
    role: 'visual-media',
    mediaType: 'image',
    assetId: `asset-${id}`,
    alt: id.replaceAll('-', ' '),
    intrinsicSize: { width: ratio[0] * 100, height: ratio[1] * 100 },
    preserveAspectRatio: true,
    layout: { ...base.layout, sizingX: 'fill', sizingY: 'hug', position: 'flow' },
    appearance: {
      ...base.appearance,
      fills: [{ type: 'solid', visible: true, opacity: 1, color: '#dfe5eb' }],
      radius: { topLeft: 24, topRight: 24, bottomRight: 24, bottomLeft: 24 }
    }
  };
}

function button(id: string, label: string): SceneFrameNode {
  return frame(id, 'horizontal', [text(`${id}-label`, label, { sizingX: 'hug', fontWeight: 650, role: 'button-label' })], {
    sizingX: 'hug', sizingY: 'hug', padding: { top: 12, right: 18, bottom: 12, left: 18 },
    rowGap: 0, columnGap: 0, align: 'center', justify: 'center', role: 'button', background: '#e9eef4'
  });
}

function nav(prefix: string, brand: string, links: string[]): SceneFrameNode {
  return frame(`${prefix}-nav`, 'horizontal', [
    text(`${prefix}-brand`, brand, { sizingX: 'hug', fontSize: 20, fontWeight: 760, role: 'brand' }),
    ...links.map((link, index) => text(`${prefix}-nav-link-${index + 1}`, link, { sizingX: 'hug', fontSize: 14, role: 'navigation-link' })),
    button(`${prefix}-nav-action`, '开始')
  ], { wrap: true, align: 'center', justify: 'between', columnGap: 20, rowGap: 12, role: 'primary-navigation' });
}

function heading(id: string, eyebrow: string, title: string, body: string): SceneFrameNode {
  return frame(id, 'vertical', [
    text(`${id}-eyebrow`, eyebrow, { sizingX: 'hug', fontSize: 13, fontWeight: 700, role: 'eyebrow', color: '#4f6478' }),
    text(`${id}-title`, title, { fontSize: 56, fontWeight: 760, lineHeight: 1.05, role: 'heading' }),
    text(`${id}-body`, body, { maxWidth: 680, fontSize: 18, role: 'body-copy', color: '#4b5967' })
  ], { rowGap: 14, role: 'heading-group' });
}

function card(id: string, title: string, body: string): SceneFrameNode {
  return frame(id, 'vertical', [
    text(`${id}-title`, title, { fontSize: 20, fontWeight: 700, role: 'card-title' }),
    text(`${id}-body`, body, { role: 'card-body', color: '#51606f' })
  ], { padding: 24, rowGap: 10, role: 'card', background: '#f4f6f8' });
}

function mergeOverrides(...groups: Array<SceneResponsiveNodeOverride[] | undefined>): SceneResponsiveNodeOverride[] {
  const merged = new Map<string, SceneResponsiveNodeOverride>();
  for (const override of groups.flatMap((group) => group ?? [])) {
    const current = merged.get(override.nodeId);
    merged.set(override.nodeId, {
      ...current,
      ...override,
      layout: current?.layout || override.layout ? {
        ...(current?.layout ?? {}),
        ...(override.layout ?? {}),
        padding: override.layout?.padding ?? current?.layout?.padding,
        gap: override.layout?.gap ?? current?.layout?.gap,
        grid: override.layout?.grid ?? current?.layout?.grid
      } : undefined
    });
  }
  return [...merged.values()];
}

function structuralSignature(document: SceneDocument): string {
  const root = document.pages[0].children[0];
  function visit(node: SceneNode): string {
    const children = isSceneContainer(node) ? node.children : isSceneSlotContainer(node) ? Object.values(node.slots).flat() : [];
    const gridColumns = node.layout.grid?.columns.join(',') ?? '';
    return `${node.type}[${node.role ?? ''}|${node.layout.mode}|${node.layout.direction ?? ''}|${gridColumns}](${children.map(visit).join(';')})`;
  }
  return visit(root);
}

function assemble(benchmarkId: string, pattern: Phase2LayoutPattern, parts: BenchmarkParts): Phase2LayoutBenchmark {
  const document = createBlankSceneDocument(V2_WEBSITE_BENCHMARKS.find((item) => item.id === benchmarkId)!.name);
  document.documentId = `phase2-${benchmarkId}`;
  document.pages[0].id = `page-${benchmarkId}`;
  document.pages[0].name = '首页';
  const contentNodeId = `${benchmarkId}-content`;
  const content = frame(contentNodeId, 'vertical', parts.children, {
    maxWidth: 1440,
    padding: { top: 40, right: 64, bottom: 96, left: 64 },
    rowGap: 96,
    align: 'stretch',
    role: 'bounded-page-content'
  });
  const rootNodeId = `${benchmarkId}-root`;
  const root = frame(rootNodeId, 'vertical', [content], {
    padding: 0,
    rowGap: 0,
    align: 'center',
    role: 'viewport-background',
    background: '#fbfcfd'
  });
  document.pages[0].children = [root];
  document.responsiveRules = [
    {
      id: `${benchmarkId}-tablet`,
      name: 'Tablet composition',
      maxWidth: 1024,
      variableModes: {},
      nodeOverrides: mergeOverrides([{
        nodeId: contentNodeId,
        layout: { padding: { top: 32, right: 40, bottom: 72, left: 40 }, gap: { row: 72, column: 24 } }
      }], parts.tablet)
    },
    {
      id: `${benchmarkId}-mobile`,
      name: 'Mobile composition',
      maxWidth: 700,
      variableModes: {},
      nodeOverrides: mergeOverrides([{
        nodeId: contentNodeId,
        layout: { padding: { top: 24, right: 20, bottom: 56, left: 20 }, gap: { row: 56, column: 16 } }
      }], parts.mobile)
    }
  ];
  assertSceneDocument(document);
  return { benchmarkId, pattern, document, rootNodeId, contentNodeId, structuralSignature: structuralSignature(document) };
}

function saas(): BenchmarkParts {
  const prefix = 'saas-product';
  const heroCopy = frame(`${prefix}-hero-copy`, 'vertical', [
    heading(`${prefix}-hero-heading`, 'AI 工作空间', '从意图直接抵达可编辑的网站', 'AI 构建真实结构，人只需审阅、批注和锁定关键设计决策。'),
    frame(`${prefix}-hero-actions`, 'horizontal', [button(`${prefix}-try`, '免费试用'), button(`${prefix}-demo`, '预约演示')], { wrap: true, sizingX: 'hug', columnGap: 12 })
  ], { rowGap: 28 });
  const hero = frame(`${prefix}-hero`, 'horizontal', [heroCopy, media(`${prefix}-product-ui`, [16, 11])], { columnGap: 64, align: 'center', role: 'split-hero' });
  const capability = grid(`${prefix}-capabilities`, [
    card(`${prefix}-cap-1`, '结构化生成', '生成递归场景树，而不是一张无法继续编辑的截图。'),
    card(`${prefix}-cap-2`, '人工锁定', '设计师确认过的字段不会被后续 AI 指令覆盖。'),
    card(`${prefix}-cap-3`, '连续响应', '同一节点树在手机、桌面、4K 和 8K 上自然重排。')
  ], { columns: ['repeat(auto-fit, minmax(240px, 1fr))'], columnGap: 24, rowGap: 24, role: 'capability-grid' });
  const pricing = grid(`${prefix}-pricing`, [
    card(`${prefix}-price-1`, '个人版', '适合独立设计与快速提案。'),
    card(`${prefix}-price-2`, '团队版', '共享变量、组件和审阅工作流。'),
    card(`${prefix}-price-3`, '企业版', '私有运行时、治理与审计能力。')
  ], { columns: ['repeat(auto-fit, minmax(260px, 1fr))'], columnGap: 24, rowGap: 24, role: 'pricing-grid' });
  return { children: [nav(prefix, 'Arc Studio', ['产品', '案例', '定价']), hero, capability, pricing], tablet: [{ nodeId: hero.id, layout: { direction: 'vertical' } }] };
}

function commerce(): BenchmarkParts {
  const prefix = 'consumer-commerce';
  const catalog = grid(`${prefix}-catalog`, [
    ...['晨雾外套', '深海针织', '岩层长裤', '月光手袋', '砂砾鞋履', '薄暮围巾'].map((name, index) => frame(`${prefix}-product-${index + 1}`, 'vertical', [
      media(`${prefix}-product-image-${index + 1}`, index % 2 ? [4, 5] : [3, 4]),
      text(`${prefix}-product-name-${index + 1}`, name, { fontSize: 18, fontWeight: 650 }),
      text(`${prefix}-product-price-${index + 1}`, `¥${(index + 2) * 380}`, { sizingX: 'hug', color: '#6e594b' })
    ], { rowGap: 12, role: 'product-tile' }))
  ], { columns: ['repeat(auto-fit, minmax(220px, 1fr))'], columnGap: 28, rowGap: 44, role: 'product-catalog' });
  const story = frame(`${prefix}-story`, 'horizontal', [media(`${prefix}-story-image`, [5, 4]), heading(`${prefix}-story-copy`, '材料档案', '天然纤维，慢速制作', '每件作品记录材料来源、工艺时间与长期护理方式。')], { columnGap: 56, align: 'center', role: 'editorial-brand-story' });
  return { children: [nav(prefix, 'Morrow Objects', ['新品', '系列', '材料']), media(`${prefix}-campaign`, [21, 9]), catalog, story], tablet: [{ nodeId: story.id, layout: { direction: 'vertical' } }] };
}

function editorial(): BenchmarkParts {
  const prefix = 'editorial-magazine';
  const lead = frame(`${prefix}-lead`, 'vertical', [media(`${prefix}-lead-image`, [16, 10]), text(`${prefix}-lead-title`, '城市如何在夜间重新学习呼吸', { fontSize: 40, fontWeight: 760, lineHeight: 1.08, role: 'lead-story-title' })], { rowGap: 20, role: 'lead-story' });
  lead.layout.gridPlacement = { columnStart: 1, rowStart: 1, columnSpan: 2, rowSpan: 2 };
  const cover = grid(`${prefix}-cover-grid`, [
    lead,
    card(`${prefix}-brief-1`, '观察', '一座旧车站的十二小时。'),
    card(`${prefix}-brief-2`, '人物', '修复师与正在消失的声音。'),
    card(`${prefix}-brief-3`, '档案', '从地图边缘重新认识河流。')
  ], { columns: ['1fr', '1fr', '1fr'], columnGap: 28, rowGap: 28, role: 'asymmetric-cover-grid' });
  const columns = grid(`${prefix}-columns`, [
    heading(`${prefix}-editor-note`, '卷首语', '慢下来，才能看见变化', '本期从城市、声音和材料三个尺度观察缓慢发生的转向。'),
    frame(`${prefix}-reading-list`, 'vertical', ['建筑', '文化', '生态', '影像'].map((item, index) => card(`${prefix}-article-${index + 1}`, item, `第 ${index + 1} 篇深度阅读与相关资料。`)), { rowGap: 18, role: 'reading-list' })
  ], { columns: ['minmax(280px, 0.8fr)', 'minmax(360px, 1.2fr)'], columnGap: 64, role: 'editorial-columns' });
  return {
    children: [nav(prefix, 'FIELD NOTES', ['当期', '栏目', '作者']), cover, columns],
    tablet: [{ nodeId: cover.id, layout: { grid: { columns: ['1fr', '1fr'], rows: [], autoFlow: 'row' } } }, { nodeId: lead.id, layout: { gridPlacement: { columnStart: 1, rowStart: 1, columnSpan: 2, rowSpan: 1 } } }, { nodeId: columns.id, layout: { grid: { columns: ['1fr'], rows: [], autoFlow: 'row' } } }],
    mobile: [{ nodeId: cover.id, layout: { grid: { columns: ['1fr'], rows: [], autoFlow: 'row' } } }, { nodeId: lead.id, layout: { gridPlacement: { columnStart: 1, rowStart: 1, columnSpan: 1, rowSpan: 1 } } }, { nodeId: columns.id, layout: { grid: { columns: ['1fr'], rows: [], autoFlow: 'row' } } }]
  };
}

function agency(): BenchmarkParts {
  const prefix = 'creative-agency';
  const projects = [
    ['North Sea / Identity', [16, 10], 7],
    ['Afterlight / Campaign', [4, 5], 5],
    ['Common Ground / Space', [5, 4], 5],
    ['Signal / Digital Product', [16, 9], 7]
  ] as const;
  const tiles = projects.map(([name, ratio, span], index) => {
    const tile = frame(`${prefix}-project-${index + 1}`, 'vertical', [media(`${prefix}-project-media-${index + 1}`, [...ratio]), text(`${prefix}-project-name-${index + 1}`, name, { fontSize: 24, fontWeight: 700 })], { rowGap: 16, role: 'project-tile' });
    tile.layout.gridPlacement = { columnSpan: span };
    return tile;
  });
  const mosaic = grid(`${prefix}-mosaic`, tiles, { columns: ['repeat(12, 1fr)'], columnGap: 24, rowGap: 48, autoFlow: 'dense', role: 'project-mosaic' });
  return {
    children: [nav(prefix, 'OTHER / OFFICE', ['作品', '方法', '联系']), text(`${prefix}-manifesto`, 'We shape useful friction.', { fontSize: 88, fontWeight: 780, lineHeight: 0.95, role: 'display-manifesto' }), mosaic, heading(`${prefix}-method`, '方法', '先找到值得被看见的差异', '策略、叙事、视觉与原型在同一个团队里连续推进。')],
    mobile: tiles.map((tile) => ({ nodeId: tile.id, layout: { gridPlacement: { columnStart: 1, columnSpan: 12, rowSpan: 1 } } }))
  };
}

function portfolio(): BenchmarkParts {
  const prefix = 'personal-portfolio';
  const intro = frame(`${prefix}-intro`, 'horizontal', [heading(`${prefix}-identity`, '产品设计师 · 上海', '把复杂系统变成清晰体验', '专注 AI 工具、创作软件与企业级协作产品。'), media(`${prefix}-portrait`, [4, 5])], { columnGap: 72, align: 'end', role: 'portfolio-intro' });
  const index = frame(`${prefix}-project-index`, 'vertical', ['AI 研究台', '城市交通服务', '创作者结算系统', '远程医疗门户'].map((name, itemIndex) => frame(`${prefix}-index-row-${itemIndex + 1}`, 'horizontal', [
    text(`${prefix}-index-number-${itemIndex + 1}`, `0${itemIndex + 1}`, { sizingX: 'hug', fontSize: 14 }),
    text(`${prefix}-index-title-${itemIndex + 1}`, name, { fontSize: 34, fontWeight: 700 }),
    text(`${prefix}-index-year-${itemIndex + 1}`, `${2026 - itemIndex}`, { sizingX: 'hug' })
  ], { columnGap: 24, align: 'baseline', role: 'project-index-row' })), { rowGap: 28, role: 'project-index' });
  const timeline = frame(`${prefix}-timeline`, 'vertical', ['2026 独立设计顾问', '2023 产品设计负责人', '2020 交互设计师'].map((item, index) => card(`${prefix}-timeline-${index + 1}`, item, '围绕复杂工作流建立可理解、可扩展的产品体验。')), { rowGap: 16, role: 'career-timeline' });
  return { children: [nav(prefix, 'Lin / Design', ['项目', '关于', '联系']), intro, index, timeline], tablet: [{ nodeId: intro.id, layout: { direction: 'vertical' } }], mobile: Array.from({ length: 4 }, (_, index) => ({ nodeId: `${prefix}-index-row-${index + 1}`, layout: { direction: 'vertical' } })) };
}

function conference(): BenchmarkParts {
  const prefix = 'conference-event';
  const hero = frame(`${prefix}-hero`, 'horizontal', [heading(`${prefix}-hero-copy`, '杭州 · 10.18—10.20', 'Build What Matters', '三天跨越设计、工程与组织的实践大会。'), frame(`${prefix}-countdown`, 'vertical', [text(`${prefix}-days`, '42', { fontSize: 72, fontWeight: 800, align: 'center' }), text(`${prefix}-days-label`, 'DAYS TO GO', { sizingX: 'hug', fontWeight: 700 })], { sizingX: 'hug', padding: 28, align: 'center', role: 'countdown', background: '#eef1ff' })], { columnGap: 56, align: 'center', justify: 'between', role: 'event-hero' });
  const speakers = grid(`${prefix}-speakers`, ['Maya Chen', 'Noah Williams', '林澈', 'Amina Diallo', 'Sora Kim', 'Jon Bell'].map((name, index) => frame(`${prefix}-speaker-${index + 1}`, 'vertical', [media(`${prefix}-speaker-photo-${index + 1}`, [1, 1]), text(`${prefix}-speaker-name-${index + 1}`, name, { fontWeight: 700 })], { rowGap: 12, role: 'speaker-card' })), { columns: ['repeat(auto-fit, minmax(160px, 1fr))'], columnGap: 20, rowGap: 28, role: 'speaker-grid' });
  const schedule = grid(`${prefix}-schedule`, [
    card(`${prefix}-schedule-1`, '09:30 · 主舞台', 'AI 原生产品的真实边界'),
    card(`${prefix}-schedule-2`, '11:00 · 设计轨', '从意图模型到可编辑界面'),
    card(`${prefix}-schedule-3`, '14:00 · 工程轨', '大规模协作中的确定性'),
    card(`${prefix}-schedule-4`, '16:30 · 圆桌', '工具如何改变组织形态')
  ], { columns: ['160px', 'repeat(2, 1fr)'], columnGap: 20, rowGap: 20, role: 'multi-track-schedule' });
  return { children: [nav(prefix, 'MATTER / 2026', ['议程', '嘉宾', '场地']), hero, speakers, schedule], tablet: [{ nodeId: hero.id, layout: { direction: 'vertical' } }, { nodeId: schedule.id, layout: { grid: { columns: ['1fr', '1fr'], rows: [], autoFlow: 'row' } } }], mobile: [{ nodeId: schedule.id, layout: { grid: { columns: ['1fr'], rows: [], autoFlow: 'row' } } }] };
}

function hospitality(): BenchmarkParts {
  const prefix = 'hospitality-restaurant';
  const menu = grid(`${prefix}-menu`, ['炭烤春笋', '海盐番茄', '烟熏鳟鱼', '山椒乳鸽', '青梅冰酪', '焙茶布丁'].map((name, index) => frame(`${prefix}-dish-${index + 1}`, 'horizontal', [text(`${prefix}-dish-name-${index + 1}`, name, { fontSize: 19, fontWeight: 650 }), text(`${prefix}-dish-price-${index + 1}`, `¥${68 + index * 16}`, { sizingX: 'hug' })], { justify: 'between', columnGap: 20, role: 'menu-item' })), { columns: ['1fr', '1fr'], columnGap: 56, rowGap: 28, role: 'restaurant-menu' });
  const gallery = grid(`${prefix}-gallery`, [media(`${prefix}-gallery-1`, [16, 10]), media(`${prefix}-gallery-2`, [4, 5]), media(`${prefix}-gallery-3`, [4, 5]), media(`${prefix}-gallery-4`, [16, 10])], { columns: ['repeat(2, 1fr)'], columnGap: 20, rowGap: 20, role: 'atmosphere-gallery' });
  return { children: [nav(prefix, '松间 / MATSUMA', ['菜单', '空间', '预订']), media(`${prefix}-hero`, [21, 9]), heading(`${prefix}-story`, '杭州 · 北山街', '一席山林，一餐四季', '菜单随产地与节气变化，空间在午后与夜晚呈现不同的光。'), menu, gallery], mobile: [{ nodeId: menu.id, layout: { grid: { columns: ['1fr'], rows: [], autoFlow: 'row' } } }, { nodeId: gallery.id, layout: { grid: { columns: ['1fr'], rows: [], autoFlow: 'row' } } }] };
}

function healthcare(): BenchmarkParts {
  const prefix = 'healthcare-service';
  const quick = frame(`${prefix}-quick-access`, 'horizontal', [button(`${prefix}-appointment`, '预约门诊'), button(`${prefix}-urgent`, '紧急帮助'), button(`${prefix}-results`, '查看报告')], { wrap: true, columnGap: 12, role: 'service-quick-access' });
  const services = grid(`${prefix}-services`, ['全科诊疗', '儿童健康', '女性健康', '慢病管理', '心理支持', '康复照护'].map((name, index) => card(`${prefix}-service-${index + 1}`, name, '查看适用症状、医生与可预约时间。')), { columns: ['repeat(auto-fit, minmax(230px, 1fr))'], columnGap: 20, rowGap: 20, role: 'service-grid' });
  const process = frame(`${prefix}-process`, 'horizontal', ['描述需求', '选择医生', '确认时间', '持续随访'].map((item, index) => card(`${prefix}-step-${index + 1}`, `${index + 1}. ${item}`, '每一步都提供明确反馈和人工帮助入口。')), { wrap: true, columnGap: 18, role: 'care-process' });
  const trust = frame(`${prefix}-trust`, 'horizontal', [heading(`${prefix}-trust-copy`, '持续照护', '不是一次问诊，而是一段被理解的过程', '跨学科团队共享经过授权的健康上下文，让每次沟通从已有认知继续。'), media(`${prefix}-doctor-team`, [4, 3])], { columnGap: 56, align: 'center', role: 'trust-story' });
  return { children: [nav(prefix, '安和健康', ['服务', '医生', '帮助']), heading(`${prefix}-hero`, '今日可预约', '清楚、可信、有人回应的医疗服务', '从症状导航到预约和随访，重要信息始终处在用户看得到的位置。'), quick, services, process, trust], tablet: [{ nodeId: trust.id, layout: { direction: 'vertical' } }], mobile: [{ nodeId: process.id, layout: { direction: 'vertical' } }] };
}

function education(): BenchmarkParts {
  const prefix = 'education-course';
  const hero = frame(`${prefix}-hero`, 'horizontal', [heading(`${prefix}-hero-copy`, '12 周线上工作坊', '用真实项目掌握 AI 产品设计', '从问题定义、原型、评估到上线，每周交付一个可以被验证的成果。'), media(`${prefix}-course-preview`, [16, 10])], { columnGap: 56, align: 'center', role: 'course-hero' });
  const curriculum = frame(`${prefix}-curriculum`, 'vertical', ['建立问题地图', '设计 AI 行为与边界', '构建可编辑原型', '运行真实用户评估'].map((item, index) => frame(`${prefix}-module-${index + 1}`, 'horizontal', [text(`${prefix}-week-${index + 1}`, `W${index * 3 + 1}—${index * 3 + 3}`, { sizingX: 'hug', fontWeight: 750 }), card(`${prefix}-module-card-${index + 1}`, item, '包含讲解、案例拆解、工作坊和一次针对性反馈。')], { columnGap: 32, align: 'start', role: 'curriculum-step' })), { rowGap: 20, role: 'curriculum-timeline' });
  const outcomes = grid(`${prefix}-outcomes`, [card(`${prefix}-outcome-1`, '作品', '完成一个有研究证据的完整项目。'), card(`${prefix}-outcome-2`, '方法', '建立可以重复使用的 AI 设计框架。'), card(`${prefix}-outcome-3`, '反馈', '获得导师和同伴的结构化评审。')], { columns: ['repeat(auto-fit, minmax(240px, 1fr))'], columnGap: 20, rowGap: 20, role: 'learning-outcomes' });
  return { children: [nav(prefix, 'Practice Lab', ['课程', '导师', '成果']), hero, curriculum, outcomes], tablet: [{ nodeId: hero.id, layout: { direction: 'vertical' } }], mobile: Array.from({ length: 4 }, (_, index) => ({ nodeId: `${prefix}-module-${index + 1}`, layout: { direction: 'vertical' } })) };
}

function nonprofit(): BenchmarkParts {
  const prefix = 'nonprofit-campaign';
  const story = frame(`${prefix}-story`, 'horizontal', [media(`${prefix}-story-photo`, [4, 3]), heading(`${prefix}-story-copy`, '河流守护计划', '让每个社区都能看见身边的水', '我们与本地志愿者共同采样、记录和公开水质变化。')], { columnGap: 56, align: 'center', role: 'human-story' });
  const metrics = frame(`${prefix}-metrics`, 'horizontal', [['128', '持续监测点'], ['46', '合作社区'], ['82%', '公开数据覆盖率']].map(([number, label], index) => frame(`${prefix}-metric-${index + 1}`, 'vertical', [text(`${prefix}-metric-value-${index + 1}`, number, { fontSize: 52, fontWeight: 780 }), text(`${prefix}-metric-label-${index + 1}`, label, { sizingX: 'hug' })], { sizingX: 'fill', rowGap: 4, role: 'impact-metric' })), { columnGap: 24, role: 'impact-metrics' });
  const actions = grid(`${prefix}-actions`, [card(`${prefix}-action-1`, '参与采样', '加入附近社区的月度监测。'), card(`${prefix}-action-2`, '支持设备', '资助一套开放水质检测工具。'), card(`${prefix}-action-3`, '使用数据', '下载数据并推动本地公共决策。')], { columns: ['repeat(auto-fit, minmax(240px, 1fr))'], columnGap: 20, rowGap: 20, role: 'action-grid' });
  return { children: [nav(prefix, 'CLEAR WATER', ['项目', '数据', '参与']), heading(`${prefix}-hero`, '开放环境数据', '一条河的变化，应该被所有人看见', '真实数据、人物故事与资金流向放在同一个公开界面里。'), story, metrics, actions], tablet: [{ nodeId: story.id, layout: { direction: 'vertical' } }], mobile: [{ nodeId: metrics.id, layout: { direction: 'vertical' } }] };
}

function property(): BenchmarkParts {
  const prefix = 'real-estate-property';
  const facts = frame(`${prefix}-facts`, 'horizontal', [['2028', '交付'], ['186', '户住宅'], ['3.2m', '标准层高'], ['48%', '景观覆盖']].map(([value, label], index) => frame(`${prefix}-fact-${index + 1}`, 'vertical', [text(`${prefix}-fact-value-${index + 1}`, value, { fontSize: 34, fontWeight: 720 }), text(`${prefix}-fact-label-${index + 1}`, label, { sizingX: 'hug' })], { sizingX: 'fill', rowGap: 4, role: 'property-fact' })), { wrap: true, columnGap: 32, role: 'property-facts' });
  const floorplans = grid(`${prefix}-floorplans`, [['A1', '125㎡ · 三室'], ['B2', '168㎡ · 四室'], ['P1', '238㎡ · 顶层']].map(([name, detail], index) => frame(`${prefix}-plan-${index + 1}`, 'vertical', [media(`${prefix}-plan-image-${index + 1}`, [4, 3]), text(`${prefix}-plan-name-${index + 1}`, name, { fontSize: 22, fontWeight: 720 }), text(`${prefix}-plan-detail-${index + 1}`, detail)], { rowGap: 12, role: 'floorplan-card' })), { columns: ['repeat(auto-fit, minmax(260px, 1fr))'], columnGap: 24, rowGap: 32, role: 'floorplan-browser' });
  const location = frame(`${prefix}-location`, 'horizontal', [heading(`${prefix}-location-copy`, '城市与山水之间', '十五分钟生活圈，步行抵达江岸', '教育、文化、商业与自然资源形成清晰的日常路径。'), media(`${prefix}-location-map`, [16, 10])], { columnGap: 56, align: 'center', role: 'location-story' });
  return { children: [nav(prefix, '栖江 / RIVER HOUSE', ['建筑', '户型', '位置']), media(`${prefix}-architecture`, [21, 9]), facts, floorplans, location], tablet: [{ nodeId: location.id, layout: { direction: 'vertical' } }], mobile: [{ nodeId: facts.id, layout: { direction: 'vertical' } }] };
}

function developer(): BenchmarkParts {
  const prefix = 'developer-platform';
  const sidebar = frame(`${prefix}-sidebar`, 'vertical', ['快速开始', '身份验证', '流式响应', '工具调用', '错误处理'].map((item, index) => text(`${prefix}-sidebar-item-${index + 1}`, item, { sizingX: 'hug', fontSize: 14, role: 'docs-navigation-item' })), { sizingX: 'fixed', width: 220, rowGap: 14, role: 'docs-sidebar' });
  const article = frame(`${prefix}-article`, 'vertical', [heading(`${prefix}-article-heading`, 'API / QUICKSTART', '五分钟发出第一次请求', '安装 SDK、配置密钥并创建一个支持流式输出的响应。'), card(`${prefix}-code-install`, '1. 安装', 'npm install @arc/sdk'), card(`${prefix}-code-request`, '2. 创建响应', 'client.responses.create({ model, input, stream: true })'), card(`${prefix}-code-handle`, '3. 处理事件', 'for await (const event of response) { … }')], { rowGap: 28, role: 'docs-article' });
  const toc = frame(`${prefix}-toc`, 'vertical', [text(`${prefix}-toc-title`, '本页内容', { sizingX: 'hug', fontWeight: 700 }), text(`${prefix}-toc-1`, '安装', { sizingX: 'hug' }), text(`${prefix}-toc-2`, '创建响应', { sizingX: 'hug' }), text(`${prefix}-toc-3`, '下一步', { sizingX: 'hug' })], { sizingX: 'fixed', width: 180, rowGap: 12, role: 'docs-table-of-contents' });
  const shell = frame(`${prefix}-docs-shell`, 'horizontal', [sidebar, article, toc], { columnGap: 48, align: 'start', role: 'three-column-docs-shell' });
  const examples = grid(`${prefix}-examples`, [card(`${prefix}-example-1`, '结构化输出', '使用 JSON Schema 约束可被应用消费的结果。'), card(`${prefix}-example-2`, '图像理解', '将视觉上下文与文本指令放在同一次请求。'), card(`${prefix}-example-3`, '后台任务', '追踪长任务状态并在完成时继续工作。')], { columns: ['repeat(auto-fit, minmax(240px, 1fr))'], columnGap: 20, rowGap: 20, role: 'developer-example-grid' });
  return { children: [nav(prefix, 'ARC / DEVELOPERS', ['文档', 'API', '社区']), shell, examples], tablet: [{ nodeId: toc.id, visible: false }, { nodeId: shell.id, layout: { gap: { row: 24, column: 28 } } }], mobile: [{ nodeId: sidebar.id, visible: false }, { nodeId: shell.id, layout: { direction: 'vertical' } }] };
}

const factories: Record<string, () => BenchmarkParts> = {
  'saas-product': saas,
  'consumer-commerce': commerce,
  'editorial-magazine': editorial,
  'creative-agency': agency,
  'personal-portfolio': portfolio,
  'conference-event': conference,
  'hospitality-restaurant': hospitality,
  'healthcare-service': healthcare,
  'education-course': education,
  'nonprofit-campaign': nonprofit,
  'real-estate-property': property,
  'developer-platform': developer
};

export const PHASE2_LAYOUT_VIEWPORT_WIDTHS = V2_BASELINE_VIEWPORTS.map((viewport) => viewport.width) as readonly number[];

export function createPhase2LayoutBenchmarks(): Phase2LayoutBenchmark[] {
  return V2_WEBSITE_BENCHMARKS.map((benchmark) => assemble(benchmark.id, patterns[benchmark.id], factories[benchmark.id]()));
}

export function validatePhase2LayoutBenchmarkCatalog(benchmarks = createPhase2LayoutBenchmarks()): string[] {
  const errors: string[] = [];
  if (benchmarks.length !== V2_WEBSITE_BENCHMARKS.length) errors.push(`Expected ${V2_WEBSITE_BENCHMARKS.length} benchmark scenes, received ${benchmarks.length}.`);
  const ids = new Set(benchmarks.map((benchmark) => benchmark.benchmarkId));
  for (const definition of V2_WEBSITE_BENCHMARKS) if (!ids.has(definition.id)) errors.push(`Missing benchmark scene: ${definition.id}.`);
  if (new Set(benchmarks.map((benchmark) => benchmark.pattern)).size !== benchmarks.length) errors.push('Every benchmark must use a distinct layout pattern.');
  if (new Set(benchmarks.map((benchmark) => benchmark.structuralSignature)).size !== benchmarks.length) errors.push('Benchmark scenes must not be structurally identical templates.');
  for (const benchmark of benchmarks) {
    try {
      assertSceneDocument(benchmark.document);
    } catch (error) {
      errors.push(`${benchmark.benchmarkId}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  return errors;
}
