import { useMemo } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type ApiClient from '../../lib/api/client';
import type { Project } from '../../types';
import { useProjectRunnerCatalogState } from './runState/useProjectRunnerCatalogState';
import { useProjectRunnerCommands } from './runState/useProjectRunnerCommands';
import { buildProjectRunResolutionSuggestions } from './runState/projectRunnerResolutionSuggestions';
import { useProjectRunnerExitInspection } from './runState/useProjectRunnerExitInspection';
import { useProjectRunnerTerminalPolling } from './runState/useProjectRunnerTerminalPolling';
export type { ProjectRunnerActiveTerminal } from '../../lib/domain/projectRunner';

interface UseProjectExplorerRunStateParams {
  client: ApiClient;
  project: Project | null;
  enabled: boolean;
}

export const useProjectExplorerRunState = ({
  client,
  project,
  enabled,
}: UseProjectExplorerRunStateParams) => {
  const { t } = useI18n();
  const runnerCatalog = useProjectRunnerCatalogState({ client, project, enabled });
  const runnerTerminal = useProjectRunnerTerminalPolling({ client, project, enabled });
  const runnerCommands = useProjectRunnerCommands({
    client,
    project,
    runTargets: runnerCatalog.runTargets,
    selectedRunTargetId: runnerCatalog.selectedRunTargetId,
    commandPreview: runnerCatalog.commandPreview,
    activeRun: runnerTerminal.activeRun,
    projectRunTerminalIds: runnerTerminal.projectRunInstances.map((item) => item.terminalId),
    selectedTerminalId: runnerTerminal.selectedRunInstanceId,
    selectRunInstance: runnerTerminal.selectRunInstance,
    setActiveRun: runnerTerminal.setActiveRun,
    setLastExitedRun: runnerTerminal.setLastExitedRun,
    setActiveTerminalBusy: runnerTerminal.setActiveTerminalBusy,
    removeRunInstanceLocally: runnerTerminal.removeRunInstanceLocally,
    refreshProjectActiveRun: runnerTerminal.refreshProjectActiveRun,
  });

  useProjectRunnerExitInspection({
    lastExitedRun: runnerTerminal.lastExitedRun,
    lastExitCheckedRunKey: runnerCommands.lastExitCheckedRunKey,
    manualControlAt: runnerCommands.manualControlAt,
    onListTerminalLogs: client.listTerminalLogs.bind(client),
    setLastExitCheckedRunKey: runnerCommands.setLastExitCheckedRunKey,
    setRunnerError: runnerCommands.setRunnerError,
    setRunnerDiagnosis: runnerCommands.setRunnerDiagnosis,
    setRunnerMessage: runnerCommands.setRunnerMessage,
  });

  const selectedRunTarget = useMemo(
    () => runnerCatalog.runTargets.find((item) => item.id === runnerCatalog.selectedRunTargetId)
      || runnerCatalog.runTargets[0]
      || null,
    [runnerCatalog.runTargets, runnerCatalog.selectedRunTargetId],
  );

  const runnerSuggestions = useMemo(() => buildProjectRunResolutionSuggestions({
    diagnosis: runnerCommands.runnerDiagnosis || runnerCommands.runnerError,
    selectedTarget: selectedRunTarget,
    runTargets: runnerCatalog.runTargets,
    selectedToolchainOptions: runnerCatalog.selectedToolchainOptions,
    availableOptionsByKind: runnerCatalog.runEnvironment?.optionsByKind || {},
    t,
  }), [
    runnerCatalog.runEnvironment?.optionsByKind,
    runnerCatalog.runTargets,
    runnerCatalog.selectedToolchainOptions,
    runnerCommands.runnerDiagnosis,
    runnerCommands.runnerError,
    selectedRunTarget,
    t,
  ]);

  return {
    runStatus: runnerCatalog.runStatus,
    runTargets: runnerCatalog.runTargets,
    runCatalogLoading: runnerCatalog.runCatalogLoading,
    runCatalogError: runnerCatalog.runCatalogError,
    runEnvironment: runnerCatalog.runEnvironment,
    runEnvironmentLoading: runnerCatalog.runEnvironmentLoading,
    runEnvironmentError: runnerCatalog.runEnvironmentError,
    availableToolchainKinds: runnerCatalog.availableToolchainKinds,
    selectedToolchainOptions: runnerCatalog.selectedToolchainOptions,
    missingToolchainKinds: runnerCatalog.missingToolchainKinds,
    customToolchainDrafts: runnerCatalog.customToolchainDrafts,
    envVarsDraft: runnerCatalog.envVarsDraft,
    commandPreview: runnerCatalog.commandPreview,
    envPreview: runnerCatalog.envPreview,
    environmentHints: runnerCatalog.environmentHints,
    envVarsPlaceholder: runnerCatalog.envVarsPlaceholder,
    selectedRunTargetId: runnerCatalog.selectedRunTargetId,
    setSelectedRunTargetId: runnerCatalog.selectRunTarget,
    updateSelectedToolchain: runnerCatalog.updateSelectedToolchain,
    updateCustomToolchainDraft: runnerCatalog.updateCustomToolchainDraft,
    saveCustomToolchain: runnerCatalog.saveCustomToolchain,
    setEnvVarsDraft: runnerCatalog.setEnvVarsDraft,
    saveEnvVarsDraft: runnerCatalog.saveEnvVarsDraft,
    setTerminalUiEnabled: runnerCatalog.setTerminalUiEnabled,
    starting: runnerCommands.starting,
    stopping: runnerCommands.stopping,
    restarting: runnerCommands.restarting,
    deleting: runnerCommands.deleting,
    runnerMessage: runnerCommands.runnerMessage,
    runnerError: runnerCommands.runnerError,
    runnerDiagnosis: runnerCommands.runnerDiagnosis,
    runnerSuggestions,
    projectRunState: runnerTerminal.projectRunState,
    projectRunInstances: runnerTerminal.projectRunInstances,
    selectedRunInstanceId: runnerTerminal.selectedRunInstanceId,
    projectRunTerminal: runnerTerminal.projectRunTerminal,
    activeRun: runnerTerminal.activeRun,
    lastExitedRun: runnerTerminal.lastExitedRun,
    activeTerminalBusy: runnerTerminal.activeTerminalBusy,
    selectRunInstance: runnerTerminal.selectRunInstance,
    handleRunnerStart: runnerCommands.handleRunnerStart,
    handleRunnerStop: runnerCommands.handleRunnerStop,
    handleRunnerRestart: runnerCommands.handleRunnerRestart,
    handleRunnerDelete: runnerCommands.handleRunnerDelete,
    refreshRunnerState: async () => {
      await runnerCatalog.refreshRunnerState('analyze');
      await runnerTerminal.refreshProjectActiveRun();
    },
  };
};
