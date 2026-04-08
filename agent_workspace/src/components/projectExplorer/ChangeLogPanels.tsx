import React from 'react';

import type { ChangeLogItem } from '../../types';
import { cn, formatFileSize } from '../../lib/utils';
import {
  CHANGE_KIND_LABEL,
  CHANGE_KIND_TEXT_CLASS,
  normalizeChangeKind,
} from './utils';

type DiffRow = {
  type: 'meta' | 'hunk' | 'add' | 'del' | 'context';
  oldLine?: number | null;
  newLine?: number | null;
  text: string;
};

const parseUnifiedDiff = (diffText: string): DiffRow[] => {
  const lines = diffText.split(/\r?\n/);
  const parsed: DiffRow[] = [];
  let oldLine = 0;
  let newLine = 0;
  let inHunk = false;
  const hunkRegex = /^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/;

  for (const line of lines) {
    const hunkMatch = hunkRegex.exec(line);
    if (hunkMatch) {
      oldLine = parseInt(hunkMatch[1], 10);
      newLine = parseInt(hunkMatch[3], 10);
      inHunk = true;
      parsed.push({ type: 'hunk', text: line });
      continue;
    }
    if (!inHunk) {
      parsed.push({ type: 'meta', text: line });
      continue;
    }
    if (line.startsWith('+++') || line.startsWith('---')) {
      parsed.push({ type: 'meta', text: line });
      continue;
    }
    if (line.startsWith('+')) {
      parsed.push({ type: 'add', oldLine: null, newLine, text: line });
      newLine += 1;
      continue;
    }
    if (line.startsWith('-')) {
      parsed.push({ type: 'del', oldLine, newLine: null, text: line });
      oldLine += 1;
      continue;
    }
    if (line.startsWith('\\')) {
      parsed.push({ type: 'meta', text: line });
      continue;
    }
    parsed.push({ type: 'context', oldLine, newLine, text: line });
    oldLine += 1;
    newLine += 1;
  }

  return parsed;
};

const DiffRows: React.FC<{ diffText: string }> = ({ diffText }) => {
  const rows = parseUnifiedDiff(diffText);
  if (!rows.length) {
    return <div className="text-muted-foreground">该记录没有 diff 内容</div>;
  }

  return (
    <div className="font-mono text-xs">
      {rows.map((row, idx) => {
        let lineClass = 'text-foreground';
        if (row.type === 'hunk' || row.type === 'meta') {
          lineClass = 'text-muted-foreground';
        } else if (row.type === 'add') {
          lineClass = 'text-emerald-600 dark:text-emerald-400';
        } else if (row.type === 'del') {
          lineClass = 'text-rose-600 dark:text-rose-400';
        }
        return (
          <div key={`${idx}-${row.text}`} className={cn('grid grid-cols-[3rem_3rem_1fr] gap-2 leading-5', lineClass)}>
            <div className="text-right pr-2 text-muted-foreground">
              {row.oldLine ?? ''}
            </div>
            <div className="text-right pr-2 text-muted-foreground">
              {row.newLine ?? ''}
            </div>
            <div className="whitespace-pre">
              {row.text === '' ? ' ' : row.text}
            </div>
          </div>
        );
      })}
    </div>
  );
};

interface DiffPanelProps {
  selectedLog: ChangeLogItem | null;
}

export const DiffPanel: React.FC<DiffPanelProps> = ({ selectedLog }) => {
  if (!selectedLog) {
    return null;
  }

  const title = selectedLog.sessionTitle || selectedLog.sessionId || '未知会话';
  const time = selectedLog.createdAt ? new Date(selectedLog.createdAt).toLocaleString() : '';
  const kind = normalizeChangeKind(selectedLog.changeKind);

  return (
    <div className="border-b border-border bg-muted/30 max-h-64 overflow-hidden flex flex-col">
      <div className="px-4 py-2 text-xs font-medium text-foreground flex items-center gap-2">
        <span>变更内容</span>
        <span className="text-muted-foreground">{selectedLog.action}</span>
        <span className={CHANGE_KIND_TEXT_CLASS[kind]}>{CHANGE_KIND_LABEL[kind]}</span>
        <span className="text-muted-foreground ml-auto">{time}</span>
      </div>
      <div className="px-4 pb-3 text-xs overflow-auto min-h-0">
        <div className="text-[11px] text-muted-foreground mb-2 truncate" title={title}>
          会话：{title}
        </div>
        {selectedLog.diff ? <DiffRows diffText={selectedLog.diff} /> : (
          <div className="text-muted-foreground">该记录没有 diff 内容</div>
        )}
      </div>
    </div>
  );
};

interface ChangeLogPanelProps {
  selectedPath: string | null;
  loadingLogs: boolean;
  logsError: string | null;
  changeLogs: ChangeLogItem[];
  selectedLogId: string | null;
  onToggleLog: (logId: string) => void;
}

export const ChangeLogPanel: React.FC<ChangeLogPanelProps> = ({
  selectedPath,
  loadingLogs,
  logsError,
  changeLogs,
  selectedLogId,
  onToggleLog,
}) => {
  if (!selectedPath) {
    return <div className="px-4 py-3 text-xs text-muted-foreground">请选择文件或目录以查看变更记录</div>;
  }
  if (loadingLogs) {
    return <div className="px-4 py-3 text-xs text-muted-foreground">加载变更记录中...</div>;
  }
  if (logsError) {
    return <div className="px-4 py-3 text-xs text-destructive">{logsError}</div>;
  }
  if (!changeLogs.length) {
    return <div className="px-4 py-3 text-xs text-muted-foreground">暂无变更记录</div>;
  }

  return (
    <div className="divide-y divide-border">
      {changeLogs.map((log) => {
        const isSelected = selectedLogId === log.id;
        const title = log.sessionTitle || log.sessionId || '未知会话';
        const time = log.createdAt ? new Date(log.createdAt).toLocaleString() : '';
        const kind = normalizeChangeKind(log.changeKind);
        return (
          <button
            key={log.id}
            type="button"
            onClick={() => onToggleLog(log.id)}
            className={cn(
              'w-full px-4 py-2 text-xs text-left hover:bg-accent transition-colors',
              isSelected && 'bg-accent'
            )}
          >
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground w-3">{isSelected ? '▾' : '▸'}</span>
              <span className="font-medium text-foreground">{log.action}</span>
              <span className={cn('font-medium', CHANGE_KIND_TEXT_CLASS[kind])}>
                {CHANGE_KIND_LABEL[kind]}
              </span>
              <span className="text-muted-foreground">{formatFileSize(log.bytes || 0)}</span>
              <span className="text-muted-foreground ml-auto">{time}</span>
            </div>
            <div className="text-[11px] text-muted-foreground truncate" title={title}>
              会话：{title}
            </div>
          </button>
        );
      })}
    </div>
  );
};
