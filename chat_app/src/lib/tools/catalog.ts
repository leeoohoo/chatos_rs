import { getToolDisplayName } from './displayName';

export type ToolFamily =
  | 'browser'
  | 'web'
  | 'code'
  | 'process'
  | 'remote'
  | 'notepad'
  | 'task'
  | 'ui'
  | 'agent'
  | 'memory'
  | 'generic';

const CODE_TOOL_NAMES = new Set([
  'read_file_raw',
  'read_file_range',
  'read_file',
  'list_dir',
  'search_text',
  'search_files',
  'write_file',
  'edit_file',
  'append_file',
  'delete_path',
  'apply_patch',
  'patch',
]);

const BROWSER_TOOL_NAMES = new Set([
  'browser_navigate',
  'browser_snapshot',
  'browser_click',
  'browser_type',
  'browser_scroll',
  'browser_back',
  'browser_press',
  'browser_console',
  'browser_get_images',
  'browser_inspect',
  'browser_research',
  'browser_vision',
]);

const WEB_TOOL_NAMES = new Set([
  'web_search',
  'web_extract',
  'web_research',
]);

const PROCESS_TOOL_NAMES = new Set([
  'execute_command',
  'get_recent_logs',
  'process_list',
  'process_poll',
  'process_log',
  'process_wait',
  'process_write',
  'process_kill',
  'process',
]);

const REMOTE_TOOL_NAMES = new Set([
  'list_connections',
  'test_connection',
  'run_command',
  'list_directory',
]);

const NOTEPAD_TOOL_NAMES = new Set([
  'init',
  'list_folders',
  'create_folder',
  'rename_folder',
  'delete_folder',
  'list_notes',
  'create_note',
  'read_note',
  'update_note',
  'delete_note',
  'list_tags',
  'search_notes',
]);

const TASK_TOOL_NAMES = new Set([
  'add_task',
  'list_tasks',
  'update_task',
  'complete_task',
  'delete_task',
]);

const UI_TOOL_NAMES = new Set([
  'prompt_key_values',
  'prompt_choices',
  'prompt_mixed_form',
]);

const AGENT_TOOL_NAMES = new Set([
  'recommend_agent_profile',
  'list_available_skills',
  'create_memory_agent',
  'update_memory_agent',
  'preview_agent_context',
]);

const MEMORY_TOOL_NAMES = new Set([
  'get_command_detail',
  'get_plugin_detail',
  'get_skill_detail',
]);

const RAW_PREFIXES: Record<Exclude<ToolFamily, 'generic'>, string[]> = {
  browser: ['builtin_browser_tools_', 'browser_tools_', 'browser_'],
  web: ['builtin_web_tools_', 'web_tools_', 'web_'],
  code: [
    'builtin_code_maintainer_read_',
    'builtin_code_maintainer_write_',
    'builtin_code_maintainer_',
    'code_maintainer_read_',
    'code_maintainer_write_',
    'code_maintainer_',
  ],
  process: ['builtin_terminal_controller_', 'terminal_controller_'],
  remote: ['builtin_remote_connection_controller_', 'remote_connection_controller_'],
  notepad: ['builtin_notepad_', 'notepad_'],
  task: ['builtin_task_manager_', 'task_manager_'],
  ui: ['builtin_ui_prompter_', 'ui_prompter_'],
  agent: ['builtin_agent_builder_', 'agent_builder_'],
  memory: [
    'builtin_memory_command_reader_',
    'memory_command_reader_',
    'builtin_memory_plugin_reader_',
    'memory_plugin_reader_',
    'builtin_memory_skill_reader_',
    'memory_skill_reader_',
  ],
};

const startsWithAny = (value: string, prefixes: string[]): boolean => (
  prefixes.some((prefix) => value.startsWith(prefix))
);

export const isProcessToolName = (displayName: string): boolean => PROCESS_TOOL_NAMES.has(displayName);

export const resolveToolFamily = (
  rawName: string,
  explicitDisplayName?: string,
): ToolFamily => {
  const displayName = explicitDisplayName ?? getToolDisplayName(rawName);
  const normalizedRaw = rawName.trim();

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.browser) || BROWSER_TOOL_NAMES.has(displayName)) {
    return 'browser';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.web) || WEB_TOOL_NAMES.has(displayName)) {
    return 'web';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.remote)) {
    return 'remote';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.notepad) || NOTEPAD_TOOL_NAMES.has(displayName)) {
    return 'notepad';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.task) || TASK_TOOL_NAMES.has(displayName)) {
    return 'task';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.ui) || UI_TOOL_NAMES.has(displayName)) {
    return 'ui';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.agent) || AGENT_TOOL_NAMES.has(displayName)) {
    return 'agent';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.memory) || MEMORY_TOOL_NAMES.has(displayName)) {
    return 'memory';
  }

  if (startsWithAny(normalizedRaw, RAW_PREFIXES.code) || CODE_TOOL_NAMES.has(displayName)) {
    return 'code';
  }

  if (
    startsWithAny(normalizedRaw, RAW_PREFIXES.process)
    || PROCESS_TOOL_NAMES.has(displayName)
  ) {
    return 'process';
  }

  if (REMOTE_TOOL_NAMES.has(displayName)) {
    return 'remote';
  }

  return 'generic';
};

export const resolveToolRoutingKey = (
  rawName: string,
  explicitDisplayName?: string,
): string => {
  const displayName = explicitDisplayName ?? getToolDisplayName(rawName);
  const family = resolveToolFamily(rawName, displayName);
  return `${family}:${displayName}`;
};

