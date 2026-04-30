import { useCallback, useEffect, useMemo } from 'react';

import type ApiClient from '../../lib/api/client';
import { loadTerminalsSnapshot } from '../../lib/store/actions/terminalsCache';
import { useChatRuntimeEnv } from '../../lib/store/ChatStoreContext';
import type {
  FsEntry,
  Project,
} from '../../types';
import {
  RUNNER_RESTART_COMMAND,
  RUNNER_START_COMMAND,
  RUNNER_STOP_COMMAND,
} from '../../lib/domain/projectRunner';
import { useProjectRunnerCatalogState } from './runState/useProjectRunnerCatalogState';
import { useProjectRunnerCommands } from './runState/useProjectRunnerCommands';
import { useProjectRunnerTerminalPolling } from './runState/useProjectRunnerTerminalPolling';
import { useProjectSingleFileRunner } from './runState/useProjectSingleFileRunner';
export type { ProjectRunnerActiveTerminal, ProjectRunnerMember } from '../../lib/domain/projectRunner';

interface UseProjectExplorerRunStateParams {
  client: ApiClient;
  project: Project | null;
  selectedEntry: FsEntry | null;
  selectedPath: string | null;
  getParentPath: (path: string | null | undefined) => string;
  setActionError: (value: string | null) => void;
  setActionLoading: (value: boolean) => void;
  setActionMessage: (value: string | null) => void;
}

export const useProjectExplorerRunState = ({
  client,
  project,
  selectedEntry,
  selectedPath,
  getParentPath,
  setActionError,
  setActionLoading,
  setActionMessage,
}: UseProjectExplorerRunStateParams) => {
  const { userId } = useChatRuntimeEnv();
  const runnerCatalog = useProjectRunnerCatalogState({ client, project });
  const runnerTerminal = useProjectRunnerTerminalPolling({ client, project });
  const runnerCommands = useProjectRunnerCommands({
    client,
    project,
    runnerScriptExists: runnerCatalog.runnerScriptExists,
    setActiveRun: runnerTerminal.setActiveRun,
    setActiveTerminalBusy: runnerTerminal.setActiveTerminalBusy,
  });
  const singleFileRunner = useProjectSingleFileRunner({
    client,
    project,
    setActionError,
    setActionLoading,
    setActionMessage,
  });

  const runCwd = useMemo(() => {
    if (!project?.rootPath) {
      return '';
    }
    if (selectedEntry?.isDir) {
      return selectedEntry.path;
    }
    if (selectedEntry && !selectedEntry.isDir) {
      return getParentPath(selectedEntry.path) || project.rootPath;
    }
    if (selectedPath) {
      return getParentPath(selectedPath) || project.rootPath;
    }
    return project.rootPath;
  }, [getParentPath, project?.rootPath, selectedEntry, selectedPath]);

  const handleDispatchTerminalCommand = useCallback(async (payload: { cwd: string; command: string }) => {
    return client.dispatchTerminalCommand({
      cwd: payload.cwd,
      command: payload.command,
      project_id: project?.id,
      create_if_missing: true,
    });
  }, [client, project?.id]);

  const handleInterruptTerminal = useCallback(async (terminalId: string, payload?: { reason?: string }) => {
    return client.interruptTerminal(terminalId, payload);
  }, [client]);

  const handleGetTerminal = useCallback(async (terminalId: string) => {
    return client.getTerminal(terminalId);
  }, [client]);

  const handleListTerminalLogs = useCallback(async (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ) => {
    return client.listTerminalLogs(terminalId, params);
  }, [client]);

  const handleListTerminals = useCallback(async () => {
    return loadTerminalsSnapshot(client, userId);
  }, [client, userId]);

  useEffect(() => {
    runnerCatalog.resetRunnerCatalogState();
    runnerCommands.resetRunnerCommandState();
    runnerTerminal.resetActiveRunState();
    if (!project?.id) {
      return;
    }
    void runnerCatalog.refreshRunnerState();
  }, [
    project?.id,
    runnerCatalog.refreshRunnerState,
    runnerCatalog.resetRunnerCatalogState,
    runnerCommands.resetRunnerCommandState,
    runnerTerminal.resetActiveRunState,
  ]);

  const handleAnalyzeRunTargets = useCallback(() => {
    void runnerCatalog.refreshRunnerState();
  }, [runnerCatalog.refreshRunnerState]);

  return {
    runCwd,
    runStatus: runnerCatalog.runStatus,
    runTargets: runnerCatalog.runTargets,
    runCatalogLoading: runnerCatalog.runCatalogLoading,
    runCatalogError: runnerCatalog.runCatalogError,
    selectedRunTargetId: runnerCatalog.selectedRunTargetId,
    setSelectedRunTargetId: runnerCatalog.setSelectedRunTargetId,
    handleDispatchTerminalCommand,
    handleInterruptTerminal,
    handleGetTerminal,
    handleListTerminalLogs,
    handleListTerminals,
    handleAnalyzeRunTargets,
    canRunFile: singleFileRunner.canRunFile,
    handleRunFile: singleFileRunner.handleRunFile,
    projectMembers: runnerCatalog.projectMembers,
    projectMembersLoading: runnerCatalog.projectMembersLoading,
    projectMembersError: runnerCatalog.projectMembersError,
    runnerScriptExists: runnerCatalog.runnerScriptExists,
    runnerScriptChecking: runnerCatalog.runnerScriptChecking,
    runnerScriptPath: runnerCatalog.runnerScriptPath,
    runnerStartCommand: RUNNER_START_COMMAND,
    runnerStopCommand: RUNNER_STOP_COMMAND,
    runnerRestartCommand: RUNNER_RESTART_COMMAND,
    starting: runnerCommands.starting,
    stopping: runnerCommands.stopping,
    restarting: runnerCommands.restarting,
    runnerMessage: runnerCommands.runnerMessage,
    runnerError: runnerCommands.runnerError,
    activeRun: runnerTerminal.activeRun,
    activeTerminalBusy: runnerTerminal.activeTerminalBusy,
    handleRunnerStart: runnerCommands.handleRunnerStart,
    handleRunnerStop: runnerCommands.handleRunnerStop,
    handleRunnerRestart: runnerCommands.handleRunnerRestart,
    refreshRunnerState: runnerCatalog.refreshRunnerState,
  };
};
