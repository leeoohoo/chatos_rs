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
    label: 'Coding',
    description: 'Code read/write + terminal + tasks',
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
    label: 'Web research',
    description: 'Web search/extraction + browser automation + read-only code',
    preferredIds: [
      'builtin_web_tools',
      'builtin_browser_tools',
      'builtin_code_maintainer_read',
      'builtin_notepad',
    ],
  },
  {
    id: 'remote_ops',
    label: 'Remote ops',
    description: 'Remote connection + terminal + tasks',
    preferredIds: [
      'builtin_remote_connection_controller',
      'builtin_terminal_controller',
      'builtin_task_manager',
      'builtin_code_maintainer_read',
    ],
  },
  {
    id: 'minimal',
    label: 'Minimal',
    description: 'Minimum necessary tools',
    preferredIds: [
      'builtin_code_maintainer_read',
      'builtin_terminal_controller',
    ],
  },
];
