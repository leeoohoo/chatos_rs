// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface RawUnavailableToolPayload {
  server_name?: string;
  serverName?: string;
  tool_name?: string;
  toolName?: string;
  reason?: string;
}

export interface UnavailableToolEntry {
  id: string;
  serverName: string;
  toolName: string;
  reason: string;
  createdAt?: string;
}

export const unavailableToolEntryKey = (entry: Pick<UnavailableToolEntry, 'serverName' | 'toolName' | 'reason'>): string => (
  `${entry.serverName}::${entry.toolName}::${entry.reason}`
);

export const normalizeUnavailableToolEntry = (
  value: RawUnavailableToolPayload,
  index: number,
): UnavailableToolEntry => {
  const serverName = (
    typeof value.server_name === 'string' && value.server_name.trim().length > 0
      ? value.server_name.trim()
      : (typeof value.serverName === 'string' && value.serverName.trim().length > 0
        ? value.serverName.trim()
        : 'unknown_server')
  );
  const toolName = (
    typeof value.tool_name === 'string' && value.tool_name.trim().length > 0
      ? value.tool_name.trim()
      : (typeof value.toolName === 'string' && value.toolName.trim().length > 0
        ? value.toolName.trim()
        : 'unknown_tool')
  );
  const reason = (
    typeof value.reason === 'string' && value.reason.trim().length > 0
      ? value.reason.trim()
      : '工具当前不可用'
  );

  return {
    id: `unavailable_tool_${Date.now()}_${index}`,
    serverName,
    toolName,
    reason,
    createdAt: new Date().toISOString(),
  };
};

export const extractUnavailableToolsFromPayload = (
  data: unknown,
): RawUnavailableToolPayload[] => {
  const rawUnavailableTools = (
    data && typeof data === 'object' && 'unavailable_tools' in data
      ? (data as { unavailable_tools?: unknown }).unavailable_tools
      : data
  );
  if (Array.isArray(rawUnavailableTools)) {
    return rawUnavailableTools as RawUnavailableToolPayload[];
  }
  return rawUnavailableTools ? [rawUnavailableTools as RawUnavailableToolPayload] : [];
};

export const mergeUnavailableToolEntries = (
  existing: UnavailableToolEntry[],
  data: unknown,
): {
  items: UnavailableToolEntry[];
  addedCount: number;
} => {
  const items = [...existing];
  const existingKeys = new Set(items.map(unavailableToolEntryKey));
  let addedCount = 0;

  extractUnavailableToolsFromPayload(data).forEach((tool, index) => {
    const normalized = normalizeUnavailableToolEntry(tool, index);
    const key = unavailableToolEntryKey(normalized);
    if (existingKeys.has(key)) {
      return;
    }
    items.push(normalized);
    existingKeys.add(key);
    addedCount += 1;
  });

  return { items, addedCount };
};
