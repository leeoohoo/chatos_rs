import { useCallback, useEffect, useMemo, useState } from 'react';

import type {
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
} from '../../lib/api/client/types';
import type { ProjectRunTarget } from '../../types';

interface ActiveRunState {
  terminalId: string;
  terminalName: string;
  cwd: string;
  command: string;
  dispatchedAt: number;
  origin: 'dispatched' | 'discovered';
}

interface UseProjectPreviewRunControllerParams {
  projectId: string;
  projectRootPath: string;
  runCwd: string;
  runTargets: ProjectRunTarget[];
  selectedRunTargetId: string | null;
  onRunCommand: (payload: { cwd: string; command: string }) => Promise<TerminalDispatchResponse>;
  onInterruptTerminal: (terminalId: string, payload?: { reason?: string }) => Promise<TerminalDispatchResponse>;
  onGetTerminal: (terminalId: string) => Promise<TerminalResponse>;
  onListTerminalLogs: (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string }
  ) => Promise<TerminalLogResponse[]>;
  onListTerminals: () => Promise<TerminalResponse[]>;
}

const extractFailureReasonFromLogs = (logs: TerminalLogResponse[], command: string): string | null => {
  const lines = logs
    .map((item) => String(item?.content || ''))
    .join('\n')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) {
    return null;
  }
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

export const useProjectPreviewRunController = ({
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
}: UseProjectPreviewRunControllerParams) => {
  const [runCommand, setRunCommand] = useState('');
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [runMessage, setRunMessage] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<ActiveRunState | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);
  const [manualControlAt, setManualControlAt] = useState(0);
  const [lastExitCheckedRunKey, setLastExitCheckedRunKey] = useState('');

  const selectedRunTarget = useMemo(
    () => runTargets.find((item) => item.id === selectedRunTargetId) || null,
    [runTargets, selectedRunTargetId]
  );

  const runTargetCwd = useMemo(
    () => (selectedRunTarget?.cwd || runCwd || projectRootPath || '').trim(),
    [projectRootPath, runCwd, selectedRunTarget?.cwd]
  );

  const currentCommand = useMemo(
    () => (runCommand.trim() || selectedRunTarget?.command || '').trim(),
    [runCommand, selectedRunTarget?.command]
  );

  const runBy = useCallback(async (command: string, cwd: string, reasonLabel: string) => {
    setRunError(null);
    const result = await onRunCommand({ cwd, command });
    const terminalId = String(result?.terminal_id || result?.terminalId || '').trim();
    const terminalName = String(
      result?.terminal_name
      || result?.terminalName
      || terminalId
      || ''
    ).trim();
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
  }, [onRunCommand]);

  const handleRun = useCallback(async () => {
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
    } catch (err) {
      setRunError(err instanceof Error ? err.message : '运行失败');
      setRunMessage(null);
    } finally {
      setRunning(false);
    }
  }, [currentCommand, runBy, runTargetCwd]);

  const handleStop = useCallback(async () => {
    if (!activeRun?.terminalId) {
      return;
    }
    setStopping(true);
    setRunError(null);
    try {
      setManualControlAt(Date.now());
      await onInterruptTerminal(activeRun.terminalId, { reason: 'project_preview_stop' });
      setActiveTerminalBusy(false);
      setRunMessage(`已请求停止 ${activeRun.terminalName || activeRun.terminalId}`);
    } catch (err) {
      setRunError(err instanceof Error ? err.message : '停止失败');
      setRunMessage(null);
    } finally {
      setStopping(false);
    }
  }, [activeRun, onInterruptTerminal]);

  const handleRestart = useCallback(async () => {
    const target = activeRun;
    if (!target) {
      return;
    }
    setRestarting(true);
    setRunError(null);
    try {
      if (activeTerminalBusy) {
        setManualControlAt(Date.now());
        await onInterruptTerminal(target.terminalId, { reason: 'project_preview_restart' });
        await new Promise((resolve) => setTimeout(resolve, 180));
      }
      await runBy(target.command, target.cwd, '重启成功');
    } catch (err) {
      setRunError(err instanceof Error ? err.message : '重启失败');
      setRunMessage(null);
    } finally {
      setRestarting(false);
    }
  }, [activeRun, activeTerminalBusy, onInterruptTerminal, runBy]);

  useEffect(() => {
    if (!projectId) {
      return;
    }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const list = await onListTerminals();
        if (disposed || !Array.isArray(list)) {
          return;
        }
        const related = list
          .filter((item) => String(item?.project_id || item?.projectId || '') === projectId)
          .sort((a, b) => {
            const ta = new Date(a?.last_active_at || a?.lastActiveAt || 0).getTime();
            const tb = new Date(b?.last_active_at || b?.lastActiveAt || 0).getTime();
            return tb - ta;
          });
        const busy = related.find((item) => Boolean(item?.busy));
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
        if (disposed) {
          return;
        }
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
    if (!activeRun?.terminalId) {
      return;
    }
    if (activeRun.origin !== 'dispatched') {
      return;
    }
    if (activeTerminalBusy) {
      return;
    }
    const runKey = `${activeRun.terminalId}:${activeRun.dispatchedAt}`;
    if (runKey === lastExitCheckedRunKey) {
      return;
    }
    if (manualControlAt > 0 && Date.now() - manualControlAt < 3500) {
      setLastExitCheckedRunKey(runKey);
      return;
    }

    let disposed = false;
    const inspect = async () => {
      try {
        const logs = await onListTerminalLogs(activeRun.terminalId, { limit: 80, offset: 0 });
        if (disposed) {
          return;
        }
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

  return {
    activeRun,
    activeTerminalBusy,
    currentCommand,
    restarting,
    runCommand,
    runError,
    runMessage,
    runTargetCwd,
    running,
    selectedRunTarget,
    setRunCommand,
    stopping,
    handleRestart,
    handleRun,
    handleStop,
  };
};
