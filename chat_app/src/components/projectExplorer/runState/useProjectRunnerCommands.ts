import { useCallback, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import type { Project } from '../../../types';
import {
  readProjectRunnerDispatchTarget,
  RUNNER_RESTART_COMMAND,
  RUNNER_START_COMMAND,
  RUNNER_STOP_COMMAND,
} from '../../../lib/domain/projectRunner';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

interface UseProjectRunnerCommandsOptions {
  client: ApiClient;
  project: Project | null;
  runnerScriptExists: boolean;
  setActiveRun: (value: ProjectRunnerActiveTerminal | null) => void;
  setActiveTerminalBusy: (value: boolean) => void;
}

export const useProjectRunnerCommands = ({
  client,
  project,
  runnerScriptExists,
  setActiveRun,
  setActiveTerminalBusy,
}: UseProjectRunnerCommandsOptions) => {
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [runnerMessage, setRunnerMessage] = useState<string | null>(null);
  const [runnerError, setRunnerError] = useState<string | null>(null);

  const dispatchRunnerCommand = useCallback(async (command: string, label: string) => {
    const rootPath = readTrimmedString(project?.rootPath || '');
    if (!project?.id || !rootPath) {
      throw new Error('项目根目录不存在');
    }
    if (!runnerScriptExists) {
      throw new Error('启动脚本不存在，请先点击“生成启动脚本”');
    }

    const result = await client.dispatchTerminalCommand({
      cwd: rootPath,
      command,
      project_id: project.id,
      create_if_missing: true,
    });
    const { terminalId, terminalName } = readProjectRunnerDispatchTarget(result);
    if (terminalId) {
      setActiveRun({
        terminalId,
        terminalName: terminalName || terminalId,
        cwd: rootPath,
        command,
        dispatchedAt: Date.now(),
      });
      setActiveTerminalBusy(true);
    }
    setRunnerMessage(
      terminalName
        ? `${label}：已在终端 ${terminalName} 执行`
        : `${label}：命令已派发到终端`,
    );
  }, [
    client,
    project?.id,
    project?.rootPath,
    runnerScriptExists,
    setActiveRun,
    setActiveTerminalBusy,
  ]);

  const handleRunnerStart = useCallback(async () => {
    setStarting(true);
    setRunnerError(null);
    try {
      await dispatchRunnerCommand(RUNNER_START_COMMAND, '启动成功');
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '启动失败');
      setRunnerMessage(null);
    } finally {
      setStarting(false);
    }
  }, [dispatchRunnerCommand]);

  const handleRunnerStop = useCallback(async () => {
    setStopping(true);
    setRunnerError(null);
    try {
      await dispatchRunnerCommand(RUNNER_STOP_COMMAND, '停止成功');
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '停止失败');
      setRunnerMessage(null);
    } finally {
      setStopping(false);
    }
  }, [dispatchRunnerCommand]);

  const handleRunnerRestart = useCallback(async () => {
    setRestarting(true);
    setRunnerError(null);
    try {
      await dispatchRunnerCommand(RUNNER_RESTART_COMMAND, '重启成功');
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '重启失败');
      setRunnerMessage(null);
    } finally {
      setRestarting(false);
    }
  }, [dispatchRunnerCommand]);

  const resetRunnerCommandState = useCallback(() => {
    setRunnerMessage(null);
    setRunnerError(null);
  }, []);

  return {
    starting,
    stopping,
    restarting,
    runnerMessage,
    runnerError,
    resetRunnerCommandState,
    handleRunnerStart,
    handleRunnerStop,
    handleRunnerRestart,
  };
};
