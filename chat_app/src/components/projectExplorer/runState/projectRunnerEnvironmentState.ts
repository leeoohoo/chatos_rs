// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../../i18n/I18nProvider';
import type {
  ProjectRunCustomToolchain,
  ProjectRunEnvironment,
  ProjectRunTarget,
  ProjectRunToolchainOption,
} from '../../../types';

const fallbackTranslate: TranslateFn = (key, params) => {
  const values = params || {};
  switch (key) {
    case 'runSettings.environmentHint.javaHome':
      return `JAVA_HOME=${values.path} will be injected before start`;
    case 'runSettings.environmentHint.mavenSettings':
      return `Maven commands will append -s ${values.path}`;
    case 'runSettings.environmentHint.mavenBin':
      return `System Maven commands will be replaced with ${values.path}`;
    case 'runSettings.environmentHint.gradleHome':
      return `GRADLE_USER_HOME=${values.path} will be injected for Gradle`;
    case 'runSettings.environmentHint.gradleBin':
      return `System Gradle commands will be replaced with ${values.path}`;
    case 'runSettings.environmentHint.python':
      return `Python commands will prefer ${values.path}`;
    case 'runSettings.environmentHint.node':
      return `Node commands will prefer ${values.path}`;
    case 'runSettings.environmentHint.npm':
      return `npm commands will prefer ${values.path}`;
    case 'runSettings.environmentHint.pnpm':
      return `pnpm commands will prefer ${values.path}`;
    case 'runSettings.environmentHint.yarn':
      return `yarn commands will prefer ${values.path}`;
    case 'runSettings.environmentHint.cargo':
      return `Cargo commands will prefer ${values.path}`;
    case 'runSettings.environmentHint.rustc':
      return `Compile steps will prefer ${values.path}`;
    case 'runSettings.environmentHint.go':
      return `Go commands will prefer ${values.path}`;
    case 'runSettings.manualToolchainLabel':
      return `Manual: ${values.name}`;
    default:
      return key;
  }
};

export const resolveCommandPreview = (
  command: string,
  selectedOptions: Record<string, ProjectRunToolchainOption | null>,
): string => {
  const trimmed = command.trim();
  if (!trimmed) {
    return '';
  }

  const replacements: Array<[string, string]> = [
    ['java', selectedOptions.java?.path || ''],
    ['mvn', selectedOptions.mvn?.path || ''],
    ['gradle', selectedOptions.gradle?.path || ''],
    ['cargo', selectedOptions.cargo?.path || ''],
    ['go', selectedOptions.go?.path || ''],
    ['node', selectedOptions.node?.path || ''],
    ['python', selectedOptions.python?.path || ''],
    ['python3', selectedOptions.python?.path || ''],
    ['npm', selectedOptions.npm?.path || ''],
    ['pnpm', selectedOptions.pnpm?.path || ''],
    ['yarn', selectedOptions.yarn?.path || ''],
  ];

  for (const [prefix, replacement] of replacements) {
    if (!replacement) {
      continue;
    }
    if (trimmed === prefix || trimmed.startsWith(`${prefix} `)) {
      return trimmed.replace(prefix, replacement);
    }
  }

  let resolved = trimmed;
  const javaHomePath = selectedOptions.java_home?.path || '';
  if (javaHomePath && (resolved === 'java' || resolved.startsWith('java '))) {
    resolved = resolved.replace('java', `${javaHomePath}/bin/java`);
  }
  const mvnSettingsPath = selectedOptions.mvn_settings?.path || '';
  if (mvnSettingsPath && !resolved.includes(' -s ') && !resolved.includes(' --settings ')) {
    if (
      resolved === 'mvn'
      || resolved.startsWith('mvn ')
      || resolved === './mvnw'
      || resolved.startsWith('./mvnw ')
    ) {
      const firstSpace = resolved.indexOf(' ');
      if (firstSpace < 0) {
        resolved = `${resolved} -s ${mvnSettingsPath}`;
      } else {
        resolved = `${resolved.slice(0, firstSpace)} -s ${mvnSettingsPath} ${resolved.slice(firstSpace + 1)}`.trim();
      }
    }
  }

  return resolved;
};

export const buildCustomToolchainDrafts = (
  environment: ProjectRunEnvironment | null,
  requiredKinds: string[],
): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const kind of requiredKinds) {
    out[kind] = environment?.customToolchains[kind]?.path || '';
  }
  return out;
};

export const serializeEnvVarsDraft = (envVars: Record<string, string>): string => (
  Object.entries(envVars)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `${key}=${value}`)
    .join('\n')
);

export const parseEnvVarsDraft = (draft: string): Record<string, string> => {
  const out: Record<string, string> = {};
  draft
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => Boolean(line) && !line.startsWith('#'))
    .forEach((line) => {
      const eqIndex = line.indexOf('=');
      if (eqIndex <= 0) {
        return;
      }
      const key = line.slice(0, eqIndex).trim();
      const value = line.slice(eqIndex + 1).trim();
      if (!key) {
        return;
      }
      out[key] = value;
    });
  return out;
};

export const buildEnvPreview = (
  envVars: Record<string, string>,
  selectedOptions: Record<string, ProjectRunToolchainOption | null>,
): string => {
  const nextEnv = { ...envVars };

  const javaHome = selectedOptions.java_home?.path || '';
  if (javaHome) {
    nextEnv.JAVA_HOME = javaHome;
  }
  const mvnBin = selectedOptions.mvn?.path || '';
  if (mvnBin) {
    nextEnv.MVN_BIN = mvnBin;
  }
  const mvnSettings = selectedOptions.mvn_settings?.path || '';
  if (mvnSettings) {
    nextEnv.MVN_SETTINGS = mvnSettings;
  }
  const gradleBin = selectedOptions.gradle?.path || '';
  if (gradleBin) {
    nextEnv.GRADLE_BIN = gradleBin;
  }
  const gradleUserHome = selectedOptions.gradle_user_home?.path || '';
  if (gradleUserHome) {
    nextEnv.GRADLE_USER_HOME = gradleUserHome;
  }
  const pythonBin = selectedOptions.python?.path || '';
  if (pythonBin) {
    nextEnv.PYTHON_BIN = pythonBin;
  }
  const nodeBin = selectedOptions.node?.path || '';
  if (nodeBin) {
    nextEnv.NODE_BIN = nodeBin;
  }
  const cargoBin = selectedOptions.cargo?.path || '';
  if (cargoBin) {
    nextEnv.CARGO_BIN = cargoBin;
  }
  const goBin = selectedOptions.go?.path || '';
  if (goBin) {
    nextEnv.GO_BIN = goBin;
  }

  return serializeEnvVarsDraft(nextEnv);
};

export const buildEnvironmentHints = (
  target: ProjectRunTarget | null,
  selectedOptions: Record<string, ProjectRunToolchainOption | null>,
  t: TranslateFn = fallbackTranslate,
): string[] => {
  if (!target) {
    return [];
  }

  const hints: string[] = [];
  if (target.kind === 'java') {
    if (selectedOptions.java_home?.path) {
      hints.push(t('runSettings.environmentHint.javaHome', { path: selectedOptions.java_home.path }));
    }
    if (selectedOptions.mvn_settings?.path) {
      hints.push(t('runSettings.environmentHint.mavenSettings', { path: selectedOptions.mvn_settings.path }));
    }
    if (selectedOptions.mvn?.path) {
      hints.push(t('runSettings.environmentHint.mavenBin', { path: selectedOptions.mvn.path }));
    }
    if (selectedOptions.gradle_user_home?.path) {
      hints.push(t('runSettings.environmentHint.gradleHome', { path: selectedOptions.gradle_user_home.path }));
    }
    if (selectedOptions.gradle?.path) {
      hints.push(t('runSettings.environmentHint.gradleBin', { path: selectedOptions.gradle.path }));
    }
  }
  if (target.kind === 'python' && selectedOptions.python?.path) {
    hints.push(t('runSettings.environmentHint.python', { path: selectedOptions.python.path }));
  }
  if (target.kind === 'node' && selectedOptions.node?.path) {
    hints.push(t('runSettings.environmentHint.node', { path: selectedOptions.node.path }));
  }
  if (target.kind === 'node' && selectedOptions.npm?.path) {
    hints.push(t('runSettings.environmentHint.npm', { path: selectedOptions.npm.path }));
  }
  if (target.kind === 'node' && selectedOptions.pnpm?.path) {
    hints.push(t('runSettings.environmentHint.pnpm', { path: selectedOptions.pnpm.path }));
  }
  if (target.kind === 'node' && selectedOptions.yarn?.path) {
    hints.push(t('runSettings.environmentHint.yarn', { path: selectedOptions.yarn.path }));
  }
  if (target.kind === 'rust') {
    if (selectedOptions.cargo?.path) {
      hints.push(t('runSettings.environmentHint.cargo', { path: selectedOptions.cargo.path }));
    }
    if (selectedOptions.rustc?.path) {
      hints.push(t('runSettings.environmentHint.rustc', { path: selectedOptions.rustc.path }));
    }
  }
  if (target.kind === 'go' && selectedOptions.go?.path) {
    hints.push(t('runSettings.environmentHint.go', { path: selectedOptions.go.path }));
  }

  return hints;
};

export const buildEnvVarsPlaceholder = (target: ProjectRunTarget | null): string => {
  if (!target) {
    return 'EXAMPLE_KEY=value';
  }
  if (target.kind === 'java') {
    if (target.command.includes('gradle') || target.command.includes('gradlew')) {
      return 'JAVA_OPTS=-Xmx2g\nGRADLE_OPTS=-Dorg.gradle.daemon=false\nSPRING_PROFILES_ACTIVE=dev';
    }
    return 'JAVA_OPTS=-Xmx2g\nMAVEN_OPTS=-Dmaven.repo.local=.m2/repository\nSPRING_PROFILES_ACTIVE=dev';
  }
  if (target.kind === 'python') {
    return 'PYTHONUNBUFFERED=1\nAPP_ENV=dev';
  }
  if (target.kind === 'node') {
    return 'NODE_ENV=development\nPORT=3000';
  }
  if (target.kind === 'rust') {
    return 'RUST_LOG=info\nAPP_ENV=dev';
  }
  if (target.kind === 'go') {
    return 'GO_ENV=development\nPORT=8080';
  }
  return 'APP_ENV=development\nPORT=3000';
};

export const buildSelectedToolchainOptions = (
  environment: ProjectRunEnvironment | null,
  availableToolchainKinds: string[],
): Record<string, ProjectRunToolchainOption | null> => {
  const out: Record<string, ProjectRunToolchainOption | null> = {};
  for (const kind of availableToolchainKinds) {
    const options = environment?.optionsByKind[kind] || [];
    const selectedId = environment?.selectedToolchains[kind];
    out[kind] = options.find((item) => item.id === selectedId) || options[0] || null;
  }
  return out;
};

export const buildMissingToolchainKinds = (
  availableToolchainKinds: string[],
  environment: ProjectRunEnvironment | null,
): string[] => (
  availableToolchainKinds.filter((kind) => (environment?.optionsByKind[kind] || []).length === 0)
);

export const buildCustomToolchainSelectionState = (
  normalizedKind: string,
  draftPath: string,
  customToolchains: Record<string, ProjectRunCustomToolchain>,
  selectedToolchains: Record<string, string>,
): {
  selectedOptionId: string;
  customToolchains: Record<string, ProjectRunCustomToolchain>;
  selectedToolchains: Record<string, string>;
} => {
  const nextSelectedOptionId = `${normalizedKind}:${draftPath}`;
  const nextCustomToolchains = {
    ...customToolchains,
    [normalizedKind]: {
      kind: normalizedKind,
      label: fallbackTranslate('runSettings.manualToolchainLabel', {
        name: draftPath.split('/').filter(Boolean).slice(-2).join('/') || draftPath,
      }),
      path: draftPath,
    },
  };
  const nextSelectedToolchains = {
    ...selectedToolchains,
    [normalizedKind]: nextSelectedOptionId,
  };

  return {
    selectedOptionId: nextSelectedOptionId,
    customToolchains: nextCustomToolchains,
    selectedToolchains: nextSelectedToolchains,
  };
};
