import type {
  ProjectRunConfigFileSummaryResponse,
  ProjectRunCustomToolchainResponse,
  ProjectRunEnvironmentResponse,
  ProjectRunValidationIssueResponse,
  GitStatusResponse,
  GitSummaryResponse,
  ProjectChangeMarkResponse,
  ProjectChangeSummaryResponse,
  ProjectRunCatalogResponse,
  ProjectRunTargetResponse,
  ProjectRunToolchainOptionResponse,
} from '../api/client/types';
import type {
  GitStatusResult,
  ProjectChangeMark,
  ProjectChangeSummary,
  ProjectRunConfigFileSummary,
  ProjectRunCustomToolchain,
  ProjectRunCatalog,
  ProjectRunEnvironment,
  ProjectRunState,
  ProjectRunTarget,
  ProjectRunToolchainOption,
  ProjectRunValidationIssue,
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
import { normalizeTerminal } from './terminals';

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
    language: readStringFirst(record, ['language']) || null,
    cwd: String(readValue(record, 'cwd') || ''),
    command,
    source: String(readValue(record, 'source') || 'auto'),
    confidence: readNumberFirst(record, ['confidence']),
    isDefault: readBooleanFirst(record, ['is_default', 'isDefault']),
    entrypoint: readStringFirst(record, ['entrypoint', 'entry_point']) || null,
    manifestPath: readStringFirst(record, ['manifest_path', 'manifestPath']) || null,
    requiredToolchains: Array.isArray(readFirst(record, ['required_toolchains', 'requiredToolchains']))
      ? (readFirst(record, ['required_toolchains', 'requiredToolchains']) as unknown[])
        .map((item) => String(item || '').trim())
        .filter(Boolean)
      : [],
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

export const normalizeProjectRunToolchainOption = (
  raw: ProjectRunToolchainOptionResponse | unknown,
): ProjectRunToolchainOption => {
  const record = asRecord(raw);
  return {
    id: String(readValue(record, 'id') || ''),
    kind: String(readValue(record, 'kind') || ''),
    label: String(readValue(record, 'label') || readValue(record, 'path') || ''),
    version: readStringFirst(record, ['version']) || null,
    path: String(readValue(record, 'path') || ''),
    source: String(readValue(record, 'source') || 'auto'),
    isDefault: readBooleanFirst(record, ['is_default', 'isDefault']),
  };
};

export const normalizeProjectRunCustomToolchain = (
  raw: ProjectRunCustomToolchainResponse | unknown,
): ProjectRunCustomToolchain => {
  const record = asRecord(raw);
  return {
    kind: String(readValue(record, 'kind') || ''),
    label: String(readValue(record, 'label') || ''),
    path: String(readValue(record, 'path') || ''),
  };
};

export const normalizeProjectRunConfigFileSummary = (
  raw: ProjectRunConfigFileSummaryResponse | unknown,
): ProjectRunConfigFileSummary => {
  const record = asRecord(raw);
  return {
    kind: String(readValue(record, 'kind') || ''),
    label: String(readValue(record, 'label') || ''),
    path: String(readValue(record, 'path') || ''),
    preview: readStringFirst(record, ['preview']) || null,
    source: String(readValue(record, 'source') || 'project-local'),
  };
};

export const normalizeProjectRunValidationIssue = (
  raw: ProjectRunValidationIssueResponse | unknown,
): ProjectRunValidationIssue => {
  const record = asRecord(raw);
  return {
    kind: String(readValue(record, 'kind') || ''),
    message: String(readValue(record, 'message') || ''),
    targetId: readStringFirst(record, ['target_id', 'targetId']) || null,
    targetLabel: readStringFirst(record, ['target_label', 'targetLabel']) || null,
    path: readStringFirst(record, ['path']) || null,
    hint: readStringFirst(record, ['hint']) || null,
  };
};

export const normalizeProjectRunEnvironment = (
  raw: ProjectRunEnvironmentResponse | unknown,
): ProjectRunEnvironment => {
  const record = asRecord(raw);
  const rawOptions = asRecord(readFirst(record, ['options_by_kind', 'optionsByKind'])) || {};
  const optionsByKind = Object.fromEntries(
    Object.entries(rawOptions).map(([kind, value]) => [
      kind,
      Array.isArray(value)
        ? value
          .map(normalizeProjectRunToolchainOption)
          .filter((item) => item.id && item.path)
        : [],
    ]),
  );

  const selectedToolchains = asRecord(readFirst(record, ['selected_toolchains', 'selectedToolchains'])) || {};
  const customToolchains = asRecord(readFirst(record, ['custom_toolchains', 'customToolchains'])) || {};
  const envVars = asRecord(readFirst(record, ['env_vars', 'envVars'])) || {};
  const rawConfigFiles = readFirst(record, ['config_files', 'configFiles']);
  const rawValidationIssues = readFirst(record, ['validation_issues', 'validationIssues']);

  return {
    projectId: String(readFirst(record, ['project_id', 'projectId']) ?? ''),
    userId: readStringFirst(record, ['user_id', 'userId']) || null,
    optionsByKind,
    configFiles: Array.isArray(rawConfigFiles)
      ? rawConfigFiles
        .map(normalizeProjectRunConfigFileSummary)
        .filter((item) => item.kind && item.path)
      : [],
    validationIssues: Array.isArray(rawValidationIssues)
      ? rawValidationIssues
        .map(normalizeProjectRunValidationIssue)
        .filter((item) => item.kind && item.message)
      : [],
    selectedToolchains: Object.fromEntries(
      Object.entries(selectedToolchains)
        .map(([key, value]) => [key, String(value || '').trim()])
        .filter(([, value]) => Boolean(value)),
    ),
    customToolchains: (() => {
      const out: Record<string, ProjectRunCustomToolchain> = {};
      Object.entries(customToolchains as Record<string, unknown>).forEach(([key, value]) => {
        const normalized = normalizeProjectRunCustomToolchain(value);
        const normalizedKind = normalized.kind.trim() || key.trim();
        if (!normalizedKind || !normalized.path.trim()) {
          return;
        }
        out[normalizedKind] = {
          ...normalized,
          kind: normalizedKind,
        };
      });
      return out;
    })(),
    envVars: Object.fromEntries(
      Object.entries(envVars)
        .map(([key, value]) => [key, String(value ?? '')]),
    ),
    updatedAt: readStringFirst(record, ['updated_at', 'updatedAt']) || null,
  };
};

export const normalizeProjectRunState = (
  raw: import('../api/client/types').ProjectRunStateResponse | unknown,
): ProjectRunState => {
  const record = asRecord(raw);
  const terminalRaw = readValue(record, 'terminal');
  const terminal = terminalRaw ? normalizeTerminal(terminalRaw) : null;
  const rawInstances = readValue(record, 'instances');
  const instances = Array.isArray(rawInstances)
    ? rawInstances
      .map((item) => {
        const entry = asRecord(item);
        const terminalValue = readValue(entry, 'terminal');
        const normalizedTerminal = terminalValue ? normalizeTerminal(terminalValue) : null;
        const terminalId = readStringFirst(entry, ['terminal_id', 'terminalId']) || normalizedTerminal?.id || '';
        if (!terminalId) {
          return null;
        }
        return {
          terminalId,
          terminalName: readStringFirst(entry, ['terminal_name', 'terminalName']) || normalizedTerminal?.name || terminalId,
          cwd: readString(entry, 'cwd') || normalizedTerminal?.cwd || '',
          status: readString(entry, 'status') || normalizedTerminal?.status || 'idle',
          busy: readBooleanFirst(entry, ['busy']),
          running: readBooleanFirst(entry, ['running']),
          terminal: normalizedTerminal,
        };
      })
      .filter((item): item is NonNullable<typeof item> => Boolean(item))
    : [];
  return {
    projectId: readStringFirst(record, ['project_id', 'projectId']),
    running: readBooleanFirst(record, ['running']),
    busy: readBooleanFirst(record, ['busy']),
    status: readString(record, 'status') || 'idle',
    terminalId: readStringFirst(record, ['terminal_id', 'terminalId']) || terminal?.id || null,
    terminalName: readStringFirst(record, ['terminal_name', 'terminalName']) || terminal?.name || null,
    cwd: readString(record, 'cwd') || terminal?.cwd || null,
    terminal,
    instances,
  };
};
