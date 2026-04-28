import { cn } from '../../../lib/utils';

export const statusLabel: Record<string, string> = {
  added: '新增',
  modified: '修改',
  deleted: '删除',
  renamed: '重命名',
  copied: '复制',
  untracked: '未跟踪',
  conflicted: '冲突',
};

export const statusTitle: Record<string, string> = {
  untracked: 'Git 还没有纳入版本管理的新文件，Stage 后才会进入本次提交。',
  conflicted: '文件存在合并冲突，需要解决后再提交。',
};

export const gitClientSourceLabel: Record<string, string> = {
  env: '环境变量 Git',
  bundled: '内置 Git',
  system: '系统 Git',
  unknown: '未知 Git',
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

const formatDiffHunk = (line: string): string => {
  const match = line.match(/^@@\s+-(\S+)\s+\+(\S+)\s+@@\s*(.*)$/);
  if (!match) return line;
  const oldRange = match[1].replace(/^\+|-/, '');
  const newRange = match[2].replace(/^\+|-/, '');
  const suffix = match[3] ? ` · ${match[3]}` : '';
  return `旧 ${oldRange} / 新 ${newRange}${suffix}`;
};

export const diffLineView = (line: string): DiffLineView => {
  if (line.startsWith('diff --git')) {
    return {
      content: formatDiffHeader(line),
      className: 'border-l-sky-500 bg-sky-50 text-sky-950 dark:bg-sky-950/35 dark:text-sky-100',
    };
  }
  if (line.startsWith('@@')) {
    return {
      content: formatDiffHunk(line),
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
