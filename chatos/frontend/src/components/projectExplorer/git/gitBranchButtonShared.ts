// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';

const statusLabelKey: Record<string, string> = {
  added: 'git.status.added',
  modified: 'git.status.modified',
  deleted: 'git.status.deleted',
  renamed: 'git.status.renamed',
  copied: 'git.status.copied',
  untracked: 'git.status.untracked',
  conflicted: 'git.status.conflicted',
};

const statusTitleKey: Record<string, string> = {
  untracked: 'git.statusTitle.untracked',
  conflicted: 'git.statusTitle.conflicted',
};

const gitClientSourceLabelKey: Record<string, string> = {
  env: 'git.client.env',
  bundled: 'git.client.bundled',
  system: 'git.client.system',
  unknown: 'git.client.unknown',
};

export const getGitStatusLabel = (status: string, t: TranslateFn): string => {
  const key = statusLabelKey[status];
  return key ? t(key) : status;
};

export const getGitStatusTitle = (status: string, t: TranslateFn): string | undefined => {
  const key = statusTitleKey[status];
  return key ? t(key) : undefined;
};

export const getGitClientSourceLabel = (source: string, t: TranslateFn): string => {
  const key = gitClientSourceLabelKey[source];
  return key ? t(key) : source;
};

interface DiffLineView {
  content: string;
  className: string;
}

const formatDiffFilePath = (value: string): string => (
  value.replace(/^[ab]\//, '').trim()
);

const formatDiffHeader = (line: string): string => {
  const match = line.match(/^diff --git\s+a\/(.+?)\s+b\/(.+)$/);
  if (!match) return line;
  const oldPath = formatDiffFilePath(match[1]);
  const newPath = formatDiffFilePath(match[2]);
  return oldPath === newPath ? oldPath : `${oldPath} -> ${newPath}`;
};

const formatDiffHunk = (line: string, t: TranslateFn): string => {
  const match = line.match(/^@@\s+-(\S+)\s+\+(\S+)\s+@@\s*(.*)$/);
  if (!match) return line;
  const oldRange = match[1].replace(/^\+|-/, '');
  const newRange = match[2].replace(/^\+|-/, '');
  const suffix = match[3] ? ` · ${match[3]}` : '';
  return t('git.diff.oldNew', { oldRange, newRange, suffix });
};

export const diffLineView = (line: string, t: TranslateFn): DiffLineView => {
  if (line.startsWith('diff --git')) {
    return {
      content: formatDiffHeader(line),
      className: 'border-l-sky-500 bg-sky-50 text-sky-950 dark:bg-sky-950/35 dark:text-sky-100',
    };
  }
  if (line.startsWith('@@')) {
    return {
      content: formatDiffHunk(line, t),
      className: 'border-l-amber-500 bg-amber-50 text-amber-950 dark:bg-amber-950/35 dark:text-amber-100',
    };
  }
  if (line.startsWith('+++')) {
    return {
      content: formatDiffFilePath(line.replace(/^\+\+\+\s*/, '')),
      className: 'border-l-muted-foreground/50 bg-muted/60 text-muted-foreground',
    };
  }
  if (line.startsWith('---')) {
    return {
      content: formatDiffFilePath(line.replace(/^---\s*/, '')),
      className: 'border-l-muted-foreground/50 bg-muted/60 text-muted-foreground',
    };
  }
  if (line.startsWith('+')) {
    return {
      content: line.slice(1) || ' ',
      className: 'border-l-emerald-500 bg-emerald-100/80 text-emerald-950 dark:bg-emerald-950/45 dark:text-emerald-50',
    };
  }
  if (line.startsWith('-')) {
    return {
      content: line.slice(1) || ' ',
      className: 'border-l-rose-500 bg-rose-100/80 text-rose-950 dark:bg-rose-950/45 dark:text-rose-50',
    };
  }
  return {
    content: line.startsWith(' ') ? line.slice(1) : line || ' ',
    className: cn('border-l-transparent text-foreground'),
  };
};
