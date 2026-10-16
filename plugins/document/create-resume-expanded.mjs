import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const root = path.resolve(import.meta.dirname);
const node = '/Users/lilei/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node';
const outputDir = process.env.RESUME_OUTPUT_DIR;
if (!outputDir) throw new Error('RESUME_OUTPUT_DIR is required');
await mkdir(outputDir, { recursive: true });
const client = new Client({ name: 'resume-expanded-visual-test', version: '1.0.0' });
await client.connect(new StdioClientTransport({
  command: node,
  args: [path.join(root, 'bin/chatos-document-mcp'), 'mcp'],
  env: { ...process.env, CHATOS_PLUGIN_ROOT: root, CHATOS_PLUGIN_ARTIFACT_DIR: outputDir },
  stderr: 'inherit'
}));
async function call(name, args) {
  const result = await client.callTool({ name, arguments: args }, undefined, { timeout: 120_000 });
  if (result.isError) throw new Error(JSON.stringify(result.structuredContent));
  return result;
}

const operations = [
  { type: 'word_set_document_layout', pageSize: 'a4', marginTop: '1.7cm', marginBottom: '1.6cm', marginLeft: '1.9cm', marginRight: '1.9cm', marginFooter: '0.8cm' },
  { type: 'word_define_style', id: 'Title', name: 'Title', basedOn: 'Normal', font: 'Heiti SC', size: '22pt', bold: true, color: '000000', spaceAfter: '3pt', keepNext: true },
  { type: 'word_define_style', id: 'ResumeBody', name: 'Resume Body', basedOn: 'Normal', font: 'Heiti SC', size: '11pt', color: '222222', lineSpacing: '1.2x', spaceAfter: '4pt', widowControl: true },
  { type: 'word_define_style', id: 'ResumeSection', name: 'Resume Section', basedOn: 'ResumeBody', size: '13pt', bold: true, color: '000000', spaceBefore: '9pt', spaceAfter: '5pt', keepNext: true },
  { type: 'word_define_style', id: 'ResumeRole', name: 'Resume Role', basedOn: 'ResumeBody', size: '11pt', bold: true, color: '000000', spaceBefore: '6pt', spaceAfter: '3pt', keepNext: true },
  { type: 'word_define_style', id: 'ResumeBullet', name: 'Resume Bullet', basedOn: 'ResumeBody', hangingIndent: '0.45cm', spaceAfter: '3pt', keepLines: true },
  { type: 'word_define_style', id: 'ResumeMeta', name: 'Resume Meta', basedOn: 'ResumeBody', size: '10.5pt', color: '444444', spaceAfter: '5pt', keepNext: true }
];
const paragraph = (text, style = 'ResumeBody') => operations.push({ type: 'word_add_paragraph', text, style });
const section = (text, newPage = false) => operations.push({ type: 'word_add_paragraph', text, style: 'ResumeSection', pageBreakBefore: newPage });
const role = (text) => paragraph(text, 'ResumeRole');
const bullet = (text) => operations.push({ type: 'word_add_list_item', text, ordered: false, style: 'ResumeBullet' });

// Page 1: profile, technology breadth, and complete employment chronology.
paragraph('李磊', 'Title');
paragraph('全栈工程师  |  AI 工程化与研发工具', 'ResumeMeta');
paragraph('个人项目  https://github.com/leeohoo/chatos_rs', 'ResumeMeta');
section('职业概述');
paragraph('拥有 8 年以上全栈研发、技术管理与项目交付经验，经历技术主管、高级技术顾问和研发工程师岗位。工作覆盖需求分析、架构设计、前后端实现、研发协作与上线交付，近期重点投入 AI Agent、IDE 插件和开发效率工具的工程化落地。');
section('技术能力');
bullet('后端开发：熟悉 Java、Spring Boot、Spring Cloud，能够围绕业务需求拆分服务、设计接口并推进核心模块实现；熟悉 Rust、Axum、Tokio，以及 Python 工具开发。');
bullet('前端与全栈：熟悉 React、Vue、TypeScript，具备前后端联合开发和工程工具建设能力；在架构看板项目中使用 G6 表达服务与调用关系。');
bullet('微服务与中间件：熟悉 Redis、RabbitMQ / RocketMQ、Elasticsearch、MongoDB、Nacos、Dubbo、OpenFeign，能够结合业务特点讨论服务集成与技术选型。');
bullet('AI 与开发工具：具备多模型接入、工具编排、MCP、长期记忆和对话式开发平台建设经验；能够将模型能力与代码探索、修改、注释和测试等研发场景结合。');
section('工作经历');
role('智己汽车  |  预研团队  |  研发工程师    2024.03 - 至今');
bullet('负责内容技术预研与工程化落地，围绕内容集群、服务调用链和 AI 开发工具建设内部效率平台，推动预研能力向实际研发场景转化。');
bullet('建设架构看板和 IDEA 插件，聚合服务调用关系、辅助跨服务依赖分析；开发对接火山引擎多模型的内部 AI IDEA 插件。');
role('凯捷咨询（中国）有限公司  |  高级技术顾问    2021.12 - 2024.03');
bullet('参与多个客户项目的方案设计、研发协作与交付推进，覆盖电商、返利、电子签章等业务域，承担核心模块设计、代码评审、团队辅导和客户沟通。');
bullet('参与 4 个项目的技术支持与交付工作；建设内部知识库并组织新员工实训，沉淀可复用的项目经验和培训材料。');
role('苏州金强信息科技有限公司  |  技术主管    2017.10 - 2021.11');
bullet('主导技术选型、架构设计与前后端基础框架建设，管理研发团队，负责需求评审、任务拆分、代码评审、测试部署与交付风险控制。');
bullet('面向政企客户梳理需求并提供解决方案，组织技术培训和经验分享；担任代码评审组长，推动开发规范落地并解决关键项目中的技术问题。');

// Page 2: expand the projects explicitly described in the supplied resume.
section('代表项目与研发实践', true);
paragraph('项目经历集中在研发工具、服务架构可视化与 AI 辅助开发，既包含服务端和前端实现，也包含 IDE 内的工程集成。');
role('服务架构看板');
paragraph('所属经历：智己汽车    技术：Spring Boot、G6、服务调用关系', 'ResumeMeta');
bullet('项目目标：将公司内容集群中的服务和调用关系集中呈现，帮助研发人员理解系统结构，在跨服务协作和需求分析时获得更直观的架构视图。');
bullet('主要工作：建设架构看板，聚合服务与调用关系；后端基于 Spring Boot 提供服务支持，前端集成 G6，将关系信息表达为可阅读的图形。');
bullet('工程协作：结合研发人员对服务关系的理解与使用反馈，持续梳理看板中的信息表达，使架构视图能够服务实际开发和沟通，而不仅停留在预研展示。');
role('服务调用链 IDEA 插件');
paragraph('所属经历：智己汽车    技术：IDEA 插件、IDE API、后端集成', 'ResumeMeta');
bullet('项目目标：把调用链识别与代码定位能力放入开发人员日常使用的 IDE，减少理解跨服务依赖时在代码、接口资料和架构信息之间反复切换。');
bullet('主要工作：基于 IDE API 扫描和定位服务调用链，将识别的信息上报后端，为服务依赖理解和架构看板的数据聚合提供支持。');
bullet('实践重点：把 IDE 侧代码分析与后端关系信息衔接起来，兼顾代码定位和服务视图两种使用方式，辅助研发人员从代码入口理解系统调用关系。');
role('内部 AI IDEA 插件');
paragraph('所属经历：智己汽车    技术：火山引擎多模型、对话式代码工具', 'ResumeMeta');
bullet('项目目标：将多模型能力引入 IDE，使研发人员能够在开发环境中进行对话式代码探索与修改，并围绕现有代码完成注释和测试生成。');
bullet('主要工作：开发内部 AI IDEA 插件，对接火山引擎多模型，支持代码探索、代码修改、注释生成、测试生成等功能，面向具体开发任务组织工具能力。');
bullet('落地推进：与业务及研发团队对齐使用场景、实施路径和反馈方式，推动预研能力产品化，并根据实际使用情况持续调整工具体验。');
role('客户项目交付与知识沉淀');
bullet('在电商、返利、电子签章等项目中梳理业务需求，输出技术实现方案，协调研发协作；通过核心模块设计、评审和客户沟通推进交付。');
bullet('将交付中的实施经验整理到内部知识库，配合新人实训和考核目标，帮助团队积累可复用资料，支持后续项目实施与新成员上手。');

// Page 3: the personal project is substantiated by this repository's README.
section('个人项目与技术管理', true);
role('ChatOS 对话式 AI 项目工作台');
paragraph('项目地址  https://github.com/leeohoo/chatos_rs', 'ResumeMeta');
paragraph('围绕 AI 在真实项目中持续完成任务的需求建设桌面工作台，将项目上下文、对话、后台任务、本地文件、Git、终端和插件能力放入统一的工作流程。项目强调执行过程可查看，用户能够了解任务进度、工具调用和最终产物。');
role('项目架构与能力');
bullet('本地执行与云端服务：项目采用原生桌面客户端和 Rust 本地执行组件；本地侧承载对话、任务和工具执行，云端提供账户、模型配置、长期记忆、插件目录等服务。');
bullet('项目上下文与长期记忆：围绕项目组织对话、任务历史和可复用上下文，通过会话摘要、项目事实及分层记忆支持跨会话协作，减少长期开发中重复说明背景的成本。');
bullet('任务执行与可观察性：支持将复杂请求组织为可持续执行的任务，呈现进度、日志、工具调用、审批、重试和结果，让用户能够在执行过程中补充指导并跟踪状态。');
bullet('MCP 与插件体系：通过 MCP 工具、Skills、权限声明和受管理的产物扩展本地能力，项目中的插件覆盖浏览器操作、电脑操作、文档处理和图表等场景。');
bullet('项目工作区：集成文件浏览与编辑、全文搜索、Git 状态与差异、终端、运行配置及日志，围绕项目中的真实文件组织 AI 工具调用与结果呈现。');
role('文档工具实践');
paragraph('在文档插件中完善 Word 页面布局、复用样式、段落格式、列表和页码能力，并让渲染工具直接向模型返回页面图片。生成后通过结构校验、截图检查与修改后的再次渲染核对结果，使排版问题能够在交付前被发现。');
role('技术管理与交付经验');
bullet('技术方案与架构：在技术主管和顾问岗位中，承担技术选型、架构设计与基础框架建设，将客户需求转化为可实施的研发方案，兼顾现有项目条件和交付要求。');
bullet('团队协作与质量：组织需求评审、任务拆分和代码评审，协调测试部署与交付工作；通过开发规范、评审和技术讨论形成一致的实施方式。');
bullet('客户沟通与风险控制：与客户及业务团队讨论需求和解决方案，协调技术实现与交付进度，结合项目反馈持续识别和处理交付中的问题。');
bullet('培训与知识复用：组织技术培训、项目经验分享和新员工实训，制定考核目标，建设内部知识库，使团队能够复用已有经验和实施资料。');
role('认证与职业方向');
paragraph('持有 PMP 项目管理认证。职业方向聚焦全栈研发、AI 工程化与研发工具建设，结合技术实现、团队协作和项目交付经验，持续推进 AI 能力在实际开发场景中的应用。');
operations.push({ type: 'word_add_header_footer', location: 'footer', field: 'page', align: 'right', font: 'Arial', size: '9pt', color: '666666' });

try {
  const created = await call('office_create', { format: 'docx', outputName: 'li-lei-resume-expanded.docx', locale: 'zh-CN', operations });
  const inputPath = created.structuredContent.artifact.relativePath;
  const previews = [];
  for (const [index, pages] of [[1, 2], [3, 4]].entries()) {
    const result = await call('document_render', { inputPath, outputPrefix: `resume-expanded-preview-${index + 1}`, pages, viewportWidth: 1400, viewportHeight: 1800 });
    previews.push({ ...result.structuredContent, imageCount: result.content.filter((part) => part.type === 'image').length });
  }
  const renderedPages = previews.flatMap((preview) => preview.pages.map((page) => page.page));
  const validated = await call('document_validate', { inputPath, renderPages: renderedPages, viewportWidth: 1400, viewportHeight: 1800 });
  console.log(JSON.stringify({ artifactPath: path.join(outputDir, inputPath), created: created.structuredContent, previews, validation: validated.structuredContent }, null, 2));
} finally {
  await client.close();
}
