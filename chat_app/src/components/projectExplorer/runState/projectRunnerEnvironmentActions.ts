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

export const buildProjectRunEnvironmentUpdatePayload = ({
  selectedToolchains,
  customToolchains,
  envVars,
}: {
  selectedToolchains: Record<string, string>;
  customToolchains: Record<string, ProjectRunCustomToolchain>;
  envVars: Record<string, string>;
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
});
