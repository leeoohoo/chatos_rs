import type {
  ProjectRunConfigFileSummary,
  ProjectRunEnvironment,
  ProjectRunInstance,
  ProjectRunResolutionSuggestion,
  ProjectRunState,
  ProjectRunTarget,
  ProjectRunToolchainOption,
  ProjectRunValidationIssue,
  Terminal,
} from '../../types';

export interface ProjectRunSettingsViewProps {
  projectName?: string;
  projectRootPath?: string;
  runStatus: string;
  runCatalogLoading: boolean;
  runEnvironment: ProjectRunEnvironment | null;
  runEnvironmentLoading: boolean;
  runEnvironmentError: string | null;
  configFiles: ProjectRunConfigFileSummary[];
  validationIssues: ProjectRunValidationIssue[];
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
  terminalUiEnabled: boolean;
  selectedRunTargetId: string | null;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  deleting: boolean;
  runnerMessage: string | null;
  runnerError: string | null;
  runnerDiagnosis: string | null;
  runnerSuggestions: ProjectRunResolutionSuggestion[];
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
  onTerminalUiEnabledChange: (enabled: boolean) => void;
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRunnerDelete: () => void;
  onRefreshRunnerState: () => void;
}
