// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  TaskRunnerSelectablePluginCommandResponse,
  TaskRunnerSelectablePluginResponse,
} from '../../lib/api/client/types';

export const MAX_PLUGIN_COMMAND_INVOCATIONS = 64;
export const MAX_PLUGIN_COMMAND_ARGUMENT_BYTES = 16 * 1024;

export interface TaskPluginCommandOption {
  key: string;
  plugin: TaskRunnerSelectablePluginResponse;
  command: TaskRunnerSelectablePluginCommandResponse;
}

export interface LeadingPluginCommandDraft {
  query: string;
  arguments: string;
}

export const pluginCommandKey = (pluginId: string, commandId: string): string => (
  `${pluginId}\u0000${commandId}`
);

export const utf8ByteLength = (value: string): number => new TextEncoder().encode(value).length;

export const parseLeadingPluginCommand = (
  message: string,
): LeadingPluginCommandDraft | null => {
  const match = message.match(/^\/([^\s/]*)?(?:\s+([\s\S]*))?$/);
  if (!match) {
    return null;
  }
  return {
    query: String(match[1] || '').trim().toLowerCase(),
    arguments: String(match[2] || ''),
  };
};

export const replaceLeadingPluginCommand = (
  message: string,
  commandId: string,
  addArgumentSpace: boolean,
): string => {
  const parsed = parseLeadingPluginCommand(message);
  const argumentsText = parsed?.arguments || '';
  if (argumentsText.length > 0) {
    return `/${commandId} ${argumentsText}`;
  }
  return `/${commandId}${addArgumentSpace ? ' ' : ''}`;
};

export const pluginCommandOptions = (
  plugins: TaskRunnerSelectablePluginResponse[],
): TaskPluginCommandOption[] => plugins.flatMap((plugin) => (
  (Array.isArray(plugin.commands) ? plugin.commands : []).map((command) => ({
    key: pluginCommandKey(plugin.id, command.command_id),
    plugin,
    command,
  }))
));

export const filterPluginCommandOptions = (
  options: TaskPluginCommandOption[],
  query: string,
): TaskPluginCommandOption[] => {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return options;
  }
  return options.filter(({ plugin, command }) => (
    [
      command.command_id,
      command.display_name,
      command.description,
      command.argument_hint,
      plugin.display_name,
      plugin.plugin_key,
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase()
      .includes(normalized)
  ));
};
