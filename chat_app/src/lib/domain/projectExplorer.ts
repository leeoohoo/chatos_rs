import type {
  ProjectChangeLogResponse,
  ProjectChangeMarkResponse,
  ProjectChangeSummaryResponse,
  ProjectRunCatalogResponse,
  ProjectRunTargetResponse,
} from '../api/client/types';
import type {
  ChangeLogItem,
  ProjectChangeMark,
  ProjectChangeSummary,
  ProjectRunCatalog,
  ProjectRunTarget,
} from '../../types';
import {
  asRecord,
  readBooleanFirst,
  readFirst,
  readNullableStringFirst,
  readNumberFirst,
  readString,
  readStringFirst,
  readValue,
} from './normalizerUtils';

export type ChangeKind = 'create' | 'edit' | 'delete';

export const normalizeChangeKind = (value: unknown): ChangeKind => {
  const kind = String(value ?? '').trim().toLowerCase();
  if (kind === 'create') return 'create';
  if (kind === 'delete') return 'delete';
  return 'edit';
};

export const normalizeChangeLog = (raw: ProjectChangeLogResponse | unknown): ChangeLogItem => {
  const record = asRecord(raw);
  const action = readString(record, 'action');
  return {
    id: readString(record, 'id'),
    serverName: readStringFirst(record, ['server_name', 'serverName']),
    path: readString(record, 'path'),
    action,
    changeKind: (readFirst(record, ['change_kind', 'changeKind']) ?? (action === 'delete' ? 'delete' : 'edit')) as ChangeLogItem['changeKind'],
    bytes: readNumberFirst(record, ['bytes']),
    sha256: readNullableStringFirst(record, ['sha256']),
    diff: readNullableStringFirst(record, ['diff']),
    sessionId: readNullableStringFirst(record, ['conversation_id', 'conversationId']),
    runId: readNullableStringFirst(record, ['run_id', 'runId']),
    confirmed: readBooleanFirst(record, ['confirmed']),
    confirmedAt: readNullableStringFirst(record, ['confirmed_at', 'confirmedAt']),
    confirmedBy: readNullableStringFirst(record, ['confirmed_by', 'confirmedBy']),
    createdAt: readStringFirst(record, ['created_at', 'createdAt']),
    sessionTitle: readNullableStringFirst(record, ['conversation_title', 'conversationTitle']),
  };
};

const normalizeProjectChangeMark = (raw: ProjectChangeMarkResponse | unknown): ProjectChangeMark => {
  const record = asRecord(raw);
  return {
    path: readString(record, 'path'),
    relativePath: readStringFirst(record, ['relative_path', 'relativePath']),
    kind: normalizeChangeKind(readValue(record, 'kind')),
    lastChangeId: readStringFirst(record, ['last_change_id', 'lastChangeId']),
    updatedAt: readStringFirst(record, ['updated_at', 'updatedAt']),
  };
};

const areChangeMarksEqual = (left: ProjectChangeMark[], right: ProjectChangeMark[]): boolean => {
  if (left.length !== right.length) return false;
  for (let i = 0; i < left.length; i += 1) {
    const a = left[i];
    const b = right[i];
    if (
      a.path !== b.path ||
      a.relativePath !== b.relativePath ||
      a.kind !== b.kind ||
      a.lastChangeId !== b.lastChangeId ||
      a.updatedAt !== b.updatedAt
    ) {
      return false;
    }
  }
  return true;
};

export const EMPTY_CHANGE_SUMMARY: ProjectChangeSummary = {
  fileMarks: [],
  deletedMarks: [],
  counts: {
    create: 0,
    edit: 0,
    delete: 0,
    total: 0,
  },
};

export const normalizeProjectChangeSummary = (raw: ProjectChangeSummaryResponse | unknown): ProjectChangeSummary => {
  const record = asRecord(raw);
  const rawFileMarks = readFirst(record, ['file_marks', 'fileMarks']);
  const rawDeletedMarks = readFirst(record, ['deleted_marks', 'deletedMarks']);
  const fileMarks = Array.isArray(rawFileMarks)
    ? rawFileMarks.map(normalizeProjectChangeMark)
    : [];
  const deletedMarks = Array.isArray(rawDeletedMarks)
    ? rawDeletedMarks.map(normalizeProjectChangeMark)
    : [];
  const countsRaw = asRecord(readValue(record, 'counts')) ?? {};
  const create = Number(readValue(countsRaw, 'create') ?? 0);
  const edit = Number(readValue(countsRaw, 'edit') ?? 0);
  const del = Number(readValue(countsRaw, 'delete') ?? 0);
  const total = Number(readValue(countsRaw, 'total') ?? create + edit + del);
  return {
    fileMarks,
    deletedMarks,
    counts: {
      create: Number.isFinite(create) ? create : 0,
      edit: Number.isFinite(edit) ? edit : 0,
      delete: Number.isFinite(del) ? del : 0,
      total: Number.isFinite(total) ? total : 0,
    },
  };
};

export const isProjectChangeSummaryEqual = (
  left: ProjectChangeSummary,
  right: ProjectChangeSummary,
): boolean => {
  if (
    left.counts.create !== right.counts.create ||
    left.counts.edit !== right.counts.edit ||
    left.counts.delete !== right.counts.delete ||
    left.counts.total !== right.counts.total
  ) {
    return false;
  }
  return (
    areChangeMarksEqual(left.fileMarks, right.fileMarks)
    && areChangeMarksEqual(left.deletedMarks, right.deletedMarks)
  );
};

export const normalizeProjectRunTarget = (raw: ProjectRunTargetResponse | unknown): ProjectRunTarget => {
  const record = asRecord(raw);
  const command = readString(record, 'command');
  return {
    id: String(readValue(record, 'id') || ''),
    label: String(readValue(record, 'label') || command || ''),
    kind: String(readValue(record, 'kind') || 'custom'),
    cwd: String(readValue(record, 'cwd') || ''),
    command,
    source: String(readValue(record, 'source') || 'auto'),
    confidence: readNumberFirst(record, ['confidence']),
    isDefault: readBooleanFirst(record, ['is_default', 'isDefault']),
  };
};

export const normalizeProjectRunCatalog = (raw: ProjectRunCatalogResponse | unknown): ProjectRunCatalog => {
  const record = asRecord(raw);
  const rawTargets = readValue(record, 'targets');
  const targets = Array.isArray(rawTargets)
    ? rawTargets.map(normalizeProjectRunTarget).filter((item: ProjectRunTarget) => item.id && item.command && item.cwd)
    : [];
  const defaultTargetId = readFirst(record, ['default_target_id', 'defaultTargetId']) ?? null;
  return {
    projectId: String(readFirst(record, ['project_id', 'projectId']) ?? ''),
    status: String(readValue(record, 'status') ?? (targets.length > 0 ? 'ready' : 'empty')),
    defaultTargetId: defaultTargetId ? String(defaultTargetId) : null,
    targets,
    errorMessage: (readFirst(record, ['error_message', 'errorMessage']) ?? null) as ProjectRunCatalog['errorMessage'],
    analyzedAt: (readFirst(record, ['analyzed_at', 'analyzedAt']) ?? null) as ProjectRunCatalog['analyzedAt'],
    updatedAt: (readFirst(record, ['updated_at', 'updatedAt']) ?? null) as ProjectRunCatalog['updatedAt'],
  };
};
