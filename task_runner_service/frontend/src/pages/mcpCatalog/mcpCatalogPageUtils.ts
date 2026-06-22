import type {
  CreateExternalMcpConfigPayload,
  ExternalMcpTransport,
  McpServerToolProfileInfo,
} from '../../types';
import type { TranslateFn } from '../../i18n/I18nProvider';

export type ExternalMcpConfigFormValues = {
  name: string;
  transport: ExternalMcpTransport;
  command?: string;
  argsText?: string;
  url?: string;
  headersText?: string;
  envText?: string;
  cwd?: string;
  enabled?: boolean;
};

export const MCP_CARD_STYLE = {
  width: '100%',
  padding: 16,
  borderRadius: 6,
  background: '#fff',
  border: '1px solid #f0f0f0',
};

export const TOOL_PROFILE_COLORS: Record<string, string> = {
  admin_full: 'volcano',
  agent_default: 'blue',
  chatos_async_planner: 'geekblue',
};

export function buildExternalMcpConfigPayload(
  values: ExternalMcpConfigFormValues,
): CreateExternalMcpConfigPayload {
  const transport = values.transport || 'stdio';
  const command = values.command?.trim() || '';
  const url = values.url?.trim() || '';
  const cwd = values.cwd?.trim() || '';
  const base = {
    name: values.name?.trim() || '',
    transport,
    enabled: values.enabled ?? true,
  };
  if (transport === 'http') {
    return {
      ...base,
      command: '',
      args: [],
      url,
      headers: parseStringMapJson(values.headersText, 'Headers JSON'),
      env: {},
      cwd: '',
    };
  }
  return {
    ...base,
    command,
    args: parseLines(values.argsText),
    url: '',
    headers: {},
    cwd,
    env: parseStringMapJson(values.envText, 'Env JSON'),
  };
}

export function profileLabel(
  profile: McpServerToolProfileInfo,
  t: TranslateFn,
): string {
  if (profile.key === 'admin_full') {
    return t('mcpCatalog.profile.adminFull');
  }
  if (profile.key === 'agent_default') {
    return t('mcpCatalog.profile.agentDefault');
  }
  if (profile.key === 'chatos_async_planner') {
    return t('mcpCatalog.profile.chatosAsyncPlanner');
  }
  return profile.label;
}

export function profileDescription(
  profile: McpServerToolProfileInfo,
  t: TranslateFn,
): string {
  if (profile.key === 'admin_full') {
    return t('mcpCatalog.profile.adminFullDescription');
  }
  if (profile.key === 'agent_default') {
    return t('mcpCatalog.profile.agentDefaultDescription');
  }
  if (profile.key === 'chatos_async_planner') {
    return t('mcpCatalog.profile.chatosAsyncPlannerDescription');
  }
  return profile.description;
}

function parseLines(value?: string): string[] {
  return (value || '')
    .split('\n')
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseStringMapJson(value: string | undefined, label: string): Record<string, string> {
  const trimmed = (value || '').trim();
  if (!trimmed) {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    throw new Error(`${label} must be valid JSON`);
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error(`${label} must be a JSON object`);
  }
  return Object.fromEntries(
    Object.entries(parsed as Record<string, unknown>)
      .map(([key, item]) => [key.trim(), String(item).trim()])
      .filter(([key]) => key.length > 0),
  );
}
