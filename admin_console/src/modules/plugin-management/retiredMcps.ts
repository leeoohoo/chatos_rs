// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentMcpBindingsResponse,
  ListResponse,
  McpRecord,
  RuntimeCapabilitiesResponse,
} from './types';

const RETIRED_TASK_MANAGER_IDS = new Set(['builtin_task_manager', 'task_manager']);
const RETIRED_TASK_MANAGER_SERVER_NAME = 'task_manager';
const RETIRED_TASK_MANAGER_SYSTEM_KEY = 'task_manager';
const RETIRED_TASK_MANAGER_KIND = 'taskmanager';

export function isRetiredTaskManagerMcp(record: McpRecord): boolean {
  const systemScope =
    record.visibility === 'system_private' ||
    record.source_kind === 'system_seed' ||
    record.runtime.kind === 'system' ||
    record.runtime.kind === 'builtin';
  if (!systemScope) {
    return false;
  }

  return (
    RETIRED_TASK_MANAGER_IDS.has(normalize(record.id)) ||
    normalize(record.name) === RETIRED_TASK_MANAGER_SERVER_NAME ||
    normalize(record.runtime.server_name) === RETIRED_TASK_MANAGER_SERVER_NAME ||
    normalize(record.runtime.system_key) === RETIRED_TASK_MANAGER_SYSTEM_KEY ||
    normalize(record.runtime.builtin_kind) === RETIRED_TASK_MANAGER_KIND ||
    normalize(record.runtime.builtin_kind) === RETIRED_TASK_MANAGER_SERVER_NAME
  );
}

export function filterRetiredMcps<T extends McpRecord>(records: T[]): T[] {
  return records.filter((record) => !isRetiredTaskManagerMcp(record));
}

export function filterRetiredMcpList<T extends McpRecord>(
  response: ListResponse<T>,
): ListResponse<T> {
  const items = filterRetiredMcps(response.items);
  const removedCount = response.items.length - items.length;
  return {
    ...response,
    items,
    total: Math.max(0, response.total - removedCount),
  };
}

export function filterRetiredAgentMcpBindings(
  response: AgentMcpBindingsResponse,
): AgentMcpBindingsResponse {
  return {
    ...response,
    items: response.items.filter((item) => !isRetiredTaskManagerMcp(item.mcp)),
  };
}

export function filterRetiredRuntimeCapabilities(
  response: RuntimeCapabilitiesResponse,
): RuntimeCapabilitiesResponse {
  return {
    ...response,
    mcps: response.mcps.filter((item) => !isRetiredTaskManagerMcp(item.resource)),
  };
}

export function isRetiredTaskManagerMcpId(id: string): boolean {
  return RETIRED_TASK_MANAGER_IDS.has(normalize(id));
}

function normalize(value: string | null | undefined): string {
  return (value || '').trim().toLowerCase();
}
