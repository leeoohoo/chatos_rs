import React, { useMemo } from 'react';
import hljs from 'highlight.js';

import type {
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
} from '../../lib/api/client/types';
import type { ChangeLogItem, FsEntry, FsReadResult, ProjectRunTarget } from '../../types';
import { formatFileSize } from '../../lib/utils';
import { DiffPanel } from './ChangeLogPanels';
import { useProjectPreviewRunController } from './useProjectPreviewRunController';
import { escapeHtml, getHighlightLanguage } from './utils';

interface ProjectPreviewPaneProps {
  projectId: string;
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  error: string | null;
  selectedLog: ChangeLogItem | null;
  projectRootPath: string;
  runCwd: string;
  onRunCommand: (payload: { cwd: string; command: string }) => Promise<TerminalDispatchResponse>;
  onInterruptTerminal: (terminalId: string, payload?: { reason?: string }) => Promise<TerminalDispatchResponse>;
  onGetTerminal: (terminalId: string) => Promise<TerminalResponse>;
  onListTerminalLogs: (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string }
  ) => Promise<TerminalLogResponse[]>;
  onListTerminals: () => Promise<TerminalResponse[]>;
  runTargets: ProjectRunTarget[];
  runStatus: string;
  runCatalogLoading: boolean;
  runCatalogError: string | null;
  selectedRunTargetId: string | null;
  onSelectRunTarget: (targetId: string | null) => void;
  onAnalyzeRunTargets: () => void;
}

export const ProjectPreviewPane: React.FC<ProjectPreviewPaneProps> = ({
  projectId,
  selectedFile,
  selectedPath,
  selectedEntry,
  loadingFile,
  error,
  selectedLog,
  projectRootPath,
  runCwd,
  onRunCommand,
  onInterruptTerminal,
  onGetTerminal,
  onListTerminalLogs,
  onListTerminals,
  runTargets,
  runStatus,
  runCatalogLoading,
  runCatalogError,
  selectedRunTargetId,
  onSelectRunTarget,
  onAnalyzeRunTargets,
}) => {
  const {
    activeRun,
    activeTerminalBusy,
    restarting,
    runCommand,
    runError,
    runMessage,
    runTargetCwd,
    running,
    setRunCommand,
    stopping,
    handleRestart,
    handleRun,
    handleStop,
  } = useProjectPreviewRunController({
    projectId,
    projectRootPath,
    runCwd,
    runTargets,
    selectedRunTargetId,
    onRunCommand,
    onInterruptTerminal,
    onGetTerminal,
    onListTerminalLogs,
    onListTerminals,
  });

  const preview = useMemo(() => {
    if (loadingFile) {
      return <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>;
    }
    if (!selectedFile) {
      if (selectedPath && !selectedEntry) {
        return (
          <div className="p-4 text-sm text-muted-foreground">
            该路径已删除或不存在，当前仅支持查看变更记录。
          </div>
        );
      }
      return <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>;
    }
    const isImage = selectedFile.contentType.startsWith('image/');
    if (isImage && selectedFile.isBinary) {
      const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
      return (
        <div className="p-4 overflow-auto h-full">
          <img src={src} alt={selectedFile.name} className="max-w-full max-h-full rounded border border-border" />
        </div>
      );
    }
    if (!selectedFile.isBinary) {
      const language = getHighlightLanguage(selectedFile.name);
      let highlighted = '';
      try {
        if (language) {
          highlighted = hljs.highlight(selectedFile.content, { language }).value;
        } else {
          highlighted = hljs.highlightAuto(selectedFile.content).value;
        }
      } catch {
        highlighted = escapeHtml(selectedFile.content);
      }
      const lines = highlighted.split(/\r?\n/);
      return (
        <div className="h-full overflow-auto bg-muted/30">
          <div className="flex min-h-full text-sm">
            <div className="shrink-0 py-4 pr-3 pl-2 border-r border-border text-right text-muted-foreground select-none">
              {lines.map((_, idx) => (
                <div key={idx} className="leading-5">
                  {idx + 1}
                </div>
              ))}
            </div>
            <div className="flex-1 min-w-0 py-4 pl-3 pr-4 hljs">
              {lines.map((line, idx) => (
                <div
                  key={idx}
                  className="leading-5 font-mono whitespace-pre w-full"
                  dangerouslySetInnerHTML={{ __html: line || '&nbsp;' }}
                />
              ))}
            </div>
          </div>
        </div>
      );
    }
    const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
    return (
      <div className="p-4 text-sm text-muted-foreground space-y-2">
        <div>该文件为二进制内容，暂不支持直接预览。</div>
        <a
          href={downloadHref}
          download={selectedFile.name || 'file'}
          className="inline-flex items-center px-3 py-1.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          下载文件
        </a>
      </div>
    );
  }, [loadingFile, selectedEntry, selectedFile, selectedPath]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-4 py-2 border-b border-border bg-card flex items-center justify-between">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-foreground truncate">
            {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            {selectedFile?.path || selectedPath || '请选择文件'}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            运行目录：{runTargetCwd || '-'}
          </div>
          <div className="text-[11px] text-muted-foreground truncate">
            运行目标：{runStatus || '-'} / {runTargets.length}
          </div>
          {activeRun && (
            <div className="text-[11px] text-muted-foreground truncate">
              <span className={activeTerminalBusy ? 'text-emerald-600' : 'text-slate-500'}>●</span>
              {' '}终端：{activeRun.terminalName || activeRun.terminalId} / {activeTerminalBusy ? '运行中' : '空闲'}
            </div>
          )}
          {runCatalogError && (
            <div className="text-[11px] text-destructive truncate" title={runCatalogError}>
              {runCatalogError}
            </div>
          )}
        </div>
        <div className="ml-3 flex items-center gap-2">
          <select
            value={selectedRunTargetId || ''}
            onChange={(event) => {
              const value = event.target.value.trim();
              onSelectRunTarget(value || null);
              const target = runTargets.find((item) => item.id === value);
              if (target) {
                setRunCommand(target.command || '');
              }
            }}
            className="h-8 max-w-[260px] rounded border border-border bg-background px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-blue-500"
          >
            <option value="">手动命令</option>
            {runTargets.map((target) => (
              <option key={target.id} value={target.id}>
                {target.label}
              </option>
            ))}
          </select>
          <input
            value={runCommand}
            onChange={(event) => setRunCommand(event.target.value)}
            placeholder="输入命令，例如 npm run dev"
            className="h-8 w-64 rounded border border-border bg-background px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-blue-500"
          />
          <button
            type="button"
            onClick={() => {
              if (activeRun && activeTerminalBusy) {
                void handleStop();
                return;
              }
              void handleRun();
            }}
            disabled={running || stopping || restarting || !runTargetCwd}
            className="h-8 rounded border border-emerald-500/40 px-3 text-xs text-emerald-700 hover:bg-emerald-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {running ? '运行中...' : (activeRun && activeTerminalBusy ? (stopping ? '停止中...' : '停止') : '运行')}
          </button>
          {activeRun && (
            <button
              type="button"
              onClick={() => { void handleRestart(); }}
              disabled={running || stopping || restarting}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {restarting ? '重启中...' : '重启'}
            </button>
          )}
          <button
            type="button"
            onClick={onAnalyzeRunTargets}
            disabled={runCatalogLoading}
            className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {runCatalogLoading ? '分析中...' : '重扫目标'}
          </button>
          {selectedFile && (
            <div className="text-[11px] text-muted-foreground whitespace-nowrap">
              {formatFileSize(selectedFile.size)}
            </div>
          )}
        </div>
      </div>
      {(runMessage || runError) && (
        <div className="px-4 py-1.5 border-b border-border/70 bg-card">
          <div className={runError ? 'text-[11px] text-destructive' : 'text-[11px] text-emerald-600'}>
            {runError || runMessage}
          </div>
        </div>
      )}
      <div className="flex-1 overflow-hidden flex flex-col">
        <DiffPanel selectedLog={selectedLog} />
        <div className="flex-1 min-h-0 overflow-hidden">
          {error ? (
            <div className="p-4 text-sm text-destructive">{error}</div>
          ) : (
            preview
          )}
        </div>
      </div>
    </div>
  );
};
