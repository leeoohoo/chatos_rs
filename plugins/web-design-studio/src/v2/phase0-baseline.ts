export type WebsiteBenchmark = {
  id: string;
  name: string;
  category: string;
  objective: string;
  audience: string[];
  requiredPages: string[];
  requiredSections: string[];
  visualDirection: string[];
  responsiveStress: string[];
  forbiddenPatterns: string[];
};

export type BaselineViewport = {
  id: string;
  width: number;
  height: number;
  device: 'mobile' | 'tablet' | 'desktop' | 'wide';
};

export type QualityCriterion = {
  id: string;
  name: string;
  weight: number;
  description: string;
};

export const V2_BASELINE_VIEWPORTS: readonly BaselineViewport[] = [
  { id: 'mobile-compact', width: 320, height: 720, device: 'mobile' },
  { id: 'mobile-modern', width: 390, height: 844, device: 'mobile' },
  { id: 'tablet-portrait', width: 768, height: 1024, device: 'tablet' },
  { id: 'tablet-landscape', width: 1024, height: 768, device: 'tablet' },
  { id: 'desktop-small', width: 1280, height: 800, device: 'desktop' },
  { id: 'desktop-standard', width: 1440, height: 900, device: 'desktop' },
  { id: 'desktop-full-hd', width: 1920, height: 1080, device: 'desktop' },
  { id: 'desktop-qhd', width: 2560, height: 1440, device: 'wide' },
  { id: 'desktop-4k', width: 3840, height: 2160, device: 'wide' },
  { id: 'desktop-8k', width: 7680, height: 4320, device: 'wide' }
];

export const V2_QUALITY_RUBRIC: readonly QualityCriterion[] = [
  { id: 'hierarchy', name: '视觉层级', weight: 16, description: '首屏意图清楚，主次内容和行动路径可以快速识别。' },
  { id: 'layout', name: '布局与节奏', weight: 16, description: '对齐、留白、密度和分区节奏一致，没有偶然坐标造成的漂移。' },
  { id: 'responsive', name: '连续响应式', weight: 16, description: '所有基准宽度都能自然重排，不溢出、不遮挡，也不依赖三份独立设计。' },
  { id: 'typography', name: '排版', weight: 12, description: '字号、行高、行长和字体层级适合内容与设备。' },
  { id: 'brand', name: '品牌与原创性', weight: 12, description: '视觉语言符合 Brief，不是通用后台模板或组件示例的拼接。' },
  { id: 'content', name: '内容完整度', weight: 10, description: '真实信息结构完整，示例内容能够支撑布局判断。' },
  { id: 'interaction', name: '交互与状态', weight: 10, description: '导航、表单、弹层和状态反馈可操作且行为一致。' },
  { id: 'editability', name: '可编辑与可追踪', weight: 8, description: '节点语义清楚，人工修改受保护，AI 可以精确查询和修改。' }
];

export const V2_WEBSITE_BENCHMARKS: readonly WebsiteBenchmark[] = [
  {
    id: 'saas-product',
    name: 'AI SaaS 产品发布站',
    category: '产品与科技',
    objective: '解释一个复杂 AI 产品，并推动访客试用或预约演示。',
    audience: ['产品负责人', '技术决策者', '中小企业团队'],
    requiredPages: ['首页', '功能', '定价', '客户案例'],
    requiredSections: ['产品价值主张', '交互式产品展示', '能力矩阵', '客户证言', '价格方案', 'FAQ'],
    visualDirection: ['高端科技感', '清晰的信息层级', '克制的动态效果', '产品界面成为主视觉'],
    responsiveStress: ['宽屏产品截图缩放', '定价卡重排', '长标题换行', '导航折叠'],
    forbiddenPatterns: ['后台仪表盘冒充官网', '整页紫色渐变', '重复三张无差别功能卡']
  },
  {
    id: 'consumer-commerce',
    name: '消费品牌电商',
    category: '品牌与零售',
    objective: '建立鲜明品牌感并引导用户浏览和购买核心商品。',
    audience: ['移动端消费者', '品牌新客', '社交媒体访客'],
    requiredPages: ['首页', '商品集合', '商品详情', '品牌故事'],
    requiredSections: ['全幅商品叙事', '商品陈列', '使用场景', '评价', '品牌承诺', '订阅'],
    visualDirection: ['编辑式构图', '强摄影', '大胆排版', '留白与材质感'],
    responsiveStress: ['商品瀑布流', '横向故事模块', '图片裁切', '移动购买栏'],
    forbiddenPatterns: ['企业后台卡片', '所有图片同尺寸', '没有商品信息的装饰首屏']
  },
  {
    id: 'editorial-magazine',
    name: '数字杂志与专题',
    category: '媒体与内容',
    objective: '让读者快速理解当期主题，并深度阅读多种长度的内容。',
    audience: ['内容订阅读者', '移动阅读用户', '专题访客'],
    requiredPages: ['封面', '栏目列表', '长文', '作者页'],
    requiredSections: ['头条网格', '栏目导航', '编辑精选', '长文排版', '相关阅读', '订阅'],
    visualDirection: ['杂志版式', '强排版对比', '非对称网格', '阅读优先'],
    responsiveStress: ['多列转单列', '超长标题', '引用与边注', '图片题注'],
    forbiddenPatterns: ['等宽功能卡阵列', '过多按钮', '正文行长失控']
  },
  {
    id: 'creative-agency',
    name: '创意机构作品站',
    category: '创意与服务',
    objective: '通过案例叙事展示机构审美、方法和商业成果。',
    audience: ['品牌市场负责人', '创业公司创始人', '创意合作方'],
    requiredPages: ['首页', '作品索引', '案例详情', '机构介绍'],
    requiredSections: ['动态开场', '精选案例', '服务方法', '客户名单', '团队', '联系入口'],
    visualDirection: ['实验性但可用', '作品主导', '大字号', '有节制的动效'],
    responsiveStress: ['自由构图收敛', '视频与大图比例', '案例编号对齐', '触控替代悬停'],
    forbiddenPatterns: ['模板化 hero 加六卡片', '作品缩略图无差别', '动效阻碍阅读']
  },
  {
    id: 'personal-portfolio',
    name: '个人设计师作品集',
    category: '个人品牌',
    objective: '在短时间内表达个人定位、代表作品和合作方式。',
    audience: ['招聘经理', '潜在客户', '设计同行'],
    requiredPages: ['首页', '项目详情', '关于', '联系'],
    requiredSections: ['个人主张', '项目索引', '能力与经历', '项目过程', '成果', '联系'],
    visualDirection: ['个人化', '作品优先', '细腻排版', '轻量导航'],
    responsiveStress: ['项目卡比例变化', '履历时间线', '长案例内容', '图片组合'],
    forbiddenPatterns: ['企业定价模块', '虚假统计数据', '千篇一律头像加技能条']
  },
  {
    id: 'conference-event',
    name: '国际大会活动站',
    category: '活动与票务',
    objective: '传达活动气氛、议程与嘉宾阵容，并推动购票。',
    audience: ['参会者', '赞助商', '媒体'],
    requiredPages: ['活动首页', '议程', '嘉宾', '场地与出行'],
    requiredSections: ['日期倒计时', '购票行动', '嘉宾阵容', '多轨议程', '赞助商', '场地信息'],
    visualDirection: ['强识别活动视觉', '节奏鲜明', '信息密集但有序', '可分享'],
    responsiveStress: ['多轨议程横向信息', '嘉宾网格', '固定购票入口', '时区与长名称'],
    forbiddenPatterns: ['议程不可筛选', '移动端横向溢出', '嘉宾卡只有占位头像']
  },
  {
    id: 'hospitality-restaurant',
    name: '餐厅与酒店体验站',
    category: '餐饮与旅行',
    objective: '营造空间和服务体验，并促进预订。',
    audience: ['本地食客', '旅行者', '活动预订者'],
    requiredPages: ['首页', '菜单或房型', '空间故事', '预订'],
    requiredSections: ['沉浸式首屏', '菜单或房型', '主厨或品牌故事', '画廊', '地址营业时间', '预订'],
    visualDirection: ['电影感摄影', '温度与材质', '优雅排版', '低干扰交互'],
    responsiveStress: ['全幅媒体裁切', '菜单价格对齐', '日期人数表单', '地图信息'],
    forbiddenPatterns: ['SaaS 风格功能卡', '图片比例混乱', '预订入口不明确']
  },
  {
    id: 'healthcare-service',
    name: '医疗健康服务站',
    category: '健康与公共服务',
    objective: '建立可信感，帮助用户理解服务并快速找到预约和紧急信息。',
    audience: ['患者', '家属', '医疗转诊人员'],
    requiredPages: ['首页', '服务项目', '医生团队', '预约与帮助'],
    requiredSections: ['服务入口', '可信资质', '医生信息', '就诊流程', '常见问题', '预约'],
    visualDirection: ['平静可信', '无障碍优先', '真人与真实场景', '清晰行动路径'],
    responsiveStress: ['大字号模式', '表单错误状态', '多语言文本', '紧急信息常驻'],
    forbiddenPatterns: ['低对比度文字', '夸张科技动效', '关键服务藏在轮播中']
  },
  {
    id: 'education-course',
    name: '在线教育课程站',
    category: '教育与知识',
    objective: '解释课程成果、教学路径和师资，并推动试听或报名。',
    audience: ['学习者', '家长', '职业转型者'],
    requiredPages: ['首页', '课程目录', '课程详情', '导师'],
    requiredSections: ['学习成果', '课程路径', '试听内容', '导师', '学员成果', '报名方案'],
    visualDirection: ['友好而专业', '内容结构清楚', '学习过程可视化', '真实案例'],
    responsiveStress: ['课程目录层级', '进度与时间线', '视频比例', '价格比较'],
    forbiddenPatterns: ['只有营销没有课程内容', '表格移动端不可读', '虚假游戏化装饰']
  },
  {
    id: 'nonprofit-campaign',
    name: '公益组织行动站',
    category: '公益与倡议',
    objective: '用证据和人物故事建立共鸣，推动捐赠、报名或传播。',
    audience: ['捐赠者', '志愿者', '受议题影响的人群'],
    requiredPages: ['首页', '行动项目', '影响报告', '参与方式'],
    requiredSections: ['议题说明', '人物故事', '影响数据', '项目地图', '资金透明度', '行动入口'],
    visualDirection: ['真诚克制', '纪实影像', '数据与故事结合', '行动导向'],
    responsiveStress: ['数据图表', '长故事', '捐赠金额选择', '弱网图片回退'],
    forbiddenPatterns: ['商业 SaaS 语气', '用装饰代替证据', '捐赠流程不透明']
  },
  {
    id: 'real-estate-property',
    name: '高端地产项目站',
    category: '空间与地产',
    objective: '展示建筑、位置和生活方式，并收集高质量咨询线索。',
    audience: ['购房者', '投资者', '经纪人'],
    requiredPages: ['项目首页', '户型', '设施', '位置与预约'],
    requiredSections: ['建筑主视觉', '项目概览', '户型浏览', '设施画廊', '区位地图', '预约看房'],
    visualDirection: ['建筑杂志感', '奢华但克制', '大图叙事', '精细数据'],
    responsiveStress: ['户型图缩放', '地图与地标', '横向画廊', '复杂筛选器'],
    forbiddenPatterns: ['普通房源列表模板', '图片拉伸', '移动端筛选不可关闭']
  },
  {
    id: 'developer-platform',
    name: '开发者平台与文档站',
    category: '开发者体验',
    objective: '让开发者理解平台价值、快速开始，并持续查阅技术内容。',
    audience: ['软件工程师', '技术负责人', '开源贡献者'],
    requiredPages: ['产品首页', '快速开始', 'API 文档', '社区'],
    requiredSections: ['代码示例', '能力概览', '安装步骤', '文档导航', 'API 内容', '社区入口'],
    visualDirection: ['技术可信', '代码优先', '高信息密度', '明暗模式一致'],
    responsiveStress: ['三栏文档布局', '代码横向滚动', '深层导航', '搜索与命令面板'],
    forbiddenPatterns: ['代码作为不可复制图片', '移动端保留三栏', '组件示例与正文样式不一致']
  }
];

export const V2_PRESERVED_CAPABILITIES = [
  'ChatOS projectId participates in the isolated runtime scope',
  'projects own design membership without mutating design layout data',
  'revision conflicts fail explicitly instead of overwriting newer work',
  'document persistence uses atomic writes and lock protection',
  'MCP tools inherit the host scope instead of accepting a caller-supplied projectId',
  'third-party components continue to execute in their real library runtimes'
] as const;

export function validatePhase0Baseline(): string[] {
  const errors: string[] = [];
  const ids = new Set<string>();
  for (const benchmark of V2_WEBSITE_BENCHMARKS) {
    if (ids.has(benchmark.id)) errors.push(`Duplicate benchmark id: ${benchmark.id}`);
    ids.add(benchmark.id);
    if (benchmark.requiredPages.length < 4) errors.push(`${benchmark.id} needs at least four pages.`);
    if (benchmark.requiredSections.length < 6) errors.push(`${benchmark.id} needs at least six sections.`);
    if (benchmark.visualDirection.length < 3) errors.push(`${benchmark.id} needs a concrete visual direction.`);
    if (benchmark.responsiveStress.length < 3) errors.push(`${benchmark.id} needs responsive stress cases.`);
    if (benchmark.forbiddenPatterns.length < 3) errors.push(`${benchmark.id} needs forbidden patterns.`);
  }
  if (V2_WEBSITE_BENCHMARKS.length !== 12) errors.push('The phase 0 catalog must contain exactly 12 website benchmarks.');
  if (new Set(V2_BASELINE_VIEWPORTS.map((viewport) => viewport.width)).size !== V2_BASELINE_VIEWPORTS.length) {
    errors.push('Baseline viewport widths must be unique.');
  }
  const rubricWeight = V2_QUALITY_RUBRIC.reduce((sum, criterion) => sum + criterion.weight, 0);
  if (rubricWeight !== 100) errors.push(`Quality rubric weights must total 100, received ${rubricWeight}.`);
  return errors;
}
