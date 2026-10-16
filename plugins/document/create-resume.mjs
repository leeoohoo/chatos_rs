import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const root = path.resolve(import.meta.dirname);
const node = '/Users/lilei/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node';
const outputDir = process.env.RESUME_OUTPUT_DIR;
if (!outputDir) throw new Error('RESUME_OUTPUT_DIR is required');
await mkdir(outputDir, { recursive: true });

const transport = new StdioClientTransport({
  command: node,
  args: [path.join(root, 'bin/chatos-document-mcp'), 'mcp'],
  env: {
    ...process.env,
    CHATOS_PLUGIN_ROOT: root,
    CHATOS_PLUGIN_ARTIFACT_DIR: outputDir
  },
  stderr: 'inherit'
});
const client = new Client({ name: 'resume-e2e-test', version: '1.0.0' });
await client.connect(transport);
const callTool = client.callTool.bind(client);
client.callTool = (params) => callTool(params, undefined, { timeout: 120_000 });

const operations = [
  {
    type: 'word_set_document_layout',
    pageSize: 'a4',
    marginTop: '1.55cm',
    marginBottom: '1.45cm',
    marginLeft: '1.8cm',
    marginRight: '1.8cm',
    marginFooter: '0.8cm'
  },
  {
    type: 'word_define_style',
    id: 'Title', name: 'Title', basedOn: 'Normal',
    font: 'Heiti SC', size: '22pt', color: '000000', bold: true,
    align: 'left', spaceAfter: '2pt', keepNext: true
  },
  {
    type: 'word_define_style',
    id: 'ResumeContact', name: 'Resume Contact', basedOn: 'Normal',
    font: 'Heiti SC', size: '10.5pt', color: '000000',
    align: 'left', spaceAfter: '5pt', keepNext: true
  },
  {
    type: 'word_define_style',
    id: 'ResumeSection', name: 'Resume Section', basedOn: 'Normal',
    font: 'Heiti SC', size: '12pt', color: '000000', bold: true,
    spaceBefore: '8pt', spaceAfter: '3pt', keepNext: true,
    widowControl: true
  },
  {
    type: 'word_define_style',
    id: 'ResumeRole', name: 'Resume Role', basedOn: 'Normal',
    font: 'Heiti SC', size: '11pt', color: '000000', bold: true,
    spaceBefore: '5pt', spaceAfter: '1pt', keepNext: true,
    widowControl: true
  },
  {
    type: 'word_define_style',
    id: 'ResumeBody', name: 'Resume Body', basedOn: 'Normal',
    font: 'Heiti SC', size: '11pt', color: '222222',
    lineSpacing: '1.12x', spaceAfter: '2pt', widowControl: true
  },
  {
    type: 'word_define_style',
    id: 'ResumeBullet', name: 'Resume Bullet', basedOn: 'ResumeBody',
    font: 'Heiti SC', size: '11pt', color: '222222',
    lineSpacing: '1.12x', spaceAfter: '1pt', hangingIndent: '0.48cm',
    widowControl: true, keepLines: true
  },
  {
    type: 'word_define_style',
    id: 'ResumeSubsection', name: 'Resume Subsection', basedOn: 'ResumeBody',
    font: 'Heiti SC', size: '10.5pt', color: '000000', bold: true,
    spaceBefore: '3pt', spaceAfter: '1pt', keepNext: true
  },
  { type: 'word_add_paragraph', text: '李磊', style: 'Title' },
  { type: 'word_add_paragraph', text: '全栈工程师', style: 'ResumeContact' },
  { type: 'word_add_paragraph', text: '个人项目  https://github.com/leeohoo/chatos_rs', style: 'ResumeContact' },
  { type: 'word_add_paragraph', text: '个人优势', style: 'ResumeSection' },
  { type: 'word_add_list_item', text: '8 年以上全栈研发、技术管理与项目交付经验，能从需求拆解、架构设计推进到上线。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '熟悉 Java / Spring Cloud、Rust（Axum / Tokio）、React / Vue / TypeScript、Python，具备前后端与工程工具落地能力。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '熟悉微服务与常用中间件：Redis、RabbitMQ / RocketMQ、Elasticsearch、MongoDB、Nacos、Dubbo、OpenFeign。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '持续深耕 AI 工程化与 Agent 体系，具备模型接入、工具编排、MCP、长期记忆与对话式开发平台建设经验。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '具备 PMP 项目管理认证和团队管理经验，擅长跨团队沟通、技术方案推进与交付风险控制。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_paragraph', text: '工作经历', style: 'ResumeSection' },
  { type: 'word_add_paragraph', text: '智己汽车｜预研团队｜研发工程师    2024.03 - 至今', style: 'ResumeRole' },
  { type: 'word_add_list_item', text: '负责内容技术预研与工程化落地，围绕内容集群、服务调用链和 AI 开发工具建设内部效率平台。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '建设架构看板，聚合公司内容集群服务与调用关系，前端集成 G6，后端基于 Spring Boot。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '开发 IDEA 插件，基于 IDE API 扫描与定位服务调用链并上报后端，辅助研发侧理解跨服务依赖。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '开发内部 AI IDEA 插件，对接火山引擎多模型，支持对话式代码探索、修改、注释生成与测试生成。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '推动预研能力产品化，与业务及研发团队对齐落地路径、反馈闭环和持续优化方向。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_paragraph', text: '凯捷咨询（中国）有限公司｜高级技术顾问    2021.12 - 2024.03', style: 'ResumeRole' },
  { type: 'word_add_list_item', text: '作为高级技术顾问参与多个客户项目的方案设计、研发协作与交付推进。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '面向电商、返利、电子签章等业务域梳理需求，输出技术实现方案并协调落地。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '负责核心模块设计、代码评审、团队辅导与客户沟通，平衡交付质量与项目进度。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '沉淀内部知识库和新人实训材料，提升实施复用效率与团队上手速度。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_paragraph', text: '项目业绩', style: 'ResumeSubsection' },
  { type: 'word_add_list_item', text: '参与 4 个项目的技术支持与交付工作，获得团队与客户认可。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '主导建设公司内部知识库系统，提升实施效率与项目质量。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '组织新员工实训并制定考核目标，促进新人快速上手。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_paragraph', text: '苏州金强信息科技有限公司｜技术主管    2017.10 - 2021.11', style: 'ResumeRole' },
  { type: 'word_add_list_item', text: '主导技术选型、架构设计与前后端基础框架建设，为公司项目交付提供技术底座。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '管理研发团队，负责需求评审、任务拆分、代码评审、测试部署与交付风险控制。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '与客户深度沟通，梳理需求并提供解决方案，推动多个政企项目落地。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '组织技术培训与分享，建立代码规范和评审机制，提升团队协作效率。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_paragraph', text: '项目业绩', style: 'ResumeSubsection' },
  { type: 'word_add_list_item', text: '搭建高效的前后端开发框架，提升项目启动速度。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '担任代码评审组长，推动代码规范落地。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_list_item', text: '带领团队完成多个关键项目并解决技术难题。', ordered: false, style: 'ResumeBullet' },
  { type: 'word_add_header_footer', location: 'footer', field: 'page', align: 'right', font: 'Arial', size: '8.5pt', color: '777777' }
];

const created = await client.callTool({
  name: 'office_create',
  arguments: { format: 'docx', outputName: 'li-lei-resume.docx', locale: 'zh-CN', operations }
});
if (created.isError) throw new Error(JSON.stringify(created.structuredContent));
const artifactPath = path.join(outputDir, created.structuredContent.artifact.relativePath);
console.log(JSON.stringify({ created: created.structuredContent, artifactPath }, null, 2));

const rendered = await client.callTool({
  name: 'document_render',
  arguments: { inputPath: created.structuredContent.artifact.relativePath, outputPrefix: 'li-lei-resume-preview', pages: [1, 2], viewportWidth: 1200, viewportHeight: 1400 }
});
if (rendered.isError) throw new Error(JSON.stringify(rendered.structuredContent));
console.log(JSON.stringify({ rendered: rendered.structuredContent, imageCount: rendered.content.filter((part) => part.type === 'image').length }, null, 2));

const validated = await client.callTool({
  name: 'document_validate',
  arguments: { inputPath: created.structuredContent.artifact.relativePath, renderPages: rendered.structuredContent.pages.map((page) => page.page), viewportWidth: 1200, viewportHeight: 1400 }
});
console.log(JSON.stringify({ validated: validated.structuredContent }, null, 2));
await client.close();
