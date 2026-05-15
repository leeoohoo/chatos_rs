import type {
  GitStatusResponse,
  GitSummaryResponse,
  ProjectChangeMarkResponse,
  ProjectChangeSummaryResponse,
  ProjectRunCatalogResponse,
  ProjectRunTargetResponse,
} from '../api/client/types';
import type {
  GitStatusResult,
  ProjectChangeMark,
  ProjectChangeSummary,
  ProjectRunCatalog,
  ProjectRunTarget,
} from '../../types';
import {
  asRecord,
  readBooleanFirst,
  readFirst,
  readNumberFirst,
  readString,
  readStringFirst,
  readValue,
} from './normalizerUtils';
import { normalizeGitStatus, normalizeGitSummary } from './git';

export type ChangeKind = 'create' | 'edit' | 'delete';

export const normalizeChangeKind = (value: unknown): ChangeKind => {
  const kind = String(value ?? '').trim().toLowerCase();
  if (kind === 'create') return 'create';
  if (kind === 'delete') return 'delete';
  return 'edit';
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

const GIT_IGNORED_CHANGE_DIRS = new Set([
  'build',
  'coverage',
  'dist',
  'node_model',
  'node_modules',
  'out',
  'target',
  '.cache',
  '.next',
  '.nuxt',
  '.turbo',
]);

const normalizeWorkspacePath = (value: string): string => {
  const normalized = value.trim().replace(/\\/g, '/').replace(/\/+/g, '/');
  if (!normalized) return '';
  if (normalized === '/') return '/';
  return normalized.endsWith('/') ? normalized.slice(0, -1) : normalized;
};

const joinWorkspacePath = (base: string, relativePath: string): string => {
  const normalizedBase = normalizeWorkspacePath(base);
  const normalizedRelative = relativePath
    .trim()
    .replace(/\\/g, '/')
    .replace(/^\.\/+/, '')
    .replace(/^\/+/, '');
  if (!normalizedRelative) return normalizedBase;
  if (!normalizedBase || normalizedBase === '/') {
    return normalizeWorkspacePath(`/${normalizedRelative}`);
  }
  return normalizeWorkspacePath(`${normalizedBase}/${normalizedRelative}`);
};

const toProjectRelativePath = (absolutePath: string, projectRootPath: string): string | null => {
  const normalizedAbsolute = normalizeWorkspacePath(absolutePath);
  const normalizedProjectRoot = normalizeWorkspacePath(projectRootPath);
  if (!normalizedAbsolute || !normalizedProjectRoot) {
    return null;
  }
  if (normalizedAbsolute === normalizedProjectRoot) {
    return '';
  }
  const prefix = `${normalizedProjectRoot}/`;
  if (!normalizedAbsolute.startsWith(prefix)) {
    return null;
  }
  return normalizedAbsolute.slice(prefix.length);
};

const shouldIgnoreGitRelativePath = (relativePath: string): boolean => (
  relativePath
    .split('/')
    .some((segment) => segment.length > 0 && GIT_IGNORED_CHANGE_DIRS.has(segment))
);

const readGitChangeKind = (
  file: Pick<GitStatusResult['files'][number], 'status'>,
  currentRelativePath: string | null,
  oldRelativePath: string | null,
): ChangeKind => {
  if (file.status === 'deleted') {
    return 'delete';
  }
  if (file.status === 'added' || file.status === 'untracked') {
    return 'create';
  }
  if ((file.status === 'renamed' || file.status === 'copied') && !oldRelativePath && currentRelativePath) {
    return 'create';
  }
  if (file.status === 'renamed' && oldRelativePath && !currentRelativePath) {
    return 'delete';
  }
  return 'edit';
};

const incrementChangeCount = (
  counts: ProjectChangeSummary['counts'],
  kind: ChangeKind,
) => {
  if (kind === 'create') {
    counts.create += 1;
    return;
  }
  if (kind === 'delete') {
    counts.delete += 1;
    return;
  }
  counts.edit += 1;
};

export const buildProjectChangeSummaryFromGitStatus = (
  rawSummary: GitSummaryResponse | unknown,
  rawStatus: GitStatusResponse | unknown,
  projectRootPath: string,
): ProjectChangeSummary => {
  const summary = normalizeGitSummary(rawSummary as GitSummaryResponse);
  const status = normalizeGitStatus(rawStatus as GitStatusResponse);
  const repoRoot = normalizeWorkspacePath(
    summary.root || summary.worktreeRoot || projectRootPath,
  );
  const normalizedProjectRoot = normalizeWorkspacePath(projectRootPath);
  if (!summary.isRepo || !repoRoot || !normalizedProjectRoot) {
    return EMPTY_CHANGE_SUMMARY;
  }

  const fileMarks: ProjectChangeSummary['fileMarks'] = [];
  const deletedMarks: ProjectChangeSummary['deletedMarks'] = [];
  const counts: ProjectChangeSummary['counts'] = {
    create: 0,
    edit: 0,
    delete: 0,
    total: 0,
  };

  for (const file of status.files) {
    const currentAbsolutePath = file.path ? joinWorkspacePath(repoRoot, file.path) : '';
    const currentRelativePath = toProjectRelativePath(currentAbsolutePath, normalizedProjectRoot);
    const oldAbsolutePath = file.oldPath ? joinWorkspacePath(repoRoot, file.oldPath) : '';
    const oldRelativePath = toProjectRelativePath(oldAbsolutePath, normalizedProjectRoot);

    let targetPath = currentAbsolutePath;
    let relativePath = currentRelativePath;
    if (!relativePath && file.status === 'renamed' && oldRelativePath) {
      targetPath = oldAbsolutePath;
      relativePath = oldRelativePath;
    }
    if (relativePath == null || shouldIgnoreGitRelativePath(relativePath)) {
      continue;
    }

    const kind = readGitChangeKind(file, currentRelativePath, oldRelativePath);
    const mark: ProjectChangeMark = {
      path: targetPath,
      relativePath,
      kind,
      lastChangeId: `git:${file.status}:${relativePath}:${file.staged ? '1' : '0'}:${file.unstaged ? '1' : '0'}:${file.conflicted ? '1' : '0'}`,
      updatedAt: '',
    };

    if (kind === 'delete') {
      deletedMarks.push(mark);
    } else {
      fileMarks.push(mark);
    }
    incrementChangeCount(counts, kind);
  }

  fileMarks.sort((left, right) => left.path.localeCompare(right.path));
  deletedMarks.sort((left, right) => left.path.localeCompare(right.path));
  counts.total = counts.create + counts.edit + counts.delete;

  return {
    fileMarks,
    deletedMarks,
    counts,
  };
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
