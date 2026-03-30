import React, { useEffect, useMemo, useState } from 'react';
import hljs from 'highlight.js';

import type { ChangeLogItem, FsEntry, FsReadResult, ProjectRunTarget } from '../../types';
import { formatFileSize } from '../../lib/utils';
import { DiffPanel } from './ChangeLogPanels';
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
  onRunCommand: (payload: { cwd: string; command: string }) => Promise<any>;
  onInterruptTerminal: (terminalId: string, payload?: { reason?: string }) => Promise<any>;
  onGetTerminal: (terminalId: string) => Promise<any>;
  onListTerminalLogs: (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string }
  ) => Promise<any[]>;
  onListTerminals: () => Promise<any[]>;
  runTargets: ProjectRunTarget[];
  runStatus: string;
  runCatalogLoading: boolean;
  runCatalogError: string | null;
  selectedRunTargetId: string | null;
  onSelectRunTarget: (targetId: string | null) => void;
  onAnalyzeRunTargets: () => void;
}

interface ActiveRunState {
  terminalId: string;
  terminalName: string;
  cwd: string;
  command: string;
  dispatchedAt: number;
  origin: 'dispatched' | 'discovered';
}

const extractFailureReasonFromLogs = (logs: any[], command: string): string | null => {
  const lines = logs
    .map((item) => String(item?.content || ''))
    .join('\n')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) return null;
  const checks: RegExp[] = [
    /command not found/i,
    /no such file or directory/i,
    /permission denied/i,
    /traceback \(most recent call last\)/i,
    /\berr(or)?\b/i,
    /\bpanic\b/i,
    /\bexception\b/i,
    /\bfailed\b/i,
  ];
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    const line = lines[i];
    if (checks.some((regex) => regex.test(line))) {
      return line;
    }
  }
  const cmd = command.toLowerCase();
  const likelyLongRunning = /(run|start|dev|serve|bootrun|spring-boot:run)/i.test(cmd)
    && !/(test|build|lint)/i.test(cmd);
  if (likelyLongRunning) {
    return '命令已退出，未检测到持续运行进程';
  }
  return null;
};

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
  const [runCommand, setRunCommand] = useState('');
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<ActiveRunState | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);
  const [manualControlAt, setManualControlAt] = useState(0);
  const [lastExitCheckedRunKey, setLastExitCheckedRunKey] = useState<string>('');

  const selectedRunTarget = useMemo(
    () => runTargets.find((item) => item.id === selectedRunTargetId) || null,
    [runTargets, selectedRunTargetId]
  );
  const runTargetCwd = (selectedRunTarget?.cwd || runCwd || projectRootPath || '').trim();

  const currentCommand = (runCommand.trim() || selectedRunTarget?.command || '').trim();

  const runBy = async (command: string, cwd: string, reasonLabel: string) => {
    setRunError(null);
    const result = await onRunCommand({ cwd, command });
    const terminalId = String(result?.terminal_id || '').trim();
    const terminalName = String(result?.terminal_name || terminalId || '').trim();
    if (terminalId) {
      setActiveRun({
        terminalId,
        terminalName,
        command,
        cwd,
        dispatchedAt: Date.now(),
        origin: 'dispatched',
      });
      setActiveTerminalBusy(true);
      setLastExitCheckedRunKey('');
    }
    setRunMessage(
      terminalName
        ? `${reasonLabel}：已在终端 ${terminalName} 执行`
        : `${reasonLabel}：命令已派发到终端`
    );
  };

  const handleRun = async () => {
    const command = currentCommand;
    if (!runTargetCwd) {
      setRunError('未找到可执行目录');
      setRunMessage(null);
      return;
    }
    if (!command) {
      setRunError('请输入运行命令');
      setRunMessage(null);
      return;
    }
    setRunning(true);
    setRunError(null);
    try {
      await runBy(command, runTargetCwd, '启动成功');
    } catch (err: any) {
      setRunError(err?.message || '运行失败');
      setRunMessage(null);
    } finally {
      setRunning(false);
    }
  };

  const handleStop = async () => {
    if (!activeRun?.terminalId) return;
    setStopping(true);
    setRunError(null);
    try {
      setManualControlAt(Date.now());
      await onInterruptTerminal(activeRun.terminalId, { reason: 'project_preview_stop' });
      setActiveTerminalBusy(false);
      setRunMessage(`已请求停止 ${activeRun.terminalName || activeRun.terminalId}`);
    } catch (err: any) {
      setRunError(err?.message || '停止失败');
      setRunMessage(null);
    } finally {
      setStopping(false);
    }
  };

  const handleRestart = async () => {
    const target = activeRun;
    if (!target) return;
    setRestarting(true);
    setRunError(null);
    try {
      if (activeTerminalBusy) {
        setManualControlAt(Date.now());
        await onInterruptTerminal(target.terminalId, { reason: 'project_preview_restart' });
        await new Promise((resolve) => setTimeout(resolve, 180));
      }
      await runBy(target.command, target.cwd, '重启成功');
    } catch (err: any) {
      setRunError(err?.message || '重启失败');
      setRunMessage(null);
    } finally {
      setRestarting(false);
    }
  };

  useEffect(() => {
    if (!projectId) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const list = await onListTerminals();
        if (disposed || !Array.isArray(list)) return;
        const related = list
          .filter((item: any) => String(item?.project_id || item?.projectId || '') === projectId)
          .sort((a: any, b: any) => {
            const ta = new Date(a?.last_active_at || a?.lastActiveAt || 0).getTime();
            const tb = new Date(b?.last_active_at || b?.lastActiveAt || 0).getTime();
            return tb - ta;
          });
        const busy = related.find((item: any) => Boolean(item?.busy));
        const chosen = busy || related[0] || null;
        if (chosen) {
          const terminalId = String(chosen?.id || '').trim();
          if (terminalId) {
            setActiveTerminalBusy(Boolean(chosen?.busy));
            setActiveRun((prev) => {
              if (prev?.origin === 'dispatched' && prev.terminalId === terminalId) {
                return prev;
              }
              return {
                terminalId,
                terminalName: String(chosen?.name || terminalId),
                command: prev?.command || currentCommand || selectedRunTarget?.command || '',
                cwd: String(chosen?.cwd || runTargetCwd || projectRootPath || ''),
                dispatchedAt: prev?.dispatchedAt || Date.now(),
                origin: 'discovered',
              };
            });
          }
        }
      } catch {
        // ignore discovery polling errors
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 2000);
        }
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [currentCommand, onListTerminals, projectId, projectRootPath, runTargetCwd, selectedRunTarget?.command]);

  useEffect(() => {
    if (!activeRun?.terminalId) {
      setActiveTerminalBusy(false);
      return;
    }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const terminal = await onGetTerminal(activeRun.terminalId);
        if (disposed) return;
        setActiveTerminalBusy(Boolean(terminal?.busy));
      } catch {
        if (!disposed) {
          setActiveTerminalBusy(false);
        }
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 1500);
        }
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [activeRun?.terminalId, onGetTerminal]);

  useEffect(() => {
    if (!activeRun?.terminalId) return;
    if (activeRun.origin !== 'dispatched') return;
    if (activeTerminalBusy) return;
    const runKey = `${activeRun.terminalId}:${activeRun.dispatchedAt}`;
    if (runKey === lastExitCheckedRunKey) return;
    if (manualControlAt > 0 && Date.now() - manualControlAt < 3500) {
      setLastExitCheckedRunKey(runKey);
      return;
    }

    let disposed = false;
    const inspect = async () => {
      try {
        const logs = await onListTerminalLogs(activeRun.terminalId, { limit: 80, offset: 0 });
        if (disposed) return;
        const reason = extractFailureReasonFromLogs(logs || [], activeRun.command);
        if (reason) {
          setRunError(`运行失败：${reason}`);
          setRunMessage(null);
        }
      } catch {
        // ignore log inspection errors
      } finally {
        if (!disposed) {
          setLastExitCheckedRunKey(runKey);
        }
      }
    };
    void inspect();
    return () => {
      disposed = true;
    };
  }, [
    activeRun,
    activeTerminalBusy,
    lastExitCheckedRunKey,
    manualControlAt,
    onListTerminalLogs,
  ]);

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
