import hljs from 'highlight.js';

import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbol,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
  ChangeLogItem,
  FsEntry,
  FsReadResult,
  ProjectChangeMark,
  ProjectChangeSummary,
  ProjectSearchHit,
  ProjectRunCatalog,
  ProjectRunTarget,
} from '../../types';

export type ChangeKind = 'create' | 'edit' | 'delete';

export const normalizeEntry = (raw: any): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

export const normalizeFile = (raw: any): FsReadResult => ({
  path: raw?.path ?? '',
  name: raw?.name ?? '',
  size: raw?.size ?? 0,
  contentType: raw?.content_type ?? raw?.contentType ?? 'application/octet-stream',
  isBinary: raw?.is_binary ?? raw?.isBinary ?? false,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
  content: raw?.content ?? '',
});

export const normalizeProjectSearchHit = (raw: any): ProjectSearchHit => ({
  path: raw?.path ?? '',
  relativePath: raw?.relative_path ?? raw?.relativePath ?? raw?.path ?? '',
  line: Number.isFinite(Number(raw?.line)) ? Number(raw?.line) : 1,
  column: Number.isFinite(Number(raw?.column)) ? Number(raw?.column) : 1,
  text: raw?.text ?? '',
});

export const buildProjectSearchHitId = (hit: ProjectSearchHit): string => (
  `${hit.path}:${hit.line}:${hit.column}`
);

export interface TextMatchSegment {
  text: string;
  matched: boolean;
}

interface SplitTextByQueryOptions {
  caseSensitive?: boolean;
  wholeWord?: boolean;
}

const escapeForRegex = (value: string): string => (
  value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
);

export const splitTextByQuery = (
  text: string,
  query: string,
  options?: SplitTextByQueryOptions,
): TextMatchSegment[] => {
  if (!text) {
    return [{ text: '', matched: false }];
  }

  const keyword = query.trim();
  if (!keyword) {
    return [{ text, matched: false }];
  }

  const segments: TextMatchSegment[] = [];
  const pattern = options?.wholeWord
    ? `\\b${escapeForRegex(keyword)}\\b`
    : escapeForRegex(keyword);
  const flags = options?.caseSensitive ? 'gu' : 'giu';
  const matcher = new RegExp(pattern, flags);

  let cursor = 0;
  let hasMatch = false;
  for (const match of text.matchAll(matcher)) {
    const matchedText = match[0] ?? '';
    const startIndex = match.index ?? -1;
    if (startIndex < 0 || !matchedText) {
      continue;
    }
    hasMatch = true;
    if (startIndex > cursor) {
      segments.push({
        text: text.slice(cursor, startIndex),
        matched: false,
      });
    }
    segments.push({
      text: matchedText,
      matched: true,
    });
    cursor = startIndex + matchedText.length;
  }

  if (!hasMatch) {
    return [{ text, matched: false }];
  }

  if (cursor < text.length) {
    segments.push({
      text: text.slice(cursor),
      matched: false,
    });
  }

  return segments;
};

export const normalizeCodeNavCapabilities = (raw: any): CodeNavCapabilities => ({
  language: raw?.language ?? 'unknown',
  provider: raw?.provider ?? 'fallback',
  supportsDefinition: Boolean(raw?.supports_definition ?? raw?.supportsDefinition),
  supportsReferences: Boolean(raw?.supports_references ?? raw?.supportsReferences),
  supportsDocumentSymbols: Boolean(raw?.supports_document_symbols ?? raw?.supportsDocumentSymbols),
  fallbackAvailable: Boolean(raw?.fallback_available ?? raw?.fallbackAvailable ?? true),
});

export const normalizeCodeNavLocation = (raw: any): CodeNavLocation => ({
  path: raw?.path ?? '',
  relativePath: raw?.relative_path ?? raw?.relativePath ?? raw?.path ?? '',
  line: Number.isFinite(Number(raw?.line)) ? Number(raw?.line) : 1,
  column: Number.isFinite(Number(raw?.column)) ? Number(raw?.column) : 1,
  endLine: Number.isFinite(Number(raw?.end_line ?? raw?.endLine))
    ? Number(raw?.end_line ?? raw?.endLine)
    : Number.isFinite(Number(raw?.line)) ? Number(raw?.line) : 1,
  endColumn: Number.isFinite(Number(raw?.end_column ?? raw?.endColumn))
    ? Number(raw?.end_column ?? raw?.endColumn)
    : Number.isFinite(Number(raw?.column)) ? Number(raw?.column) : 1,
  preview: raw?.preview ?? '',
  score: Number.isFinite(Number(raw?.score)) ? Number(raw?.score) : 0,
});

export const normalizeCodeNavLocationsResult = (raw: any): CodeNavLocationsResult => ({
  provider: raw?.provider ?? 'fallback',
  language: raw?.language ?? 'unknown',
  mode: raw?.mode ?? 'unknown',
  token: raw?.token ?? null,
  locations: Array.isArray(raw?.locations) ? raw.locations.map(normalizeCodeNavLocation) : [],
});

export const normalizeCodeNavDocumentSymbol = (raw: any): CodeNavDocumentSymbol => ({
  name: raw?.name ?? '',
  kind: raw?.kind ?? 'symbol',
  line: Number.isFinite(Number(raw?.line)) ? Number(raw?.line) : 1,
  column: Number.isFinite(Number(raw?.column)) ? Number(raw?.column) : 1,
  endLine: Number.isFinite(Number(raw?.end_line ?? raw?.endLine))
    ? Number(raw?.end_line ?? raw?.endLine)
    : Number.isFinite(Number(raw?.line)) ? Number(raw?.line) : 1,
  endColumn: Number.isFinite(Number(raw?.end_column ?? raw?.endColumn))
    ? Number(raw?.end_column ?? raw?.endColumn)
    : Number.isFinite(Number(raw?.column)) ? Number(raw?.column) : 1,
});

export const normalizeCodeNavDocumentSymbolsResult = (raw: any): CodeNavDocumentSymbolsResult => ({
  provider: raw?.provider ?? 'fallback',
  language: raw?.language ?? 'unknown',
  mode: raw?.mode ?? 'unknown',
  symbols: Array.isArray(raw?.symbols) ? raw.symbols.map(normalizeCodeNavDocumentSymbol) : [],
});

export const buildCodeNavLocationId = (item: CodeNavLocation): string => (
  `${item.path}:${item.line}:${item.column}:${item.endLine}:${item.endColumn}`
);

export const normalizeChangeLog = (raw: any): ChangeLogItem => ({
  id: raw?.id ?? '',
  serverName: raw?.server_name ?? raw?.serverName ?? '',
  path: raw?.path ?? '',
  action: raw?.action ?? '',
  changeKind: raw?.change_kind ?? raw?.changeKind ?? (raw?.action === 'delete' ? 'delete' : 'edit'),
  bytes: raw?.bytes ?? 0,
  sha256: raw?.sha256 ?? null,
  diff: raw?.diff ?? null,
  sessionId: raw?.conversation_id ?? raw?.conversationId ?? null,
  runId: raw?.run_id ?? raw?.runId ?? null,
  confirmed: Boolean(raw?.confirmed),
  confirmedAt: raw?.confirmed_at ?? raw?.confirmedAt ?? null,
  confirmedBy: raw?.confirmed_by ?? raw?.confirmedBy ?? null,
  createdAt: raw?.created_at ?? raw?.createdAt ?? '',
  sessionTitle: raw?.conversation_title ?? raw?.conversationTitle ?? null,
});

export const normalizeChangeKind = (value: any): ChangeKind => {
  const kind = String(value ?? '').trim().toLowerCase();
  if (kind === 'create') return 'create';
  if (kind === 'delete') return 'delete';
  return 'edit';
};

const normalizeProjectChangeMark = (raw: any): ProjectChangeMark => ({
  path: raw?.path ?? '',
  relativePath: raw?.relative_path ?? raw?.relativePath ?? '',
  kind: normalizeChangeKind(raw?.kind),
  lastChangeId: raw?.last_change_id ?? raw?.lastChangeId ?? '',
  updatedAt: raw?.updated_at ?? raw?.updatedAt ?? '',
});

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

export const normalizeProjectChangeSummary = (raw: any): ProjectChangeSummary => {
  const fileMarks = Array.isArray(raw?.file_marks ?? raw?.fileMarks)
    ? (raw?.file_marks ?? raw?.fileMarks).map(normalizeProjectChangeMark)
    : [];
  const deletedMarks = Array.isArray(raw?.deleted_marks ?? raw?.deletedMarks)
    ? (raw?.deleted_marks ?? raw?.deletedMarks).map(normalizeProjectChangeMark)
    : [];
  const countsRaw = raw?.counts ?? {};
  const create = Number(countsRaw?.create ?? 0);
  const edit = Number(countsRaw?.edit ?? 0);
  const del = Number(countsRaw?.delete ?? 0);
  const total = Number(countsRaw?.total ?? create + edit + del);
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
  right: ProjectChangeSummary
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

export const CHANGE_KIND_COLOR_CLASS: Record<ChangeKind, string> = {
  create: 'bg-emerald-500',
  edit: 'bg-amber-500',
  delete: 'bg-rose-500',
};

export const CHANGE_KIND_TEXT_CLASS: Record<ChangeKind, string> = {
  create: 'text-emerald-600 dark:text-emerald-400',
  edit: 'text-amber-600 dark:text-amber-400',
  delete: 'text-rose-600 dark:text-rose-400',
};

export const CHANGE_KIND_ROW_CLASS: Record<ChangeKind, string> = {
  create: 'border-l-2 border-emerald-500 bg-emerald-500/10',
  edit: 'border-l-2 border-amber-500 bg-amber-500/10',
  delete: 'border-l-2 border-rose-500 bg-rose-500/10',
};

export const CHANGE_KIND_LABEL: Record<ChangeKind, string> = {
  create: '新增',
  edit: '编辑',
  delete: '删除',
};

export const CHANGE_KIND_PRIORITY: Record<ChangeKind, number> = {
  create: 2,
  edit: 1,
  delete: 3,
};

const EXT_LANGUAGE_MAP: Record<string, string> = {
  rs: 'rust',
  toml: 'toml',
  lock: 'toml',
  md: 'markdown',
  txt: 'plaintext',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  xml: 'xml',
  html: 'xml',
  htm: 'xml',
  vue: 'vue',
  svelte: 'svelte',
  astro: 'astro',
  css: 'css',
  scss: 'scss',
  less: 'less',
  js: 'javascript',
  jsx: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  go: 'go',
  java: 'java',
  kt: 'kotlin',
  swift: 'swift',
  c: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  h: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  php: 'php',
  rb: 'ruby',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  ps1: 'powershell',
  bat: 'dos',
  sql: 'sql',
  ini: 'ini',
  conf: 'ini',
  env: 'ini',
  log: 'plaintext',
  gradle: 'gradle',
  properties: 'ini',
  cfg: 'ini',
  proto: 'protobuf',
  graphql: 'graphql',
  dart: 'dart',
  lua: 'lua',
  r: 'r',
  m: 'objectivec',
  mm: 'objectivec',
  scala: 'scala',
  cmake: 'cmake',
  make: 'makefile',
  dockerfile: 'dockerfile',
};

export const getHighlightLanguage = (filename: string): string | null => {
  const lower = filename.toLowerCase();
  if (lower === 'dockerfile') return hljs.getLanguage('dockerfile') ? 'dockerfile' : null;
  if (lower === 'makefile') return hljs.getLanguage('makefile') ? 'makefile' : null;
  if (lower === 'cmakelists.txt') return hljs.getLanguage('cmake') ? 'cmake' : null;
  const parts = lower.split('.');
  if (parts.length < 2) return null;
  const ext = parts[parts.length - 1];
  const lang = EXT_LANGUAGE_MAP[ext];
  if (!lang) return null;
  return hljs.getLanguage(lang) ? lang : null;
};

export const escapeHtml = (value: string) => (
  value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
);

export const isValidEntryName = (name: string): boolean => (
  name !== '.' &&
  name !== '..' &&
  !name.includes('/') &&
  !name.includes('\\') &&
  !name.includes('\0')
);

export const normalizeProjectRunTarget = (raw: any): ProjectRunTarget => ({
  id: String(raw?.id || ''),
  label: String(raw?.label || raw?.command || ''),
  kind: String(raw?.kind || 'custom'),
  cwd: String(raw?.cwd || ''),
  command: String(raw?.command || ''),
  source: String(raw?.source || 'auto'),
  confidence: Number.isFinite(Number(raw?.confidence)) ? Number(raw?.confidence) : 0,
  isDefault: Boolean(raw?.is_default ?? raw?.isDefault),
});

export const normalizeProjectRunCatalog = (raw: any): ProjectRunCatalog => {
  const targets = Array.isArray(raw?.targets)
    ? raw.targets.map(normalizeProjectRunTarget).filter((item: ProjectRunTarget) => item.id && item.command && item.cwd)
    : [];
  const defaultTargetId = raw?.default_target_id ?? raw?.defaultTargetId ?? null;
  return {
    projectId: String(raw?.project_id ?? raw?.projectId ?? ''),
    status: String(raw?.status ?? (targets.length > 0 ? 'ready' : 'empty')),
    defaultTargetId: defaultTargetId ? String(defaultTargetId) : null,
    targets,
    errorMessage: (raw?.error_message ?? raw?.errorMessage ?? null) || null,
    analyzedAt: (raw?.analyzed_at ?? raw?.analyzedAt ?? null) || null,
    updatedAt: (raw?.updated_at ?? raw?.updatedAt ?? null) || null,
  };
};
