// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { McpRecord, RuntimeKind } from '../../types';
import { optionalText, parseJsonArray, parseJsonObject } from '../formUtils';

export const adminRuntimeKinds: RuntimeKind[] = [
  'http',
  'stdio_cloud',
  'local_connector_stdio',
  'local_connector_http',
  'local_connector_builtin_proxy',
];
export const userRuntimeKinds: RuntimeKind[] = ['local_connector_stdio', 'local_connector_http'];

export function buildMcpPayload(values: Record<string, unknown>, isAdmin: boolean) {
  const runtimeKind = values.runtime_kind as RuntimeKind;
  const runtime: Record<string, unknown> = { kind: runtimeKind };

  if (runtimeUsesCommand(runtimeKind)) {
    runtime.command = optionalText(values.command);
    runtime.cwd = optionalText(values.cwd);
    runtime.args = parseJsonArray(values.args_json, []);
    runtime.env = parseJsonObject(values.env_json, {});
  }
  if (runtimeUsesHttp(runtimeKind)) {
    runtime.url = optionalText(values.url);
    runtime.headers = parseJsonObject(values.headers_json, {});
  }
  if (runtimeUsesLocalConnector(runtimeKind)) {
    runtime.local_connector = parseJsonObject(values.local_connector_json, {});
  }

  const payload: Record<string, unknown> = {
    name: optionalText(values.name),
    display_name: optionalText(values.display_name),
    description: optionalText(values.description),
    visibility: values.visibility || 'private',
    enabled: Boolean(values.enabled),
    runtime,
  };
  if (!isAdmin && payload.visibility !== 'private') {
    payload.visibility = 'private';
  }
  return payload;
}

export function isSystemManagedMcp(record: McpRecord): boolean {
  return (
    record.source_kind === 'system_seed' ||
    record.runtime.kind === 'system' ||
    record.runtime.kind === 'builtin'
  );
}

export function runtimeUsesCommand(kind: RuntimeKind | undefined): boolean {
  return kind === 'stdio_cloud' || kind === 'local_connector_stdio';
}

export function runtimeUsesHttp(kind: RuntimeKind | undefined): boolean {
  return kind === 'http' || kind === 'local_connector_http';
}

export function runtimeUsesLocalConnector(kind: RuntimeKind | undefined): boolean {
  return (
    kind === 'local_connector_stdio' ||
    kind === 'local_connector_http' ||
    kind === 'local_connector_builtin_proxy'
  );
}
