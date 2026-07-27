// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface PluginCommandAuditEntryLike {
  plugin_id?: string | null;
  command_id?: string | null;
  arguments_present?: boolean | null;
  arguments_sha256?: string | null;
}

export const normalizePluginCommandAuditEntries = (
  value: unknown,
): PluginCommandAuditEntryLike[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const seen = new Set<string>();
  return value.flatMap((item) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) {
      return [];
    }
    const record = item as Record<string, unknown>;
    const pluginId = typeof record.plugin_id === 'string' ? record.plugin_id.trim() : '';
    const commandId = typeof record.command_id === 'string' ? record.command_id.trim() : '';
    const key = `${pluginId}\u0000${commandId}`;
    if (!pluginId || !commandId || seen.has(key)) {
      return [];
    }
    seen.add(key);
    const hash = typeof record.arguments_sha256 === 'string'
      ? record.arguments_sha256.trim().toLowerCase()
      : '';
    return [{
      plugin_id: pluginId,
      command_id: commandId,
      arguments_present: record.arguments_present === true,
      arguments_sha256: /^[a-f0-9]{64}$/.test(hash) ? hash : null,
    }];
  });
};
