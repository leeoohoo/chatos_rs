const CODE_MAINTAINER_TOOL_NAMES = new Set([
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

const TOOL_NAME_PREFIXES = [
  'builtin_code_maintainer_read_',
  'builtin_code_maintainer_write_',
  'builtin_code_maintainer_',
  'code_maintainer_read_',
  'code_maintainer_write_',
];

const GENERIC_BUILTIN_SERVER_PREFIXES = [
  'builtin_browser_tools_',
  'browser_tools_',
  'builtin_web_tools_',
  'web_tools_',
  'builtin_terminal_controller_',
  'terminal_controller_',
  'builtin_remote_connection_controller_',
  'remote_connection_controller_',
  'builtin_notepad_',
  'notepad_',
  'builtin_task_manager_',
  'task_manager_',
  'builtin_agent_builder_',
  'agent_builder_',
  'builtin_ui_prompter_',
  'ui_prompter_',
  'builtin_memory_command_reader_',
  'memory_command_reader_',
  'builtin_memory_plugin_reader_',
  'memory_plugin_reader_',
  'builtin_memory_skill_reader_',
  'memory_skill_reader_',
];

const normalizeCodeMaintainerCandidate = (candidate: string): string => {
  let normalized = candidate.trim();

  while (normalized.startsWith('builtin__')) {
    normalized = normalized.slice('builtin__'.length).trim();
  }

  while (normalized.startsWith('builtin_')) {
    normalized = normalized.slice('builtin_'.length).trim();
  }

  return normalized;
};

const normalizeBuiltinToolCandidate = (candidate: string): string => {
  let normalized = candidate.trim();

  while (normalized.startsWith('builtin__')) {
    normalized = normalized.slice('builtin__'.length).trim();
  }

  while (normalized.startsWith('builtin_')) {
    normalized = normalized.slice('builtin_'.length).trim();
  }

  return normalized;
};

export const getToolDisplayName = (rawName: string): string => {
  const normalized = rawName.trim();
  if (!normalized) {
    return 'unknown_tool';
  }

  for (const prefix of TOOL_NAME_PREFIXES) {
    if (!normalized.startsWith(prefix)) {
      continue;
    }
    const candidate = normalizeCodeMaintainerCandidate(
      normalized.slice(prefix.length).trim(),
    );
    if (candidate) {
      return candidate;
    }
  }

  if (normalized.startsWith('code_maintainer_')) {
    const candidate = normalizeCodeMaintainerCandidate(
      normalized.slice('code_maintainer_'.length).trim(),
    );
    if (CODE_MAINTAINER_TOOL_NAMES.has(candidate)) {
      return candidate;
    }
  }

  for (const prefix of GENERIC_BUILTIN_SERVER_PREFIXES) {
    if (!normalized.startsWith(prefix)) {
      continue;
    }

    const candidate = normalizeBuiltinToolCandidate(
      normalized.slice(prefix.length).trim(),
    );

    if (candidate) {
      return candidate;
    }
  }

  return normalized;
};
