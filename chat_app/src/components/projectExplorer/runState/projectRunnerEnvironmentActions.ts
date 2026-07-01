// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRunCustomToolchain,
  ProjectRunEnvironment,
} from '../../../types';
import { buildCustomToolchainSelectionState, parseEnvVarsDraft } from './projectRunnerEnvironmentState';

export const resolveSelectedToolchainEnvironment = ({
  environment,
  kind,
  optionId,
}: {
  environment: ProjectRunEnvironment | null;
  kind: string;
  optionId: string;
}): ProjectRunEnvironment | null => {
  if (!environment) {
    return null;
  }
  return {
    ...environment,
    selectedToolchains: {
      ...environment.selectedToolchains,
      [kind]: optionId,
    },
  };
};

export const resolveCustomToolchainEnvironment = ({
  environment,
  kind,
  draftPath,
}: {
  environment: ProjectRunEnvironment | null;
  kind: string;
  draftPath: string;
}): {
  nextEnvironment: ProjectRunEnvironment | null;
  nextSelectedToolchains: Record<string, string>;
  nextCustomToolchains: Record<string, ProjectRunCustomToolchain>;
} => {
  const nextSelection = buildCustomToolchainSelectionState(
    kind,
    draftPath,
    environment?.customToolchains || {},
    environment?.selectedToolchains || {},
  );

  return {
    nextEnvironment: environment ? {
      ...environment,
      selectedToolchains: nextSelection.selectedToolchains,
      customToolchains: nextSelection.customToolchains,
    } : null,
    nextSelectedToolchains: nextSelection.selectedToolchains,
    nextCustomToolchains: nextSelection.customToolchains,
  };
};

export const resolveEnvVarsEnvironment = ({
  environment,
  envVarsDraft,
}: {
  environment: ProjectRunEnvironment | null;
  envVarsDraft: string;
}): {
  nextEnvironment: ProjectRunEnvironment | null;
  nextEnvVars: Record<string, string>;
} => {
  const nextEnvVars = parseEnvVarsDraft(envVarsDraft);
  return {
    nextEnvironment: environment ? {
      ...environment,
      envVars: nextEnvVars,
    } : null,
    nextEnvVars,
  };
};

export const resolveTerminalUiEnvironment = ({
  environment,
  terminalUiEnabled,
}: {
  environment: ProjectRunEnvironment | null;
  terminalUiEnabled: boolean;
}): ProjectRunEnvironment | null => {
  if (!environment) {
    return null;
  }
  return {
    ...environment,
    terminalUiEnabled,
  };
};

export const buildProjectRunEnvironmentUpdatePayload = ({
  selectedToolchains,
  customToolchains,
  envVars,
  terminalUiEnabled,
}: {
  selectedToolchains: Record<string, string>;
  customToolchains: Record<string, ProjectRunCustomToolchain>;
  envVars: Record<string, string>;
  terminalUiEnabled: boolean;
}) => ({
  selected_toolchains: selectedToolchains,
  custom_toolchains: Object.fromEntries(
    Object.entries(customToolchains).map(([kind, value]) => [
      kind,
      {
        kind: value.kind,
        label: value.label,
        path: value.path,
      },
    ]),
  ),
  env_vars: envVars,
  terminal_ui_enabled: terminalUiEnabled,
});
