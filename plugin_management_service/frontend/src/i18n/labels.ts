// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { McpRecord, SystemAgentRecord } from '../types';

type Translate = (key: string, values?: Record<string, string | number>) => string;

export function mcpDisplayName(record: McpRecord, t: Translate): string {
  const named = t(`mcpName.${record.name}`);
  if (!named.startsWith('mcpName.')) {
    return named;
  }
  const builtinKind = record.runtime.builtin_kind;
  if (!builtinKind) {
    return record.display_name;
  }
  const translated = t(`builtin.${builtinKind}`);
  return translated.startsWith('builtin.') ? record.display_name : translated;
}

export function agentDisplayName(record: SystemAgentRecord, t: Translate): string {
  const translated = t(`agentKey.${record.agent_key}`);
  return translated.startsWith('agentKey.') ? record.display_name : translated;
}

export function runtimeKindLabel(value: string, t: Translate): string {
  return t(`runtimeKind.${value}`);
}

export function contentKindLabel(value: string, t: Translate): string {
  return t(`contentKind.${value}`);
}

export function bindingScopeLabel(value: string, t: Translate): string {
  return t(`binding.${value}`);
}

export function resourceKindLabel(value: string, t: Translate): string {
  return t(`resource.${value}`);
}

export function sourceKindLabel(value: string, t: Translate): string {
  const translated = t(`sourceKind.${value}`);
  return translated.startsWith('sourceKind.') ? value : translated;
}

export function managedByLabel(value: string, t: Translate): string {
  const translated = t(`managed.${value}`);
  return translated.startsWith('managed.') ? value : translated;
}
