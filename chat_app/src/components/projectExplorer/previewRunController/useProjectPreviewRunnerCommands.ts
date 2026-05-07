import { useCallback, useState } from 'react';

import type { TerminalDispatchResponse } from '../../../lib/api/client/types';
import type {
  ProjectPreviewActiveRunState,
  ProjectPreviewRunSetter,
  StringStateSetter,
} from './previewRunControllerTypes';

interface UseProjectPreviewRunnerCommandsOptions {
  activeRun: ProjectPreviewActiveRunState | null;
  activeTerminalBusy: boolean;
  currentCommand: string;
  runTargetCwd: string;
  onRunCommand: (payload: { cwd: string; command: string }) => Promise<TerminalDispatchResponse>;
  onInterruptTerminal: (
    terminalId: string,
    payload?: { reason?: string },
  ) => Promise<TerminalDispatchResponse>;
  setActiveRun: ProjectPreviewRunSetter;
  setActiveTerminalBusy: (value: boolean) => void;
  setLastExitCheckedRunKey: (value: string) => void;
  setManualControlAt: (value: number) => void;
  setRunError: StringStateSetter;
  setRunMessage: StringStateSetter;
}

export const useProjectPreviewRunnerCommands = ({
  activeRun,
  activeTerminalBusy,
  currentCommand,
  runTargetCwd,
  onRunCommand,
  onInterruptTerminal,
  setActiveRun,
  setActiveTerminalBusy,
  setLastExitCheckedRunKey,
  setManualControlAt,
  setRunError,
  setRunMessage,
}: UseProjectPreviewRunnerCommandsOptions) => {
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);

  const runBy = useCallback(async (command: string, cwd: string, reasonLabel: string) => {
    setRunError(null);
    const result = await onRunCommand({ cwd, command });
    const terminalId = String(result?.terminal_id || result?.terminalId || '').trim();
    const terminalName = String(
      result?.terminal_name
      || result?.terminalName
      || terminalId
      || '',
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
        : `${reasonLabel}：命令已派发到终端`,
    );
  }, [
    onRunCommand,
    setActiveRun,
    setActiveTerminalBusy,
    setLastExitCheckedRunKey,
    setRunError,
    setRunMessage,
  ]);

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
  }, [currentCommand, runBy, runTargetCwd, setRunError, setRunMessage]);

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
  }, [
    activeRun,
    onInterruptTerminal,
    setActiveTerminalBusy,
    setManualControlAt,
    setRunError,
    setRunMessage,
  ]);

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
  }, [
    activeRun,
    activeTerminalBusy,
    onInterruptTerminal,
    runBy,
    setManualControlAt,
    setRunError,
    setRunMessage,
  ]);

  return {
    handleRestart,
    handleRun,
    handleStop,
    restarting,
    running,
    stopping,
  };
};
