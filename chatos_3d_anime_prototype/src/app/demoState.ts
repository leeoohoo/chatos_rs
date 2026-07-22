import { demoProjects, demoTasks, initialMessages } from '../demoData';
import type {
  ChatAgentOption,
  ChatContact,
  ChatMessage,
  ChatModelOption,
  ChatRuntimeSettings,
  ChatSession,
  DemoTaskGraph,
} from '../types';

export const DEMO_CHAT_SESSIONS: ChatSession[] = [
  { id: 'demo-general', title: '架构师 · 图灵', projectId: null, updatedAt: '刚刚', archived: false },
  { id: 'demo-room', title: '架构师 · 图灵', projectId: demoProjects[0].id, updatedAt: '刚刚', archived: false },
  { id: 'demo-project', title: '项目管家 · 小旅', projectId: demoProjects[1].id, updatedAt: '18 分钟前', archived: false },
  { id: 'demo-ideas', title: '内容助手 · 知秋', projectId: demoProjects[4].id, updatedAt: '昨天', archived: false },
];

export const DEMO_SESSION_CONTACT_IDS: Record<string, string> = {
  'demo-general': 'contact-architect',
  'demo-room': 'contact-architect',
  'demo-project': 'contact-planner',
  'demo-ideas': 'contact-editor',
};

export const DEMO_CHAT_CONTACTS: ChatContact[] = [
  { id: 'contact-architect', agentId: 'agent-architect', name: '架构师 · 图灵', description: '负责技术方案、代码实现与系统设计', sessionId: 'demo-general', projectId: null, lastActive: '刚刚' },
  { id: 'contact-planner', agentId: 'agent-planner', name: '项目管家 · 小旅', description: '负责项目拆解、计划推进与风险跟踪', sessionId: null, projectId: null, lastActive: '18 分钟前' },
  { id: 'contact-editor', agentId: 'agent-editor', name: '内容助手 · 知秋', description: '负责资料整理、写作和知识归档', sessionId: null, projectId: null, lastActive: '昨天' },
];

export const DEMO_AVAILABLE_AGENTS: ChatAgentOption[] = [
  { id: 'agent-designer', name: '视觉设计师 · 澄空', description: '界面、视觉与交互设计', enabled: true },
  { id: 'agent-tester', name: '测试工程师 · 山雀', description: '自动化测试、质量检查与回归验证', enabled: true },
];

export const DEMO_PROJECT_CONTACT_IDS: Record<string, string[]> = {
  [demoProjects[0].id]: ['contact-architect'],
  [demoProjects[1].id]: ['contact-planner'],
  [demoProjects[4].id]: ['contact-editor'],
};

export const DEMO_CHAT_MODELS: ChatModelOption[] = [
  { id: 'demo-gpt', name: 'ChatOS 智能模型', modelName: 'chatos-demo', thinkingLevel: 'medium', supportsImages: true, supportsReasoning: true, enabled: true },
  { id: 'demo-fast', name: 'ChatOS 快速模型', modelName: 'chatos-fast-demo', thinkingLevel: 'low', supportsImages: true, supportsReasoning: true, enabled: true },
];

export const DEMO_RUNTIME_SETTINGS: ChatRuntimeSettings = {
  selectedModelId: DEMO_CHAT_MODELS[0].id,
  selectedModelName: DEMO_CHAT_MODELS[0].modelName,
  selectedThinkingLevel: DEMO_CHAT_MODELS[0].thinkingLevel,
  reasoningEnabled: true,
  planModeEnabled: false,
};

export const EMPTY_DEMO_TASK_GRAPH: DemoTaskGraph = {
  rootTaskIds: [],
  nodes: [],
  edges: [],
  sourceSessionId: null,
  sourceTurnId: null,
  sourceUserMessageId: null,
};

export const DEMO_TASK_GRAPH: DemoTaskGraph = {
  rootTaskIds: ['task-fallback'],
  nodes: demoTasks.map((task, index) => ({
    id: task.id,
    title: task.title,
    detail: task.detail,
    status: task.status,
    progress: task.progress,
    depth: index,
    isRoot: task.id === 'task-fallback',
    isCurrent: task.status === 'doing',
    prerequisiteIds: task.id === 'task-assets'
      ? ['task-fallback']
      : task.id === 'task-scene'
        ? ['task-assets']
        : task.id === 'task-chat'
          ? ['task-scene']
          : [],
    creatorName: 'ChatOS Agent',
    updatedAt: task.updatedAt || '刚刚',
    resultSummary: task.status === 'done' ? task.detail : null,
  })),
  edges: [
    { id: 'demo-fallback-assets', source: 'task-fallback', target: 'task-assets' },
    { id: 'demo-assets-scene', source: 'task-assets', target: 'task-scene' },
    { id: 'demo-scene-chat', source: 'task-scene', target: 'task-chat' },
  ],
  sourceSessionId: 'demo-room',
  sourceTurnId: 'demo-task-flow',
  sourceUserMessageId: 'welcome-user',
};

export const DEMO_SESSION_MESSAGES: Record<string, ChatMessage[]> = {
  'demo-general': initialMessages,
  'demo-room': initialMessages,
  'demo-project': [
    { id: 'demo-project-user', role: 'user', content: '把旅行项目里的路线和预算整理成一个清晰的执行计划。', time: '09:18' },
    { id: 'demo-project-ai', role: 'assistant', content: '可以。我会按下面顺序整理：\n\n1. 确认城市和日期\n2. 拆分每日路线\n3. 汇总交通、住宿与餐饮预算\n4. 标记需要预订的项目', time: '09:18' },
  ],
  'demo-ideas': [
    { id: 'demo-ideas-ai', role: 'assistant', content: '这里可以存放零散灵感、代码片段和待办。示例代码：\n\n```ts\nconst room = await createWorkspace({ mode: "3d" });\n```', time: '昨天' },
  ],
};
