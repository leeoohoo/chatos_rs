import { useCallback, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type ApiClient from '../../../lib/api/client';
import { ApiRequestError } from '../../../lib/api/client/shared';
import { readProjectRunnerDispatchTarget } from '../../../lib/domain/projectRunner';
import { normalizeProjectRunValidationIssue } from '../../../lib/domain/projectExplorer';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import type { Project, ProjectRunTarget } from '../../../types';
import {
  formatProjectRunValidationIssues,
} from './projectRunnerFailureDiagnostics';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

interface UseProjectRunnerCommandsOptions {
  client: ApiClient;
  project: Project | null;
  runTargets: ProjectRunTarget[];
  selectedRunTargetId: string | null;
  commandPreview: string;
  activeRun: ProjectRunnerActiveTerminal | null;
  projectRunTerminalIds: string[];
  selectedTerminalId: string | null;
  selectRunInstance: (terminalId: string | null) => void;
  setActiveRun: (value: ProjectRunnerActiveTerminal | null) => void;
  setLastExitedRun: Dispatch<SetStateAction<ProjectRunnerActiveTerminal | null>>;
  setActiveTerminalBusy: (value: boolean) => void;
  removeRunInstanceLocally: (terminalId: string, nextSelectedTerminalId?: string | null) => void;
  refreshProjectActiveRun: () => Promise<void>;
}

const extractRunValidationMessage = (error: unknown, fallback: string): string => {
  if (!(error instanceof ApiRequestError) || !error.payload || typeof error.payload !== 'object') {
    return fallback;
  }
  const payload = error.payload as Record<string, unknown>;
  const rawIssues = payload.validation_issues;
  if (!Array.isArray(rawIssues)) {
    return fallback;
  }
  const issues = rawIssues
    .map(normalizeProjectRunValidationIssue)
    .filter((item) => item.kind && item.message);
  return formatProjectRunValidationIssues(issues, fallback);
};

export const useProjectRunnerCommands = ({
  client,
  project,
  runTargets,
  selectedRunTargetId,
  commandPreview,
  activeRun,
  projectRunTerminalIds,
  selectedTerminalId,
  selectRunInstance,
  setActiveRun,
  setLastExitedRun,
  setActiveTerminalBusy,
  removeRunInstanceLocally,
  refreshProjectActiveRun,
}: UseProjectRunnerCommandsOptions) => {
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [runnerMessage, setRunnerMessage] = useState<string | null>(null);
  const [runnerError, setRunnerError] = useState<string | null>(null);
  const [runnerDiagnosis, setRunnerDiagnosis] = useState<string | null>(null);
  const [manualControlAt, setManualControlAt] = useState(0);
  const [lastExitCheckedRunKey, setLastExitCheckedRunKey] = useState('');

  const selectedRunTarget = runTargets.find((item) => item.id === selectedRunTargetId) || runTargets[0] || null;

  const dispatchProjectRunTarget = useCallback(async (
    target: ProjectRunTarget,
    label: string,
    preferredTerminalId?: string | null,
  ) => {
    if (!project?.id) {
      throw new Error('项目不存在');
    }
    const result = await client.executeProjectRun(project.id, {
      target_id: target.id,
      create_if_missing: true,
      terminal_id: preferredTerminalId || undefined,
    });
    const { terminalId, terminalName } = readProjectRunnerDispatchTarget(result);
    setLastExitedRun(null);
    if (terminalId) {
      selectRunInstance(terminalId);
      setActiveRun({
        terminalId,
        terminalName: terminalName || terminalId,
        cwd: target.cwd,
        command: commandPreview || target.command,
        dispatchedAt: Date.now(),
        origin: 'dispatched',
        exitCode: null,
        exitReason: null,
      });
      setActiveTerminalBusy(true);
      setLastExitCheckedRunKey('');
    }
    setRunnerMessage(
      terminalName
        ? `${label}：已在终端 ${terminalName} 执行`
        : `${label}：命令已派发到终端`,
    );
    setRunnerDiagnosis(null);
  }, [
    commandPreview,
    client,
    project?.id,
    selectRunInstance,
    setActiveRun,
    setLastExitedRun,
    setActiveTerminalBusy,
  ]);

  const handleRunnerStart = useCallback(async () => {
    setStarting(true);
    setRunnerError(null);
    try {
      if (!selectedRunTarget) {
        throw new Error('未发现可运行目标');
      }
      await dispatchProjectRunTarget(selectedRunTarget, '启动成功', null);
    } catch (error) {
      setRunnerError(extractRunValidationMessage(error, error instanceof Error ? error.message : '启动失败'));
      setRunnerMessage(null);
    } finally {
      setStarting(false);
    }
  }, [dispatchProjectRunTarget, selectedRunTarget]);

  const handleRunnerStop = useCallback(async () => {
    setStopping(true);
    setRunnerError(null);
    try {
      const terminalId = readTrimmedString(selectedTerminalId || activeRun?.terminalId);
      if (!terminalId) {
        throw new Error('当前项目还没有独立运行终端');
      }
      setManualControlAt(Date.now());
      await client.interruptTerminal(terminalId, { reason: 'project_run_stop' });
      setActiveTerminalBusy(false);
      setRunnerMessage('停止成功：已请求中断当前运行终端');
      setRunnerDiagnosis(null);
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '停止失败');
      setRunnerMessage(null);
    } finally {
      setStopping(false);
    }
  }, [
    client,
    selectedTerminalId,
    activeRun?.terminalId,
    setActiveTerminalBusy,
  ]);

  const handleRunnerRestart = useCallback(async () => {
    setRestarting(true);
    setRunnerError(null);
    try {
      if (!project?.id) {
        throw new Error('项目不存在');
      }
      if (!selectedRunTarget) {
        throw new Error('未发现可运行目标');
      }
      const terminalId = readTrimmedString(selectedTerminalId || activeRun?.terminalId);
      if (terminalId) {
        setManualControlAt(Date.now());
        await client.interruptTerminal(terminalId, { reason: 'project_run_restart' });
      }
      await dispatchProjectRunTarget(selectedRunTarget, '重启成功', terminalId || null);
    } catch (error) {
      setRunnerError(extractRunValidationMessage(error, error instanceof Error ? error.message : '重启失败'));
      setRunnerMessage(null);
    } finally {
      setRestarting(false);
    }
  }, [activeRun?.terminalId, client, dispatchProjectRunTarget, project?.id, selectedRunTarget, selectedTerminalId]);

  const handleRunnerDelete = useCallback(async () => {
    setDeleting(true);
    setRunnerError(null);
    try {
      const terminalId = readTrimmedString(selectedTerminalId || activeRun?.terminalId);
      if (!terminalId) {
        throw new Error('当前项目还没有独立运行终端');
      }
      const currentIndex = projectRunTerminalIds.findIndex((item) => item === terminalId);
      const nextTerminalId = currentIndex >= 0
        ? (projectRunTerminalIds[currentIndex + 1]
          || projectRunTerminalIds[currentIndex - 1]
          || null)
        : null;
      await client.deleteTerminal(terminalId);
      removeRunInstanceLocally(terminalId, nextTerminalId);
      selectRunInstance(nextTerminalId);
      if (activeRun?.terminalId === terminalId) {
        setActiveRun(null);
      }
      setLastExitedRun((value) => (value?.terminalId === terminalId ? null : value));
      setActiveTerminalBusy(false);
      setRunnerMessage(nextTerminalId ? '删除成功：已切换到其它项目实例' : '删除成功：当前项目实例已移除');
      setRunnerDiagnosis(null);
      await refreshProjectActiveRun();
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '删除失败');
      setRunnerMessage(null);
    } finally {
      setDeleting(false);
    }
  }, [
    selectedTerminalId,
    activeRun,
    projectRunTerminalIds,
    client,
    removeRunInstanceLocally,
    selectRunInstance,
    setActiveRun,
    setLastExitedRun,
    setActiveTerminalBusy,
    refreshProjectActiveRun,
  ]);

  const resetRunnerCommandState = useCallback(() => {
    setRunnerMessage(null);
    setRunnerError(null);
    setRunnerDiagnosis(null);
    setManualControlAt(0);
    setLastExitCheckedRunKey('');
  }, []);

  return {
    starting,
    stopping,
    restarting,
    deleting,
    runnerMessage,
    runnerError,
    runnerDiagnosis,
    manualControlAt,
    lastExitCheckedRunKey,
    setLastExitCheckedRunKey,
    setRunnerDiagnosis,
    setRunnerError,
    setRunnerMessage,
    resetRunnerCommandState,
    handleRunnerStart,
    handleRunnerStop,
    handleRunnerRestart,
    handleRunnerDelete,
  };
};
