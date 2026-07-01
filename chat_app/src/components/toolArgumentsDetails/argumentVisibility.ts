// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { resolveToolRoutingKey } from '../../lib/tools/catalog';
import { getToolDisplayName } from '../../lib/tools/displayName';

const GENERIC_HIDDEN_ARGUMENT_KEYS = new Set([
  'annotation_count',
  'annotationCount',
  'clear_applied',
  'clearApplied',
  'debug',
  'fallback',
  'fallback_used',
  'fallbackUsed',
  'include_metadata',
  'includeMetadata',
  'include_raw',
  'includeRaw',
  'max_results',
  'maxResults',
  'model',
  'process_id',
  'processId',
  'prompt_source',
  'promptSource',
  'provider',
  'terminal_id',
  'terminalId',
  'timeout',
  'timeout_ms',
  'timeoutMs',
  'transport',
  'with_line_numbers',
  'withLineNumbers',
]);

const TOOL_ARGUMENT_ALLOWLIST: Record<string, Set<string>> = {
  'code:read_file': new Set(['path', 'start_line', 'startLine', 'end_line', 'endLine']),
  'code:read_file_raw': new Set(['path', 'start_line', 'startLine', 'end_line', 'endLine']),
  'code:read_file_range': new Set(['path', 'start_line', 'startLine', 'end_line', 'endLine']),
  'code:search_text': new Set(['path', 'pattern', 'query', 'text', 'directory', 'dir']),
  'code:search_files': new Set(['path', 'pattern', 'query', 'glob', 'directory', 'dir']),
  'code:list_dir': new Set(['path', 'directory', 'dir']),
  'code:write_file': new Set(['path']),
  'code:edit_file': new Set(['path']),
  'code:append_file': new Set(['path']),
  'code:delete_path': new Set(['path']),
  'code:apply_patch': new Set([]),
  'code:patch': new Set([]),
  'browser:browser_open': new Set(['url']),
  'browser:browser_snapshot': new Set(['full']),
  'browser:browser_click': new Set(['ref']),
  'browser:browser_type': new Set(['ref', 'text']),
  'browser:browser_scroll': new Set(['direction']),
  'browser:browser_back': new Set([]),
  'browser:browser_press': new Set(['key']),
  'browser:browser_console': new Set(['clear', 'expression']),
  'browser:browser_get_images': new Set([]),
  'browser:browser_inspect': new Set(['question', 'full', 'annotate']),
  'browser:browser_research': new Set(['question', 'web_query', 'include_web', 'web_limit', 'extract_top', 'full', 'annotate']),
  'browser:browser_vision': new Set(['question', 'annotate']),
  'browser:browser_navigate': new Set(['url']),
  'web:web_search': new Set(['query']),
  'web:web_extract': new Set(['url', 'urls', 'selected_urls', 'selectedUrls']),
  'web:web_research': new Set(['query', 'url', 'urls']),
  'process:execute_command': new Set(['path', 'common', 'command', 'background']),
  'process:get_recent_logs': new Set(['per_terminal_limit', 'terminal_limit']),
  'process:process_list': new Set(['include_exited', 'limit']),
  'process:process_poll': new Set(['terminal_id', 'offset', 'limit']),
  'process:process_log': new Set(['terminal_id', 'offset', 'limit']),
  'process:process_wait': new Set(['terminal_id', 'timeout_ms', 'timeout']),
  'process:process_write': new Set(['terminal_id', 'data', 'submit']),
  'process:process_kill': new Set(['terminal_id']),
  'process:process': new Set(['action', 'terminal_id', 'include_exited', 'offset', 'limit', 'timeout_ms', 'timeout', 'data']),
  'remote:list_connections': new Set([]),
  'remote:test_connection': new Set([]),
  'remote:run_command': new Set(['command', 'timeout_seconds', 'allow_dangerous']),
  'remote:list_directory': new Set(['path', 'limit']),
  'remote:read_file': new Set(['path', 'max_bytes']),
  'notepad:init': new Set([]),
  'notepad:list_folders': new Set([]),
  'notepad:create_folder': new Set(['folder']),
  'notepad:rename_folder': new Set(['from', 'to']),
  'notepad:delete_folder': new Set(['folder', 'recursive']),
  'notepad:list_notes': new Set(['folder', 'recursive', 'tags', 'match', 'query', 'limit']),
  'notepad:create_note': new Set(['folder', 'title', 'content', 'tags']),
  'notepad:read_note': new Set(['id']),
  'notepad:update_note': new Set(['id', 'title', 'content', 'folder', 'tags']),
  'notepad:delete_note': new Set(['id']),
  'notepad:list_tags': new Set([]),
  'notepad:search_notes': new Set(['query', 'folder', 'recursive', 'tags', 'match', 'includeContent', 'limit']),
  'task:add_task': new Set(['tasks', 'title', 'details', 'priority', 'status', 'tags', 'due_at']),
  'task:list_tasks': new Set(['include_done', 'current_turn_only', 'limit']),
  'task:update_task': new Set(['task_id', 'changes']),
  'task:complete_task': new Set(['task_id']),
  'task:delete_task': new Set(['task_id']),
  'ui:prompt_key_values': new Set(['title', 'message', 'fields', 'allow_cancel']),
  'ui:prompt_choices': new Set(['title', 'message', 'multiple', 'options', 'default', 'min_selections', 'max_selections', 'allow_cancel']),
  'ui:prompt_mixed_form': new Set(['title', 'message', 'fields', 'choice', 'allow_cancel']),
  'agent:recommend_agent_profile': new Set(['requirement']),
  'agent:list_available_skills': new Set([]),
  'agent:create_memory_agent': new Set(['name', 'role_definition', 'description', 'category', 'enabled', 'plugin_sources', 'skill_ids', 'default_skill_ids', 'skills']),
  'agent:update_memory_agent': new Set(['agent_id', 'name', 'role_definition', 'description', 'category', 'enabled', 'plugin_sources', 'skill_ids', 'default_skill_ids', 'skills']),
  'agent:preview_agent_context': new Set(['role_definition', 'skills', 'plugin_sources', 'skill_ids']),
  'memory:get_command_detail': new Set(['command_ref']),
  'memory:get_plugin_detail': new Set(['plugin_ref']),
  'memory:get_skill_detail': new Set(['skill_ref']),
};

export const shouldHideArgumentKey = (rawToolName: string | undefined, key: string): boolean => {
  if (GENERIC_HIDDEN_ARGUMENT_KEYS.has(key)) {
    return true;
  }

  if (!rawToolName) {
    return false;
  }

  const displayName = getToolDisplayName(rawToolName);
  const routingKey = resolveToolRoutingKey(rawToolName, displayName);
  const allowlist = TOOL_ARGUMENT_ALLOWLIST[routingKey];
  if (allowlist) {
    return !allowlist.has(key);
  }

  return false;
};
