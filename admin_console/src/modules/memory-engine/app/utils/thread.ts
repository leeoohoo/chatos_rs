// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineJobRun, EngineThread } from '../../types';

import { isObjectRecord, textOrUndefined } from './common';

export function threadScopeKey(
  thread?: Pick<EngineThread, 'id' | 'tenant_id' | 'source_id'> | null,
): string | null {
  if (!thread?.id) {
    return null;
  }
  return [thread.tenant_id ?? '', thread.source_id ?? '', thread.id].join('::');
}

export function jobRunThreadLookupKey(run: {
  thread_id?: string | null;
  tenant_id?: string | null;
  source_id?: string | null;
}): string | null {
  if (!run.thread_id) {
    return null;
  }
  return [run.tenant_id ?? '', run.source_id ?? '', run.thread_id].join('::');
}

export function threadDisplayName(
  thread?: Pick<EngineThread, 'id' | 'title' | 'subject_id'> | null,
): string {
  return (
    textOrUndefined(thread?.title) ??
    textOrUndefined(thread?.subject_id) ??
    thread?.id ??
    '-'
  );
}

function firstAgentLabel(labels?: string[] | null): string | undefined {
  return labels
    ?.map((label) => textOrUndefined(label))
    .find((label): label is string => Boolean(label?.startsWith('agent:')));
}

function objectStringValue(
  record: Record<string, unknown> | undefined,
  key: string,
): string | undefined {
  const value = record?.[key];
  return typeof value === 'string' ? textOrUndefined(value) : undefined;
}

export function threadMemorySubjectId(
  thread?: Pick<EngineThread, 'subject_id' | 'labels' | 'metadata'> | null,
): string | null {
  const subjectId = textOrUndefined(thread?.subject_id);
  if (subjectId?.startsWith('agent:')) {
    return subjectId;
  }

  const labelSubjectId = firstAgentLabel(thread?.labels);
  if (labelSubjectId) {
    return labelSubjectId;
  }

  const metadata = isObjectRecord(thread?.metadata) ? thread.metadata : undefined;
  const legacySessionMapping = isObjectRecord(metadata?.legacy_session_mapping)
    ? metadata.legacy_session_mapping
    : undefined;
  const agentId =
    objectStringValue(legacySessionMapping, 'agent_id') ?? objectStringValue(metadata, 'agent_id');
  if (agentId) {
    return `agent:${agentId}`;
  }

  return subjectId ?? null;
}

export function fallbackThreadDisplayName(
  run: Pick<EngineJobRun, 'thread_id' | 'thread_label' | 'subject_id'>,
): string {
  const subjectName =
    textOrUndefined(run.subject_id)?.replace(/^session:/, '') ??
    textOrUndefined(run.thread_label);
  return subjectName ?? run.thread_id ?? '-';
}
