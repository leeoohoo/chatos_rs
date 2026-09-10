import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { PlanningStore } from '../dist/store.mjs';
import { startPlanningServer } from '../dist/http.mjs';

// Disposable visual test data. Never opens a user's installed plugin database.
const dir = mkdtempSync(path.join(tmpdir(), 'chatos-planning-preview-'));
const store = new PlanningStore({ CHATOS_CONTEXT_SCOPE: 'project', CHATOS_PROJECT_ID: 'preview-only', CHATOS_PROJECT_NAME: 'ChatOS · 项目管理预览', CHATOS_CONTEXT_SCOPE_ID: 'a'.repeat(64), CHATOS_PLUGIN_DATA_DIR: dir });
const change = fields => store.mutate({ requestId: randomUUID(), expectedRevision: store.read().revision, ...fields }).result.id;
const req = change({ operation: 'requirement.create', title: '客户端项目与规划插件', detail: '客户端是项目主体的唯一权威。需求、文档、工作项和规划在插件内保存。', acceptanceCriteria: '断网仍可编辑资料；UI 与 MCP 使用相同存储；业务验收和执行状态分开。', parentId: null, status: 'draft' });
const child = change({ operation: 'requirement.create', title: '文档与范围闭包', detail: '提供独立文档版本、需求树与双向依赖浏览。', acceptanceCriteria: 'Plan 固定具体文档版本，范围图包含子需求与前置依赖。', parentId: req, status: 'draft' });
change({ operation: 'document.create', title: '客户端项目权威技术方案', kind: 'technical', format: 'markdown', status: 'published', requirementIds: [req, child], content: '# 技术方案\n\n## 数据所有权\n客户端持有项目身份，插件持有规划资料，Task Runner 持有执行记录。\n\n## 存储\n使用宿主隔离目录下的 SQLite；所有变更带 revision 与幂等键。\n\n- 不查询 Project 微服务\n- 不复制 Git 状态\n- 不在插件保存运行状态' });
change({ operation: 'document.create', title: '规划闭环关系图', kind: 'design', format: 'svg', status: 'published', requirementIds: [child], content: '<svg viewBox="0 0 520 160"><rect x="16" y="50" width="130" height="54" rx="12" fill="#eaf4ff" stroke="#007aff"/><text x="81" y="82" text-anchor="middle" fill="#1d1d1f">客户端项目</text><path d="M150 77h74" stroke="#007aff"/><rect x="228" y="50" width="130" height="54" rx="12" fill="#eaf4ff" stroke="#007aff"/><text x="293" y="82" text-anchor="middle" fill="#1d1d1f">规划插件</text><path d="M362 77h42" stroke="#ff9f0a"/><rect x="408" y="50" width="96" height="54" rx="12" fill="#fff6df" stroke="#ff9f0a"/><text x="456" y="82" text-anchor="middle" fill="#1d1d1f">Task Runner</text></svg>' });
change({ operation: 'requirement.update', ...store.read().requirements.find(value => value.id === req), status: 'approved' });
change({ operation: 'requirement.update', ...store.read().requirements.find(value => value.id === child), status: 'approved' });
const work = change({ operation: 'work_item.create', requirementId: req, title: '完成需求与工作项编辑', detail: '共用本地业务存储，实现并发冲突检测和依赖环校验。', acceptanceCriteria: 'HTTP 与 stdio MCP 可交替读写相同资料，失败事务不改变 revision。', status: 'todo' });
const childWork = change({ operation: 'work_item.create', requirementId: child, title: '完成多版本文档与范围图', detail: '实现安全 Markdown/SVG 阅读和关系闭包。', acceptanceCriteria: '脚本、外部资源与未知 SVG 元素不会进入预览 DOM。', status: 'todo' });
change({ operation: 'work_item.update', ...store.read().workItems.find(value => value.id === work), status: 'ready' });
change({ operation: 'work_item.update', ...store.read().workItems.find(value => value.id === childWork), status: 'ready', prerequisiteIds: [work] });
const plan = change({ operation: 'plan.create', title: '本地业务闭环 · 第一版', workItemIds: [childWork] });
change({ operation: 'plan.approve', id: plan });
const intent = change({ operation: 'execution.prepare', planId: plan, title: '交付本地项目管理插件' });
change({ operation: 'execution.approve', id: intent });
change({ operation: 'execution.link', intentId: intent, referenceType: 'batch', externalId: 'task-batch-preview', url: 'https://tasks.example/preview', revision: '1' });
const { server, origin } = await startPlanningServer(store, 0);
process.stdout.write(`${origin}\n`);
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => { server.closeAllConnections(); server.close(() => { store.close(); rmSync(dir, { recursive: true }); process.exit(0); }); });
