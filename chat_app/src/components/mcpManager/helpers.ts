import type { McpConfig } from '../../types';
import type { DynamicConfigRecord, McpFormData } from './types';

const DEFAULT_FORM_DATA: McpFormData = {
  name: '',
  command: '',
  type: 'stdio',
  cwd: '',
  argsInput: '',
};

export const getDefaultMcpFormData = (): McpFormData => ({
  ...DEFAULT_FORM_DATA,
});

export const parseArgsInput = (value?: string): string[] | undefined => {
  if (!value?.trim()) {
    return undefined;
  }

  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
};

export const getMcpConfigArgsInput = (config: McpConfig): string => {
  const rawArgs = config.args as unknown;

  if (Array.isArray(rawArgs)) {
    return rawArgs.map((item) => String(item)).join(', ');
  }

  if (typeof rawArgs === 'string' && rawArgs.trim() !== '') {
    try {
      const parsed = JSON.parse(rawArgs);
      if (Array.isArray(parsed)) {
        return parsed.map((item) => String(item)).join(', ');
      }
    } catch {
      return rawArgs;
    }

    return rawArgs;
  }

  return '';
};

export const isReadonlyMcpConfig = (config?: McpConfig | null): boolean => {
  if (!config) {
    return false;
  }

  return config.readonly === true || config.builtin === true;
};

export const getMcpDisplayName = (config: McpConfig): string => {
  return config.display_name || config.name;
};

export const normalizeDynamicConfig = (raw: unknown): DynamicConfigRecord => {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return {};
  }

  return Object.fromEntries(
    Object.entries(raw as Record<string, unknown>).map(([key, value]) => {
      if (typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string' || value === null) {
        return [key, value];
      }

      if (Array.isArray(value)) {
        return [key, value.map((item) => String(item))];
      }

      try {
        return [key, JSON.stringify(value)];
      } catch {
        return [key, String(value)];
      }
    }),
  );
};
