// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ProjectRunSettingsViewProps } from './projectSettingsViewTypes';
import type { ProjectExplorerWorkspaceViewParams } from './workspaceViewTypes';

type Project = ProjectExplorerWorkspaceViewParams['project'];
type Run = ProjectExplorerWorkspaceViewParams['run'];

export const buildProjectRunSettingsProps = (
  project: Project,
  run: Run,
): ProjectRunSettingsViewProps => ({
  projectName: project.name,
  projectRootPath: project.rootPath,
  runStatus: run.runStatus,
  runCatalogLoading: run.runCatalogLoading,
  runEnvironment: run.runEnvironment,
  runEnvironmentLoading: run.runEnvironmentLoading,
  runEnvironmentError: run.runEnvironmentError,
  configFiles: run.runEnvironment?.configFiles || [],
  validationIssues: run.runEnvironment?.validationIssues || [],
  runTargets: run.runTargets,
  availableToolchainKinds: run.availableToolchainKinds,
  selectedToolchainOptions: run.selectedToolchainOptions,
  missingToolchainKinds: run.missingToolchainKinds,
  customToolchainDrafts: run.customToolchainDrafts,
  envVarsDraft: run.envVarsDraft,
  commandPreview: run.commandPreview,
  envPreview: run.envPreview,
  environmentHints: run.environmentHints,
  envVarsPlaceholder: run.envVarsPlaceholder,
  sandboxToggleVisible: run.sandboxToggleVisible,
  sandboxEnabled: run.sandboxEnabled,
  sandboxLoading: run.sandboxLoading,
  sandboxSaving: run.sandboxSaving,
  sandboxError: run.sandboxError,
  showTerminalUi: run.terminalUiEnabled,
  selectedRunTargetId: run.selectedRunTargetId,
  starting: run.starting,
  stopping: run.stopping,
  restarting: run.restarting,
  deleting: run.deleting,
  runnerMessage: run.runnerMessage,
  runnerError: run.runnerError,
  runnerDiagnosis: run.runnerDiagnosis,
  runnerSuggestions: run.runnerSuggestions,
  projectRunState: run.projectRunState,
  projectRunInstances: run.projectRunInstances,
  selectedRunInstanceId: run.selectedRunInstanceId,
  projectRunTerminal: run.projectRunTerminal,
  projectRunTerminalBusy: run.activeTerminalBusy,
  onSelectRunTarget: (targetId: string) => {
    void run.setSelectedRunTargetId(targetId);
  },
  onSelectRunInstance: run.selectRunInstance,
  onSelectToolchain: (kind: string, optionId: string) => {
    void run.updateSelectedToolchain(kind, optionId);
  },
  onApplySuggestion: (suggestion) => {
    if (suggestion.actionKind === 'switch_target' && suggestion.targetId) {
      void run.setSelectedRunTargetId(suggestion.targetId);
      return;
    }
    if (
      suggestion.actionKind === 'select_toolchain'
      && suggestion.toolchainKind
      && suggestion.optionId
    ) {
      void run.updateSelectedToolchain(suggestion.toolchainKind, suggestion.optionId);
    }
  },
  onCustomToolchainDraftChange: run.updateCustomToolchainDraft,
  onSaveCustomToolchain: (kind: string) => {
    void run.saveCustomToolchain(kind);
  },
  onEnvVarsDraftChange: run.setEnvVarsDraft,
  onSaveEnvVarsDraft: () => {
    void run.saveEnvVarsDraft();
  },
  onSandboxEnabledChange: (enabled: boolean) => {
    void run.updateSandboxEnabled(enabled);
  },
  onRunnerStart: run.handleRunnerStart,
  onRunnerStop: run.handleRunnerStop,
  onRunnerRestart: run.handleRunnerRestart,
  onRunnerDelete: run.handleRunnerDelete,
  onRefreshRunnerState: run.refreshRunnerState,
});
