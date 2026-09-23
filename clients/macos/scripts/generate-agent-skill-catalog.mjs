#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const directory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../Sources/ChatOSCore/Resources",
);
const sourcePath = path.join(directory, "ChatOSSkillCatalog.json");
const outputPath = path.join(directory, "ChatOSSkillCatalog.json");
const catalog = JSON.parse(fs.readFileSync(sourcePath, "utf8"));

const split = (value) => value.split("|").map((item) => item.trim()).filter(Boolean);
const bullets = (items) => items.map((item) => `- ${item}`).join("\n");
const numbered = (items) => items.map((item, index) => `${index + 1}. ${item}`).join("\n");
const bilingual = (zh, en) => ({ zh: split(zh), en: split(en) });

const professionFamilies = {
  management: bilingual(
    "把模糊目标转成可决策的范围、责任、优先级和验收|让计划、风险、团队状态和真实交付证据保持一致",
    "Turn ambiguous goals into decision-ready scope, ownership, priorities, and acceptance|Keep plans, risks, team state, and actual delivery evidence aligned",
  ),
  architecture_quality: bilingual(
    "先识别高影响风险和系统边界，再选择与风险相称的验证深度|结论必须能追溯到模型、测试、审阅或可复现证据",
    "Identify high-impact risks and system boundaries before choosing proportionate validation|Make conclusions traceable to models, tests, reviews, or reproducible evidence",
  ),
  engineering: bilingual(
    "优先交付可运行、可测试、可审阅的小增量|设计正常、边界、失败和恢复路径，并保留兼容与回退能力",
    "Prefer small increments that run, test, and review cleanly|Design normal, boundary, failure, and recovery paths while preserving compatibility and rollback",
  ),
  data_ai: bilingual(
    "明确数据、指标、样本或证据的口径、来源和适用边界|量化不确定性、偏差和失效条件，不把相关性包装成因果",
    "Define the meaning, source, and applicability of data, metrics, samples, or evidence|Quantify uncertainty, bias, and failure conditions instead of presenting correlation as causation",
  ),
  design_content: bilingual(
    "从受众、任务、媒介和约束出发，而不是从个人偏好出发|用可审阅源文件、版本和真实内容推动评审与交接",
    "Start from audience, task, medium, and constraints rather than personal taste|Use reviewable source artifacts, versions, and realistic content for review and handoff",
  ),
  business_delivery: bilingual(
    "把业务术语、规则、例外和控制点转成可验证流程与验收|同时核对系统状态、业务台账和真实操作结果",
    "Turn business terms, rules, exceptions, and controls into verifiable processes and acceptance|Reconcile system state, business records, and real operational outcomes",
  ),
  general: bilingual(
    "先确认目标、边界、输入、输出和完成标准|选择与风险相称的方法，并留下下一位成员可复用的证据",
    "Confirm the objective, boundaries, inputs, outputs, and definition of done|Choose a method proportionate to risk and leave reusable evidence for the next contributor",
  ),
};

const professionArtifacts = {
  project_manager: bilingual("团队 Todo 图、里程碑和依赖关系|风险/问题/决策记录、进度摘要与交接证据", "Team Todo graph, milestones, and dependencies|Risk/issue/decision log, progress summary, and handoff evidence"),
  product_manager: bilingual("产品简报、用户场景、范围与非目标|优先级、验收标准、假设与效果衡量方案", "Product brief, user scenarios, scope, and non-goals|Priorities, acceptance criteria, assumptions, and outcome measurement"),
  technical_manager: bilingual("技术路线、模块责任和集成计划|工程质量基线、评审结论与验证结果", "Technical direction, module ownership, and integration plan|Engineering quality baseline, review decisions, and verification results"),
  solution_architect: bilingual("系统边界、接口/数据契约和 ADR|非功能预算、迁移/回退方案和架构验证", "System boundaries, interface/data contracts, and ADRs|Non-functional budgets, migration/rollback plan, and architecture validation"),
  security_engineer: bilingual("威胁模型、风险分级和控制建议|最小复现、修复验证与剩余风险", "Threat model, risk ratings, and control recommendations|Minimal reproduction, remediation verification, and residual risk"),
  qa_engineer: bilingual("风险测试矩阵、自动化/手工结果|缺陷证据、复验记录和发布质量结论", "Risk-based test matrix and automated/manual results|Defect evidence, retest record, and release quality assessment"),
  software_engineer: bilingual("可审阅代码、必要测试和实现说明|构建/验证结果、兼容性与回退说明", "Reviewable code, necessary tests, and implementation notes|Build/verification results, compatibility, and rollback notes"),
  fullstack_engineer: bilingual("端到端纵向功能切片和接口契约|迁移、端到端测试、部署与排障说明", "End-to-end vertical slices and interface contracts|Migrations, end-to-end tests, deployment, and troubleshooting guidance"),
  frontend_engineer: bilingual("页面/组件、完整界面状态和真实接口集成|视觉、可访问性、性能与兼容证据", "Pages/components, complete UI states, and real integrations|Visual, accessibility, performance, and compatibility evidence"),
  backend_engineer: bilingual("API/数据契约、服务实现与迁移|事务、权限、负载和恢复验证", "API/data contracts, service implementation, and migrations|Transaction, authorization, load, and recovery verification"),
  mobile_engineer: bilingual("可安装构建、关键旅程和设备状态处理|真机、升级、崩溃、性能与发布证据", "Installable builds, critical journeys, and device-state handling|Device, upgrade, crash, performance, and release evidence"),
  desktop_engineer: bilingual("可安装应用、平台交互和本地系统集成|签名安装、更新、恢复和多窗口验证", "Installable app, platform interaction, and local system integration|Signed installation, update, recovery, and multi-window validation"),
  game_engineer: bilingual("可玩构建、核心系统和资源/工具链|性能、存档、确定性与平台验证", "Playable build, core systems, and asset/tool pipeline|Performance, save, determinism, and platform verification"),
  embedded_iot_engineer: bilingual("固件/边缘代码、协议和硬件假设|台架/HIL、OTA/回退、功耗与长稳证据", "Firmware/edge code, protocols, and hardware assumptions|Bench/HIL, OTA/rollback, power, and soak evidence"),
  database_engineer: bilingual("数据模型、DDL/迁移、索引和查询验证|容量、锁、复制、备份恢复与回退记录", "Data model, DDL/migrations, indexes, and query verification|Capacity, locking, replication, backup/restore, and rollback record"),
  devops_engineer: bilingual("CI/CD、基础设施代码和环境契约|监控告警、部署/回滚、容量和恢复演练", "CI/CD, infrastructure code, and environment contract|Monitoring/alerts, deployment/rollback, capacity, and recovery rehearsal"),
  data_engineer: bilingual("数据契约、模型/管道、质量规则和血缘|回填、对账、性能和恢复证据", "Data contracts, models/pipelines, quality rules, and lineage|Backfill, reconciliation, performance, and recovery evidence"),
  data_analyst: bilingual("指标字典、可复现查询/分析和图表|数据质量、敏感性、限制与决策建议", "Metric dictionary, reproducible queries/analysis, and charts|Data quality, sensitivity, limitations, and decision recommendations"),
  machine_learning_engineer: bilingual("数据/特征契约、实验、模型卡和推理实现|基线、偏差、漂移、负载与回滚证据", "Data/feature contracts, experiments, model card, and inference implementation|Baseline, bias, drift, load, and rollback evidence"),
  research_specialist: bilingual("研究问题、来源清单和证据矩阵|引用、交叉核验、置信度、限制与建议", "Research question, source list, and evidence matrix|Citations, triangulation, confidence, limitations, and recommendations"),
  product_designer: bilingual("用户流、可编辑设计源文件、原型和组件规范|可用性结果、版本与开发验收记录", "User flows, editable design sources, prototypes, and component specs|Usability results, versions, and implementation acceptance record"),
  ui_designer: bilingual("视觉源文件、Token、组件变体和状态规范|对比度、可访问性、资源导出与视觉验收", "Editable visual source, tokens, component variants, and state specs|Contrast, accessibility, asset export, and visual acceptance"),
  ux_designer: bilingual("研究发现、旅程/任务流、信息架构和原型|可用性证据、问题优先级与无障碍建议", "Research findings, journeys/task flows, information architecture, and prototypes|Usability evidence, issue priorities, and accessibility recommendations"),
  game_designer: bilingual("玩法规则、状态/数值表、关卡或内容规范|试玩记录、平衡指标和迭代结论", "Gameplay rules, state/balance tables, and level/content specs|Playtest records, balance metrics, and iteration decisions"),
  technical_writer: bilingual("信息架构、教程/指南/参考/运行手册|可执行示例、链接、版本适用性和审阅记录", "Information architecture, tutorials/how-to/reference/runbooks|Executable examples, links, version applicability, and review record"),
  business_analyst: bilingual("现状/目标流程、业务术语和规则|需求、验收场景、差距、决策与追踪矩阵", "Current/target processes, business terms, and rules|Requirements, acceptance scenarios, gaps, decisions, and traceability"),
  implementation_consultant: bilingual("差距/配置工作簿、迁移/UAT/培训计划|对账、切换、回退、稳定期与交接记录", "Gap/configuration workbook and migration/UAT/training plan|Reconciliation, cutover, rollback, stabilization, and handover record"),
  erp_consultant: bilingual("业务蓝图、配置/主数据和单据/会计规则|期初迁移、跨模块对账、UAT 与结账验证", "Business blueprint, configuration/master data, and document/posting rules|Opening migration, cross-module reconciliation, UAT, and close validation"),
  wms_consultant: bilingual("仓储蓝图、策略/主数据和设备接口|库存对账、场景测试、盘点、峰值与切换验证", "Warehouse blueprint, strategy/master data, and device interfaces|Inventory reconciliation, scenario tests, counting, peak, and cutover validation"),
  domain_expert: bilingual("领域词汇、规则/例外矩阵和风险控制|权威来源、反例、验收场景与专业确认", "Domain glossary, rule/exception matrix, and risk controls|Authoritative sources, counterexamples, acceptance scenarios, and professional confirmation"),
  operations_specialist: bilingual("运营日历/SOP、指标阈值和异常台账|服务水平、数据对账、实验与改进结果", "Operations calendar/SOPs, metric thresholds, and exception log|Service-level, reconciliation, experiment, and improvement results"),
  growth_marketing_specialist: bilingual("受众/信息框架、活动/内容和实验计划|素材来源、审批、漏斗结果与归因限制", "Audience/message framework and campaign/content/experiment plan|Asset provenance, approvals, funnel results, and attribution limits"),
  general_member: bilingual("与任务相符的实际产物和验证结果|可复现步骤、未完成事项与下一步建议", "Task-aligned artifacts and verification results|Reproduction steps, incomplete items, and recommended next actions"),
};

const capabilityByProfession = {
  management: bilingual("通讯 Run 优先使用未读消息、团队群和团队 Todo；创建执行 Todo 时按任务选择 project_read、project_write、terminal，并从当次可选目录挑选真正需要的已安装插件", "In communication runs, prioritize unread messages, the team group, and team Todos; when creating execution Todos, select project_read, project_write, terminal, and only the installed plugins actually required by the task"),
  architecture_quality: bilingual("执行任务通常先用 project_read 检查项目与证据；需要产生修复或验证脚本时再选择 project_write/terminal，并通过能力发现查找安全、测试、浏览器或分析类插件", "Execution usually starts with project_read to inspect the project and evidence; add project_write/terminal only for fixes or verification scripts, and use capability discovery for security, testing, browser, or analysis plugins"),
  engineering: bilingual("代码任务按需使用 project_read、project_write 和 terminal；涉及界面、浏览器、图表、媒体或外部格式时，通过能力发现选择本机已安装插件，不预设插件 ID", "For code work, use project_read, project_write, and terminal as needed; for UI, browser, diagrams, media, or external formats, discover an installed plugin at runtime instead of hard-coding plugin IDs"),
  data_ai: bilingual("先使用 project_read 读取数据契约与现有资产；需要生成分析、脚本或模型产物时使用 project_write/terminal，并按输出格式发现表格、图表、文档或浏览器插件", "Use project_read first for data contracts and existing assets; use project_write/terminal for analysis, scripts, or model artifacts, and discover spreadsheet, visualization, document, or browser plugins according to the required output"),
  design_content: bilingual("使用 project_read 获取需求与品牌资产；产物需要落入项目时使用 project_write，图像、视频、文档、演示、网页或设计审阅通过能力发现调用对应已安装插件", "Use project_read for requirements and brand assets; use project_write when artifacts belong in the project, and discover the installed image, video, document, presentation, web, or design-review plugin required by the deliverable"),
  business_delivery: bilingual("使用 project_read 核对规则、数据和现有材料；涉及批量核对或正式交付物时，通过能力发现选择表格、文档、演示、浏览器或行业插件，必要脚本才使用 terminal", "Use project_read to inspect rules, data, and existing material; for reconciliation or formal deliverables, discover spreadsheet, document, presentation, browser, or domain plugins, and use terminal only for necessary scripts"),
  general: bilingual("先使用 capability_search 搜索当前任务需要的能力；只启用完成任务所需的最小文件、终端和插件集合", "Start with capability_search for the current task and activate only the minimal file, terminal, and plugin capabilities needed to complete it"),
};

function professionMarkdown(item, language) {
  const lang = language === "en" ? "en" : "zh";
  const title = lang === "en" ? item.label_en : item.label;
  const description = lang === "en" ? item.description_en : item.description;
  const family = professionFamilies[item.category_key] ?? professionFamilies.general;
  const artifacts = professionArtifacts[item.key];
  if (!artifacts) throw new Error(`Missing profession artifacts: ${item.key}`);
  const capability = capabilityByProfession[item.category_key] ?? capabilityByProfession.general;
  const taskBoundary = item.can_create_tasks
    ? (lang === "en"
      ? "Use Project Manager authority to maintain the team Todo graph and coordination state; do not take over specialist execution merely because you can schedule it."
      : "使用项目经理权限维护团队 Todo 图和协作状态；不要因为拥有排产权就替专业成员执行其工作。")
    : (lang === "en"
      ? "Do not create, reassign, reorder, or cancel team Todos. Send task-ready findings and evidence to the bound Project Manager when new work is needed."
      : "不得创建、改派、改序或取消团队 Todo；需要新增工作时，把可直接建任务的问题与证据交给团队绑定的项目经理。");
  return `---
name: chatos-profession-${item.key.replaceAll("_", "-")}
description: ${description}
---

# ${title}

## ${lang === "en" ? "Role focus" : "岗位重点"}

${bullets([description, ...family[lang]])}

## ${lang === "en" ? "Expected artifacts and evidence" : "预期产物与证据"}

${bullets(artifacts[lang])}

## ${lang === "en" ? "Tools and plugins" : "工具与插件"}

- ${capability[lang].join(" ")}
- ${lang === "en"
    ? "A communication run coordinates and records work; actual project mutation belongs in a Todo execution run with the capabilities selected for that Todo."
    : "通讯 Run 负责协调和记录；真正修改项目的工作应进入 Todo 执行 Run，并且只获得该 Todo 已选择的能力。"}
- ${lang === "en"
    ? "Inside an execution run, use capability_search with a task-oriented query, capability_describe for the best matching installed option, and capability_invoke for its exposed tool. Never assume a plugin is installed or invent its ID."
    : "进入执行 Run 后，用 capability_search 按任务搜索能力，只对最匹配的已安装选项调用 capability_describe，再通过 capability_invoke 使用其工具；不得假设插件已安装或编造插件 ID。"}

## ${lang === "en" ? "Collaboration boundary" : "协作边界"}

- ${taskBoundary}
- ${lang === "en"
    ? "Treat the current message or assigned Todo, its acceptance criteria, accessible team assets, and current-run tools as authoritative scope. Put material progress, blockers, and results where the affected team can see them."
    : "以当前消息或已分配 Todo、验收标准、可访问的团队共享资产和本轮工具为权威范围；重要进度、阻塞和结果应回到受影响团队可见的位置。"}
`;
}

const projectFamilies = {
  software_product: bilingual("先稳定用户/调用方需求、接口和状态模型，再实现纵向可运行增量|按真实风险决定设计、测试、迁移、部署和回退深度", "Stabilize user/caller needs, interfaces, and state models before implementing runnable vertical slices|Choose design, testing, migration, deployment, and rollback depth from actual risk"),
  enterprise_system: bilingual("先固化业务流程、主数据、权限、单据/状态与对账不变量|配置、扩展、集成、迁移、UAT 和切换必须形成闭环", "Establish business process, master data, permissions, document/state, and reconciliation invariants first|Close the loop across configuration, extension, integration, migration, UAT, and cutover"),
  data_research: bilingual("先明确问题、数据/证据口径、来源、适用范围和成功标准|让分析、研究或模型结论可复现，并如实表达不确定性", "Define the question, data/evidence semantics, sources, applicability, and success criteria first|Keep analysis, research, or model conclusions reproducible and uncertainty explicit"),
  design_content: bilingual("先确定受众、场景、媒介、语气、结构和评审标准|保留可编辑源文件、版本、素材来源和发布检查", "Define audience, context, medium, tone, structure, and review criteria first|Preserve editable sources, versions, asset provenance, and release checks"),
  operations_delivery: bilingual("先绘制现状、触发、责任、异常和恢复路径|通过演练、对账和观测证明持续运行与交接能力", "Map current state, triggers, ownership, exceptions, and recovery paths first|Prove sustainable operation and handoff through rehearsal, reconciliation, and observation"),
  general: bilingual("明确目标、范围、非目标、负责人、约束和可验证完成标准|按交付形态选择必要阶段，不机械套用软件流程", "Define objective, scope, non-goals, ownership, constraints, and verifiable completion criteria|Choose necessary stages from the delivery shape rather than mechanically applying a software lifecycle"),
};

const projectDelivery = {
  software_development: bilingual("需求与验收|技术方案和契约|可运行增量与必要迁移|系统验证、交付和维护", "Requirements and acceptance|Technical plan and contracts|Runnable increments and necessary migrations|System verification, delivery, and maintenance"),
  web_application: bilingual("用户旅程、权限和信息架构|关键页面/状态设计|前后端契约与纵向切片|浏览器验证、部署和交接", "User journeys, permissions, and information architecture|Critical page/state design|Front/back-end contracts and vertical slices|Browser verification, deployment, and handoff"),
  mobile_application: bilingual("移动场景、设备范围和导航设计|权限、本地状态、网络与生命周期|关键旅程实现|真机、发布和升级验证", "Mobile contexts, device range, and navigation design|Permissions, local state, networking, and lifecycle|Critical journey implementation|Device, release, and upgrade validation"),
  desktop_application: bilingual("桌面工作流和平台交互设计|本地数据、权限、沙箱与系统集成|功能实现|安装、更新、恢复与平台验收", "Desktop workflows and platform interaction design|Local data, permissions, sandbox, and system integration|Implementation|Install, update, recovery, and platform acceptance"),
  backend_service: bilingual("调用方与服务边界|API、权限、数据和失败契约|服务与迁移实现|负载、安全、恢复与运维验证", "Caller and service boundaries|API, authorization, data, and failure contracts|Service and migration implementation|Load, security, recovery, and operational verification"),
  library_sdk: bilingual("使用场景、支持矩阵和公共 API|错误、并发、版本与兼容策略|实现、示例和契约测试|打包发布与升级验证", "Use cases, support matrix, and public API|Error, concurrency, versioning, and compatibility|Implementation, examples, and contract tests|Packaging, release, and upgrade validation"),
  game_development: bilingual("玩家目标、核心循环和平台范围|规则、状态、数值、内容与表现设计|可玩纵向切片|性能、内容扩展与平台发布", "Player goals, core loop, and platform scope|Rule, state, balance, content, and presentation design|Playable vertical slice|Performance, content expansion, and platform release"),
  iot_embedded_system: bilingual("硬件/现场约束和安全状态|协议、设备身份与边云边界|固件/边缘实现|台架/HIL、OTA 和现场恢复", "Hardware/field constraints and safe state|Protocol, device identity, and edge/cloud boundaries|Firmware/edge implementation|Bench/HIL, OTA, and field recovery"),
  enterprise_erp: bilingual("组织与业务蓝图|主数据、权限、单据流与会计控制|配置/扩展/集成和迁移|UAT、期初、切换与结账", "Organization and business blueprint|Master data, permissions, document flows, and accounting controls|Configuration/extensions/integrations and migration|UAT, opening balances, cutover, and close"),
  warehouse_management_system: bilingual("仓网、货品、容器和库存不变量|作业/异常流程与设备接口|配置、开发和集成|库存迁移、峰值演练与上线", "Network, item, container, and inventory invariants|Operational/exception flows and device interfaces|Configuration, development, and integration|Inventory migration, peak rehearsal, and go-live"),
  customer_relationship_management: bilingual("客户生命周期、角色和数据治理|线索到服务的流程与规则|配置、集成和迁移|采用、权限与业务验收", "Customer lifecycle, roles, and data governance|Lead-to-service processes and rules|Configuration, integration, and migration|Adoption, permissions, and business acceptance"),
  manufacturing_execution_system: bilingual("工厂模型、BOM/工艺和生产不变量|工单、在制品、质量、追溯和设备流程|ERP/设备集成与迁移|线边演练、对账与切换", "Plant model, BOM/routing, and production invariants|Order, WIP, quality, traceability, and equipment flows|ERP/equipment integration and migration|Line-side rehearsal, reconciliation, and cutover"),
  ecommerce_platform: bilingual("交易角色和商品/价格/库存/订单不变量|购物、支付、履约、售后与对账|纵向交易切片和外部集成|峰值、安全与恢复验证", "Commerce roles and product/price/inventory/order invariants|Shopping, payment, fulfillment, after-sales, and reconciliation|Vertical transaction slices and external integrations|Peak, security, and recovery validation"),
  data_analysis: bilingual("业务问题和指标定义|数据获取、质量与分析设计|可复现分析和可视化|结论、限制与建议", "Business question and metric definition|Data acquisition, quality, and analysis design|Reproducible analysis and visualization|Conclusions, limitations, and recommendations"),
  data_engineering_platform: bilingual("数据源和消费方契约|模型、血缘、质量与调度|管道、回填和迁移|容量、恢复与服务化验证", "Producer and consumer contracts|Models, lineage, quality, and orchestration|Pipelines, backfills, and migrations|Capacity, recovery, and serving verification"),
  machine_learning_system: bilingual("问题、基线、数据/标签/切分|特征、训练、评估与风险|推理和产品集成|监控、反馈、回滚与生命周期", "Problem, baseline, data/labels/splits|Features, training, evaluation, and risk|Inference and product integration|Monitoring, feedback, rollback, and lifecycle"),
  research: bilingual("研究问题、范围与决策用途|来源和检索策略|证据提取、交叉核验与综合|结论、置信度和后续验证", "Research question, scope, and decision use|Source and search strategy|Evidence extraction, triangulation, and synthesis|Conclusions, confidence, and follow-up validation"),
  product_design: bilingual("用户问题、研究和体验目标|信息架构、任务流与低保真探索|视觉/交互系统和原型|可用性验证与开发验收", "User problem, research, and experience goals|Information architecture, task flows, and low-fidelity exploration|Visual/interaction system and prototype|Usability validation and implementation acceptance"),
  design_system_brand: bilingual("品牌/产品原则与审计|Token、组件和视觉/内容语言|文档、工具与迁移|采用、治理和跨端一致性", "Brand/product principles and audit|Tokens, components, and visual/content language|Documentation, tooling, and migration|Adoption, governance, and cross-platform consistency"),
  novel_writing: bilingual("受众、体裁、主题与叙事承诺|世界、人物、冲突、结构和视角|分章创作与连续性维护|结构、事实、语言和定稿编辑", "Audience, genre, theme, and narrative promise|World, character, conflict, structure, and point of view|Drafting with continuity control|Structural, factual, line, and final editing"),
  general_writing: bilingual("受众、目的、渠道、语气与事实边界|提纲和证据/素材组织|起草与结构迭代|事实、语言、格式和发布检查", "Audience, purpose, channel, tone, and factual boundary|Outline and evidence/source organization|Drafting and structural iteration|Fact, language, format, and release checks"),
  documentation: bilingual("读者/任务清单和内容审计|信息架构与文档类型规划|编写、示例和交叉链接|技术审阅、发布与持续维护", "Audience/task inventory and content audit|Information architecture and document-type plan|Writing, examples, and cross-linking|Technical review, release, and maintenance"),
  marketing_content: bilingual("受众、价值主张、渠道和品牌边界|活动/内容矩阵与素材计划|创作、审批和发布|追踪、归因与复盘", "Audience, value proposition, channels, and brand boundary|Campaign/content matrix and asset plan|Creation, approval, and publication|Tracking, attribution, and retrospective"),
  automation: bilingual("现状流程、触发和人工决策点|输入输出、权限、幂等与失败模型|小范围自动化实现|观测、人工接管与安全推广", "Current process, triggers, and human decision points|Inputs/outputs, permissions, idempotency, and failure model|Small-scope automation implementation|Observability, human takeover, and safe rollout"),
  operations: bilingual("服务对象、运营目标和现状基线|节奏、SOP、数据与异常机制|执行、监控和持续改进|交接与韧性验证", "Service audience, operational goals, and current baseline|Cadence, SOPs, data, and exception handling|Execution, monitoring, and continuous improvement|Handoff and resilience validation"),
  implementation_migration: bilingual("现状、目标与差距评估|配置/扩展、集成、数据和测试计划|迁移演练、UAT 与培训|切换、稳定期和正式交接", "Current state, target state, and gap assessment|Configuration/extensions, integrations, data, and test plan|Migration rehearsal, UAT, and training|Cutover, stabilization, and formal handoff"),
  general: bilingual("目标、范围和成功标准|必要方案、计划与责任分工|分阶段执行和验证|验收、总结与维护交接", "Objective, scope, and success criteria|Necessary approach, plan, and ownership|Incremental execution and validation|Acceptance, summary, and maintenance handoff"),
};

const projectCapabilityHints = {
  software_product: bilingual("代码类 Todo 通常按需选择 project_read、project_write、terminal；界面或浏览器验证任务再从已安装目录选择设计、浏览器、截图或测试插件", "Code Todos usually select project_read, project_write, and terminal as needed; UI or browser-validation Todos additionally choose installed design, browser, screenshot, or testing plugins"),
  enterprise_system: bilingual("流程/配置任务优先使用 project_read；实现与迁移任务按需加入 project_write/terminal；对账、文档、演示、浏览器或设备工作从已安装插件中分别选择", "Process/configuration Todos start with project_read; implementation and migration add project_write/terminal as needed; reconciliation, documents, presentations, browser, or device work select the matching installed plugin"),
  data_research: bilingual("数据与脚本任务按需选择 project_read、project_write、terminal；表格、图表、文档、浏览器检索或模型工作通过能力发现选择对应已安装插件", "Data and script Todos select project_read, project_write, and terminal as needed; spreadsheet, visualization, document, browser-research, or model work discovers the matching installed plugin"),
  design_content: bilingual("素材与项目上下文使用 project_read，需保存到项目时加入 project_write；图像、视频、文档、演示、网页或设计审阅从已安装插件中按交付格式选择", "Use project_read for assets and project context and add project_write when saving into the project; choose installed image, video, document, presentation, web, or design-review plugins according to the deliverable"),
  operations_delivery: bilingual("核对现状先使用 project_read；脚本或配置变更按需加入 project_write/terminal；表格、文档、浏览器、远程系统或行业操作通过能力发现选择实际可用插件", "Start current-state inspection with project_read; add project_write/terminal for scripts or configuration changes; discover available spreadsheet, document, browser, remote-system, or domain plugins for operational work"),
  general: bilingual("项目经理创建 Todo 前先读取当次能力选项，只选择该任务真正需要的 project_read、project_write、terminal 和已安装插件", "Before creating a Todo, the Project Manager reads the current capability options and selects only the project_read, project_write, terminal, and installed plugins genuinely needed by that task"),
};

function projectMarkdown(item, language) {
  const lang = language === "en" ? "en" : "zh";
  const title = lang === "en" ? item.label_en : item.label;
  const description = lang === "en" ? item.description_en : item.description;
  const family = projectFamilies[item.category_key] ?? projectFamilies.general;
  const delivery = projectDelivery[item.key];
  if (!delivery) throw new Error(`Missing project delivery path: ${item.key}`);
  const capabilities = projectCapabilityHints[item.category_key] ?? projectCapabilityHints.general;
  return `---
name: chatos-project-type-${item.key.replaceAll("_", "-")}
description: ${description}
---

# ${title}

## ${lang === "en" ? "Project focus" : "项目重点"}

${bullets([description, ...family[lang]])}

## ${lang === "en" ? "Suggested delivery path" : "建议交付路径"}

${numbered(delivery[lang])}

## ${lang === "en" ? "Todo capabilities and plugins" : "Todo 能力与插件"}

- ${capabilities[lang].join(" ")}
- ${lang === "en"
    ? "Put capability selection on the specific execution Todo, not on the Agent profile. Split work when different tasks require different authority or plugins."
    : "能力选择应绑定到具体执行 Todo，而不是固化在 Agent 身上；不同任务需要不同权限或插件时，应拆成边界清楚的 Todo。"}
- ${lang === "en"
    ? "Executors discover the concrete installed tool with capability_search → capability_describe → capability_invoke; the Skill describes capability intent, not a fixed plugin identity."
    : "执行者通过 capability_search → capability_describe → capability_invoke 发现本机实际工具；Skill 只描述能力意图，不绑定固定插件身份。"}

## ${lang === "en" ? "Shared assets and completion" : "共享资产与完成"}

- ${lang === "en"
    ? "Keep project background, current architecture or operating model, key decisions, shared terminology, milestone status, and reusable evidence in team shared assets."
    : "把项目背景、当前架构或运营模型、关键决策、共享术语、里程碑状态和可复用证据维护在团队共享资产中。"}
- ${lang === "en"
    ? "Completion requires accessible deliverables, acceptance evidence, an honest residual-risk statement, and a handoff that another team member can continue."
    : "完成必须包含可访问的交付物、验收证据、如实说明的剩余风险，以及其他成员可以继续工作的交接。"}

## ${lang === "en" ? "Tailoring rule" : "裁剪规则"}

${lang === "en"
    ? "Apply only the stages and evidence required by the real deliverable and risk. Do not invent UI design, Docker deployment, migration, external publication, or production operations for work that does not need them; when required, represent them as explicit team Todos with owners, prerequisites, selected capabilities, and acceptance evidence."
    : "只采用真实交付物和风险所需的阶段与证据。不得为不需要的项目机械增加界面设计、Docker、迁移、外部发布或生产运维；确实需要时，应建立有负责人、前置关系、已选能力和验收证据的团队 Todo。"}
`;
}

catalog.professions = catalog.professions.map((item) => ({
  ...item,
  skill_name: `chatos-profession-${item.key.replaceAll("_", "-")}`,
  skill_markdown: professionMarkdown(item, "zh"),
  skill_markdown_en: professionMarkdown(item, "en"),
}));
catalog.projectTypes = catalog.projectTypes.map((item) => ({
  ...item,
  rule_markdown: projectMarkdown(item, "zh"),
  rule_markdown_en: projectMarkdown(item, "en"),
}));

if (catalog.professions.length !== 33 || catalog.projectTypes.length !== 27) {
  throw new Error("Unexpected catalog size");
}
fs.writeFileSync(outputPath, `${JSON.stringify(catalog, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
