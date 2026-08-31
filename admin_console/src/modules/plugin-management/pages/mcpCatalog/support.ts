// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { McpRecord, RuntimeKind } from '../../types';
import { optionalText, parseJsonObject } from '../formUtils';

export const adminRuntimeKinds: RuntimeKind[] = ['http'];

export function buildMcpPayload(values: Record<string, unknown>, isAdmin: boolean) {
  const runtimeKind = values.runtime_kind as RuntimeKind;
  const runtime: Record<string, unknown> = { kind: runtimeKind };

  if (runtimeUsesHttp(runtimeKind)) {
    runtime.url = optionalText(values.url);
    runtime.headers = parseJsonObject(values.headers_json, {});
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

export function runtimeUsesHttp(kind: RuntimeKind | undefined): boolean {
  return kind === 'http';
}
