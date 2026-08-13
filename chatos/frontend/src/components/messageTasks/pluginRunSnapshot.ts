// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

type UnknownRecord = Record<string, unknown>;

export interface PluginRunCommandAudit {
  pluginId: string;
  commandId: string;
  argumentsPresent: boolean;
  argumentsSha256: string | null;
}

export interface PluginRunPluginSummary {
  pluginId: string;
  selectedSkillIds: string[];
  selectedCommandIds: string[];
  selectedAgentIds: string[];
}

export interface PluginRunSnapshotSummary {
  plugins: PluginRunPluginSummary[];
  commands: PluginRunCommandAudit[];
}

const record = (value: unknown): UnknownRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

const boundedText = (value: unknown, limit = 256): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const normalized = value.trim();
  return normalized && normalized.length <= limit ? normalized : null;
};

const lowerSha256 = (value: unknown): string | null => (
  typeof value === 'string' && /^[a-f0-9]{64}$/u.test(value) ? value : null
);

const uniqueTextItems = (value: unknown, maxItems = 128): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const seen = new Set<string>();
  const items: string[] = [];
  for (const entry of value.slice(0, maxItems)) {
    const text = boundedText(entry);
    if (text && !seen.has(text)) {
      seen.add(text);
      items.push(text);
    }
  }
  return items;
};

export const pluginRunSnapshotSummary = (
  inputSnapshot: unknown,
): PluginRunSnapshotSummary | null => {
  const snapshot = record(inputSnapshot);
  const config = record(snapshot?.plugin_config);
  const selected = Array.isArray(config?.selected_plugins)
    ? config.selected_plugins.slice(0, 50)
    : [];
  if (selected.length === 0) {
    return null;
  }

  const seenPlugins = new Set<string>();
  const plugins = selected.flatMap((value): PluginRunPluginSummary[] => {
    const plugin = record(value);
    const pluginId = boundedText(plugin?.plugin_id);
    if (!pluginId || seenPlugins.has(pluginId)) {
      return [];
    }
    seenPlugins.add(pluginId);
    return [{
      pluginId,
      selectedSkillIds: uniqueTextItems(plugin?.selected_skill_ids),
      selectedCommandIds: uniqueTextItems(plugin?.selected_command_ids),
      selectedAgentIds: uniqueTextItems(plugin?.selected_agent_ids),
    }];
  });
  if (plugins.length === 0) {
    return null;
  }

  const commands: PluginRunCommandAudit[] = [];
  const seenCommands = new Set<string>();
  if (Array.isArray(config?.command_invocations)) {
    for (const value of config.command_invocations.slice(0, 16)) {
      const command = record(value);
      const pluginId = boundedText(command?.plugin_id);
      const commandId = boundedText(command?.command_id);
      if (!pluginId || !commandId || !seenPlugins.has(pluginId)) {
        continue;
      }
      const key = `${pluginId}\u0000${commandId}`;
      if (seenCommands.has(key)) {
        continue;
      }
      seenCommands.add(key);
      commands.push({
        pluginId,
        commandId,
        argumentsPresent: command?.arguments_present === true
          || (typeof command?.arguments === 'string' && command.arguments.length > 0),
        argumentsSha256: lowerSha256(command?.arguments_sha256),
      });
    }
  }

  return {
    plugins,
    commands,
  };
};
