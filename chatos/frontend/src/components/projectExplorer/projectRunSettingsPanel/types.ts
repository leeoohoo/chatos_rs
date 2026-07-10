// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRunEnvironment,
  ProjectRunInstance,
  ProjectRunResolutionSuggestion,
  ProjectRunState,
  ProjectRunTarget,
  ProjectRunToolchainOption,
  Terminal,
} from '../../../types';

export interface ProjectRunSettingsPanelProps {
  projectName?: string;
  projectRootPath?: string;
  runStatus: string;
  runCatalogLoading: boolean;
  runEnvironment: ProjectRunEnvironment | null;
  runEnvironmentLoading: boolean;
  runEnvironmentError: string | null;
  configFiles: Array<{
    kind: string;
    label: string;
    path: string;
    preview?: string | null;
    source: string;
  }>;
  validationIssues: Array<{
    kind: string;
    message: string;
    targetId?: string | null;
    targetLabel?: string | null;
    path?: string | null;
    hint?: string | null;
  }>;
  runTargets: ProjectRunTarget[];
  availableToolchainKinds: string[];
  selectedToolchainOptions: Record<string, ProjectRunToolchainOption | null>;
  missingToolchainKinds: string[];
  customToolchainDrafts: Record<string, string>;
  envVarsDraft: string;
  commandPreview: string;
  envPreview: string;
  environmentHints: string[];
  envVarsPlaceholder: string;
  sandboxToggleVisible: boolean;
  sandboxEnabled: boolean | null;
  sandboxLoading: boolean;
  sandboxSaving: boolean;
  sandboxError: string | null;
  showTerminalUi: boolean;
  selectedRunTargetId: string | null;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  deleting: boolean;
  runnerMessage?: string | null;
  runnerError?: string | null;
  runnerDiagnosis?: string | null;
  runnerSuggestions?: ProjectRunResolutionSuggestion[];
  projectRunState: ProjectRunState | null;
  projectRunInstances: ProjectRunInstance[];
  selectedRunInstanceId: string | null;
  projectRunTerminal: Terminal | null;
  projectRunTerminalBusy: boolean;
  onSelectRunTarget: (targetId: string) => void;
  onSelectRunInstance: (terminalId: string | null) => void;
  onSelectToolchain: (kind: string, optionId: string) => void;
  onApplySuggestion: (suggestion: ProjectRunResolutionSuggestion) => void;
  onCustomToolchainDraftChange: (kind: string, value: string) => void;
  onSaveCustomToolchain: (kind: string) => void;
  onEnvVarsDraftChange: (value: string) => void;
  onSaveEnvVarsDraft: () => void;
  onSandboxEnabledChange: (enabled: boolean) => void;
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRunnerDelete: () => void;
  onRefreshRunnerState: () => void;
}
