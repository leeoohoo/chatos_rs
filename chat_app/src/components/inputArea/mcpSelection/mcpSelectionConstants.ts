import type { McpToolsetPresetSpec } from './mcpSelectionTypes';

export const AGENT_BUILDER_MCP_ID = 'builtin_agent_builder';

export const PROJECT_REQUIRED_MCP_IDS = new Set([
  'builtin_code_maintainer',
  'builtin_code_maintainer_read',
  'builtin_code_maintainer_write',
  'builtin_terminal_controller',
]);

export const REMOTE_REQUIRED_MCP_IDS = new Set([
  'builtin_remote_connection_controller',
]);

export const MCP_TOOLSET_PRESET_SPECS: McpToolsetPresetSpec[] = [
  {
    id: 'coding',
    label: '代码开发',
    description: '代码读写 + 终端 + 任务，适合实现与调试',
    preferredIds: [
      'builtin_code_maintainer_read',
      'builtin_code_maintainer_write',
      'builtin_code_maintainer',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_notepad',
    ],
  },
  {
    id: 'web_research',
    label: 'Web 研究',
    description: '网页搜索/提取 + 浏览器自动化 + 只读代码',
    preferredIds: [
      'builtin_web_tools',
      'builtin_browser_tools',
      'builtin_code_maintainer_read',
      'builtin_notepad',
    ],
  },
  {
    id: 'remote_ops',
    label: '远程运维',
    description: '远程连接 + 终端 + 任务，适合服务器排障',
    preferredIds: [
      'builtin_remote_connection_controller',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_code_maintainer_read',
    ],
  },
  {
    id: 'minimal',
    label: '轻量模式',
    description: '仅保留最小必要工具，减少噪音',
    preferredIds: [
      'builtin_code_maintainer_read',
      'builtin_terminal_controller',
    ],
  },
];
