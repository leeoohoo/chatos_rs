import { AGENT_BUILDER_MCP_ID } from './mcpSelectionConstants';
import type { SelectableMcpConfig } from './mcpSelectionTypes';

interface McpConfigLike {
  id?: string;
  name?: string;
  display_name?: string;
  enabled?: boolean;
  builtin?: boolean;
}

export const normalizeSelectableMcpConfigs = (rows: unknown): SelectableMcpConfig[] => {
  const seenIds = new Set<string>();
  return (Array.isArray(rows) ? rows : [])
    .map((item) => {
      const candidate = (item && typeof item === 'object' ? item : {}) as McpConfigLike;
      const id = typeof candidate.id === 'string' ? candidate.id.trim() : '';
      if (!id || id === AGENT_BUILDER_MCP_ID) {
        return null;
      }
      if (seenIds.has(id)) {
        return null;
      }
      seenIds.add(id);
      const enabled = typeof candidate.enabled === 'boolean' ? candidate.enabled : true;
      if (!enabled) {
        return null;
      }
      const displayNameRaw = typeof candidate.display_name === 'string' ? candidate.display_name.trim() : '';
      const nameRaw = typeof candidate.name === 'string' ? candidate.name.trim() : '';
      return {
        id,
        name: nameRaw || id,
        displayName: displayNameRaw || nameRaw || id,
        builtin: candidate.builtin === true,
      } satisfies SelectableMcpConfig;
    })
    .filter((item: SelectableMcpConfig | null): item is SelectableMcpConfig => item !== null)
    .sort((left, right) => {
      if (left.builtin !== right.builtin) {
        return left.builtin ? -1 : 1;
      }
      return left.displayName.localeCompare(right.displayName, 'zh-Hans-CN');
    });
};
