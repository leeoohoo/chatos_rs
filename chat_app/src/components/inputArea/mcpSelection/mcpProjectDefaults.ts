import type { StoredMcpProjectDefaultMap } from './mcpSelectionTypes';

const MCP_PROJECT_DEFAULTS_STORAGE_KEY = 'chatos_mcp_project_defaults_v1';

export const normalizeProjectScopeKey = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const normalizeEnabledMcpIdList = (value: unknown): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: string[] = [];
  for (const item of value) {
    if (typeof item !== 'string') {
      continue;
    }
    const trimmed = item.trim();
    if (!trimmed || out.includes(trimmed)) {
      continue;
    }
    out.push(trimmed);
  }
  return out;
};

export const readProjectDefaultMap = (): StoredMcpProjectDefaultMap => {
  if (typeof window === 'undefined' || !window.localStorage) {
    return {};
  }
  try {
    const raw = window.localStorage.getItem(MCP_PROJECT_DEFAULTS_STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {};
    }

    const out: StoredMcpProjectDefaultMap = {};
    for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
      const normalizedKey = normalizeProjectScopeKey(key);
      if (!normalizedKey || !value || typeof value !== 'object' || Array.isArray(value)) {
        continue;
      }
      const entry = value as Record<string, unknown>;
      out[normalizedKey] = {
        mcpEnabled: entry.mcpEnabled !== false,
        enabledMcpIds: normalizeEnabledMcpIdList(entry.enabledMcpIds),
        updatedAt: typeof entry.updatedAt === 'string' ? entry.updatedAt : '',
      };
    }
    return out;
  } catch {
    return {};
  }
};

export const writeProjectDefaultMap = (value: StoredMcpProjectDefaultMap): void => {
  if (typeof window === 'undefined' || !window.localStorage) {
    return;
  }
  try {
    window.localStorage.setItem(MCP_PROJECT_DEFAULTS_STORAGE_KEY, JSON.stringify(value));
  } catch {
    // ignore localStorage write errors
  }
};
