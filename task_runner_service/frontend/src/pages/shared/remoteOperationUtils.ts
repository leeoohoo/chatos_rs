// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type RemoteOperationStatItem = {
  connectionId?: string;
  success: boolean;
};

export type RemoteOperationStats = {
  total: number;
  serverCount: number;
  successCount: number;
  failedCount: number;
};

const remoteToolNames = new Set([
  'list_connections',
  'test_connection',
  'run_command',
  'list_directory',
  'read_file',
]);

export function isRemoteToolName(name: string): boolean {
  return remoteToolNames.has(name);
}

export function payloadAsRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

export function payloadAsOptionalString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const text = value.trim();
  return text ? text : undefined;
}

export function payloadAsOptionalNumber(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  return undefined;
}

export function payloadAsOptionalBoolean(value: unknown): boolean | undefined {
  if (typeof value === 'boolean') {
    return value;
  }
  return undefined;
}

export function summarizeRemoteOperationStats<T extends RemoteOperationStatItem>(
  items: T[],
): RemoteOperationStats {
  const serverIds = new Set(items.map((item) => item.connectionId).filter(Boolean));
  const successCount = items.filter((item) => item.success).length;
  return {
    total: items.length,
    serverCount: serverIds.size,
    successCount,
    failedCount: items.length - successCount,
  };
}

export function formatRemoteEndpoint(
  username?: string,
  host?: string,
  port?: number,
): string | undefined {
  if (!host) {
    return undefined;
  }
  const userPrefix = username ? `${username}@` : '';
  const portSuffix = port ? `:${port}` : '';
  return `${userPrefix}${host}${portSuffix}`;
}
