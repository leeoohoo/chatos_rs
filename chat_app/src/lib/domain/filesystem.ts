// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FsEntryResponse, FsReadFileResponse } from '../api/client/types';
import type { FsEntry, FsReadResult } from '../../types';
import {
  asRecord,
  readBooleanFirst,
  readFirst,
  readNumberFirst,
  readStringFirst,
} from './normalizerUtils';

export interface NormalizeFsEntryOptions {
  fallbackIsDir?: boolean;
}

export const isValidEntryName = (name: string): boolean => (
  name !== '.'
  && name !== '..'
  && !name.includes('/')
  && !name.includes('\\')
  && !name.includes('\0')
);

export const normalizeFsEntry = (
  raw: FsEntryResponse | unknown,
  options: NormalizeFsEntryOptions = {},
): FsEntry => {
  const record = asRecord(raw);
  return {
    name: readStringFirst(record, ['name']),
    path: readStringFirst(record, ['path']),
    displayPath: (readFirst(record, ['display_path', 'displayPath']) ?? null) as FsEntry['displayPath'],
    isDir: readBooleanFirst(record, ['is_dir', 'isDir'], options.fallbackIsDir ?? false),
    writable: (readFirst(record, ['writable']) ?? null) as FsEntry['writable'],
    size: (readFirst(record, ['size']) ?? null) as FsEntry['size'],
    modifiedAt: (readFirst(record, ['modified_at', 'modifiedAt']) ?? null) as FsEntry['modifiedAt'],
  };
};

export const normalizeFsReadResult = (raw: FsReadFileResponse | unknown): FsReadResult => {
  const record = asRecord(raw);
  return {
    path: readStringFirst(record, ['path']),
    name: readStringFirst(record, ['name']),
    size: readNumberFirst(record, ['size']),
    contentType: readStringFirst(record, ['content_type', 'contentType'], 'application/octet-stream'),
    isBinary: readBooleanFirst(record, ['is_binary', 'isBinary']),
    writable: (readFirst(record, ['writable']) ?? null) as FsReadResult['writable'],
    modifiedAt: (readFirst(record, ['modified_at', 'modifiedAt']) ?? null) as FsReadResult['modifiedAt'],
    content: readStringFirst(record, ['content']),
  };
};

export const deriveParentPath = (path: string): string | null => {
  const trimmed = path.trim();
  if (/^[A-Za-z]:[\\/]?$/.test(trimmed)) {
    return `${trimmed.slice(0, 2)}\\`;
  }
  const normalized = trimmed.replace(/[\\/]+$/, '');
  if (!normalized) return null;
  const idx = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));
  if (idx < 0) return null;
  if (idx === 0) return normalized[0];
  const parent = normalized.slice(0, idx);
  if (/^[A-Za-z]:$/.test(parent)) {
    return `${parent}\\`;
  }
  return parent;
};

const normalizePathSeparators = (value: string): string => value.replace(/\\/g, '/');

const normalizePathForCompare = (value: string): string => {
  const normalized = normalizePathSeparators(value.trim()).replace(/\/+$/, '');
  return normalized || '/';
};

const joinDisplayPath = (prefix: string, relative: string): string => {
  const cleanPrefix = prefix === '/' ? '' : prefix.replace(/\/+$/, '');
  const cleanRelative = relative.replace(/^\/+/, '');
  if (!cleanRelative) {
    return cleanPrefix || '/';
  }
  return `${cleanPrefix}/${cleanRelative}`;
};

const userScopedRootMatch = (path: string): { root: string; kind: 'workspaces' | 'public'; relative: string } | null => {
  const normalized = normalizePathForCompare(path);
  const match = normalized.match(/^(.*\/users\/[^/]+\/(workspaces|public))(?:\/(.*))?$/);
  if (!match) {
    return null;
  }
  return {
    root: match[1],
    kind: match[2] as 'workspaces' | 'public',
    relative: match[3] || '',
  };
};

export const getUserVisiblePath = (
  path: string | null | undefined,
  scopeRoot?: string | null,
): string => {
  const raw = (path || '').trim();
  if (!raw) {
    return '';
  }

  const normalized = normalizePathForCompare(raw);
  const normalizedScopeRoot = scopeRoot ? normalizePathForCompare(scopeRoot) : '';
  if (normalizedScopeRoot && normalized === normalizedScopeRoot) {
    return '/';
  }
  if (normalizedScopeRoot && normalized.startsWith(`${normalizedScopeRoot}/`)) {
    return joinDisplayPath('/', normalized.slice(normalizedScopeRoot.length + 1));
  }

  const scoped = userScopedRootMatch(normalized);
  if (scoped) {
    const prefix = scoped.kind === 'public' ? '/public' : '/';
    return joinDisplayPath(prefix, scoped.relative);
  }

  return raw;
};

export const resolveUserVisiblePathInput = (
  visiblePath: string,
  currentPath: string | null | undefined,
): string => {
  const trimmed = visiblePath.trim();
  const current = (currentPath || '').trim();
  const scoped = userScopedRootMatch(current);
  if (!trimmed || !scoped) {
    return trimmed;
  }

  const normalizedVisible = normalizePathSeparators(trimmed);
  const root = scoped.kind === 'public' ? scoped.root : scoped.root;
  if (scoped.kind === 'public' && normalizedVisible === '/public') {
    return root;
  }
  if (scoped.kind === 'public' && normalizedVisible.startsWith('/public/')) {
    return joinDisplayPath(root, normalizedVisible.slice('/public/'.length));
  }
  return joinDisplayPath(root, normalizedVisible);
};
