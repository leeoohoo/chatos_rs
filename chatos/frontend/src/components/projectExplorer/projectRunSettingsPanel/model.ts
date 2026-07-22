// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../../i18n/I18nProvider';
import type { ProjectRunTarget } from '../../../types';

export const TOOLCHAIN_LABELS: Record<string, string> = {
  java_home: 'JDK',
  java: 'Java',
  mvn: 'Maven',
  mvn_settings: 'Maven Settings',
  gradle: 'Gradle',
  gradle_user_home: 'Gradle User Home',
  cargo: 'Cargo',
  rustc: 'Rustc',
  go: 'Go',
  node: 'Node.js',
  npm: 'npm',
  pnpm: 'pnpm',
  yarn: 'yarn',
  python: 'Python',
};

export const TOOLCHAIN_AUTO_ENV_KEYS: Record<string, string> = {
  java_home: 'JAVA_HOME',
  mvn: 'MVN_BIN',
  mvn_settings: 'MVN_SETTINGS',
  gradle: 'GRADLE_BIN',
  gradle_user_home: 'GRADLE_USER_HOME',
  python: 'PYTHON_BIN',
  node: 'NODE_BIN',
  cargo: 'CARGO_BIN',
  go: 'GO_BIN',
};

export const TOOLCHAIN_DISPLAY_NAMES: Record<string, string> = {
  java_home: 'JDK',
  mvn: 'Maven',
  mvn_settings: 'Maven Settings',
  gradle: 'Gradle',
  gradle_user_home: 'Gradle User Home',
  python: 'Python',
  node: 'Node.js',
  npm: 'npm',
  pnpm: 'pnpm',
  yarn: 'yarn',
  cargo: 'Cargo',
  rustc: 'Rustc',
  go: 'Go',
};

export const TARGET_CONFIG_KIND_MAP: Record<string, string[]> = {
  java: ['maven_config', 'maven_jvm_config', 'gradle_properties', 'gradle_user_properties'],
  node: ['package_json', 'node_lockfile', 'node_workspace', 'node_runtime_config'],
  python: ['python_manifest', 'python_runtime_config'],
  go: ['go_manifest'],
  rust: ['cargo_manifest', 'cargo_runtime_config', 'cargo_toolchain'],
};

export const getRunStatusLabel = (status: string, t: TranslateFn): string => {
  const key = `runSettings.status.${status}`;
  const label = t(key);
  return label === key ? status : label;
};

export const getRunStatusTone = (status: string): string => {
  if (status === 'ready') return 'text-emerald-700 border-emerald-500/30 bg-emerald-500/10';
  if (status === 'error') return 'text-destructive border-destructive/30 bg-destructive/10';
  return 'text-muted-foreground border-border bg-background';
};

export const getSandboxStatusText = (
  loading: boolean,
  saving: boolean,
  enabled: boolean | null,
  t: TranslateFn,
): string => {
  if (loading) return t('runSettings.sandboxLoading');
  if (saving) return t('runSettings.sandboxSaving');
  return t(enabled ? 'runSettings.sandboxEnabled' : 'runSettings.sandboxDisabled');
};

export const formatRunTargetSource = (target: ProjectRunTarget, t: TranslateFn): string => {
  const kind = (target.kind || '').trim();
  const entrypoint = (target.entrypoint || '').trim();
  const manifestPath = (target.manifestPath || '').trim();

  if (kind === 'node') {
    if (entrypoint.startsWith('package.json:scripts.')) {
      return t('runSettings.source.packageScript', { script: entrypoint.replace('package.json:scripts.', '') });
    }
    return manifestPath ? t('runSettings.source.packageJson') : t('runSettings.source.nodeAuto');
  }
  if (kind === 'python') {
    if (entrypoint.endsWith('.py')) {
      return t('runSettings.source.pythonScript', { entrypoint });
    }
    if (target.command.includes('pytest')) {
      return t('runSettings.source.pythonTest');
    }
    return manifestPath ? t('runSettings.source.pythonManifest') : t('runSettings.source.pythonAuto');
  }
  if (kind === 'go') {
    if (entrypoint.startsWith('./cmd/')) {
      return t('runSettings.source.goCmd', { entrypoint });
    }
    if (entrypoint === '.') {
      return t('runSettings.source.goRootMain');
    }
    return manifestPath ? t('runSettings.source.goMod') : t('runSettings.source.goAuto');
  }
  if (kind === 'rust') {
    if (entrypoint.startsWith('src/bin/')) {
      return t('runSettings.source.rustBin', { entrypoint });
    }
    if (entrypoint === 'src/main.rs') {
      return t('runSettings.source.rustDefaultMain');
    }
    return manifestPath ? t('runSettings.source.cargoManifest') : t('runSettings.source.rustAuto');
  }
  if (kind === 'java') {
    if (entrypoint) {
      return t('runSettings.source.javaMain', { entrypoint });
    }
    if (manifestPath.endsWith('pom.xml')) {
      return t('runSettings.source.mavenProject');
    }
    if (manifestPath) {
      return t('runSettings.source.gradleProject');
    }
  }
  return target.source === 'auto' ? t('runSettings.source.auto') : t('runSettings.source.generic', { source: target.source });
};

export const formatRunTargetOptionHint = (target: ProjectRunTarget, t: TranslateFn): string => {
  const kind = (target.kind || '').trim();
  const entrypoint = (target.entrypoint || '').trim();

  if (kind === 'node' && entrypoint.startsWith('package.json:scripts.')) {
    return t('runSettings.option.script', { script: entrypoint.replace('package.json:scripts.', '') });
  }
  if (kind === 'python' && entrypoint) {
    return entrypoint;
  }
  if (kind === 'go' && entrypoint) {
    return entrypoint === '.' ? t('runSettings.option.goRootMain') : entrypoint;
  }
  if (kind === 'rust' && entrypoint.startsWith('src/bin/')) {
    return entrypoint.replace('src/bin/', '').replace('/main.rs', '').replace('.rs', '');
  }
  if (kind === 'rust' && entrypoint === 'src/main.rs') {
    return t('runSettings.option.rustDefaultMain');
  }
  if (kind === 'java' && entrypoint) {
    return entrypoint;
  }
  return target.command;
};

export const resolveTargetDisplayName = (kind?: string | null): string => {
  switch ((kind || '').trim()) {
    case 'java':
      return 'Java';
    case 'rust':
      return 'Rust';
    case 'python':
      return 'Python';
    case 'node':
      return 'Node.js';
    case 'go':
      return 'Go';
    default:
      return '';
  }
};

export const formatToolchainKind = (kind: string): string => TOOLCHAIN_LABELS[kind] || kind;

export const formatToolchainSource = (
  source: string | null | undefined,
  t: TranslateFn,
): string => {
  switch ((source || '').trim()) {
    case 'sandbox':
      return t('runSettings.toolchainSource.sandbox');
    case 'project-local':
      return t('runSettings.toolchainSource.projectLocal');
    case 'env':
      return t('runSettings.toolchainSource.env');
    case 'path':
      return 'PATH';
    case 'system':
      return t('runSettings.toolchainSource.system');
    case 'manual':
      return t('runSettings.toolchainSource.manual');
    default:
      return t('runSettings.toolchainSource.auto');
  }
};

export const resolveManualHint = (kind: string, t: TranslateFn): string => {
  if (kind === 'mvn_settings') {
    return t('runSettings.manualHint.mavenSettings');
  }
  if (kind === 'gradle_user_home') {
    return t('runSettings.manualHint.gradleHome');
  }
  return t('runSettings.manualHint.default');
};

export const resolveConfigFilesEmptyText = (
  target: ProjectRunTarget | null,
  toolchainKinds: string[],
  t: TranslateFn,
): string => {
  const projectLabel = resolveTargetDisplayName(target?.kind);

  const toolchainLabels = toolchainKinds
    .map((kind) => TOOLCHAIN_DISPLAY_NAMES[kind] || formatToolchainKind(kind))
    .filter((value, index, list) => Boolean(value) && list.indexOf(value) === index);

  if (projectLabel && toolchainLabels.length > 0) {
    return t('runSettings.config.emptyWithToolchain', { project: projectLabel, toolchains: toolchainLabels.join(' / ') });
  }
  if (projectLabel) {
    return t('runSettings.config.emptyProject', { project: projectLabel });
  }
  return t('runSettings.config.emptyDefault');
};

export const resolveConfigSectionTitle = (
  target: ProjectRunTarget | null,
  t: TranslateFn,
): string => {
  switch ((target?.kind || '').trim()) {
    case 'java':
      return t('runSettings.config.section.java');
    case 'rust':
      return t('runSettings.config.section.rust');
    case 'python':
      return t('runSettings.config.section.python');
    case 'node':
      return t('runSettings.config.section.node');
    case 'go':
      return t('runSettings.config.section.go');
    default:
      return t('runSettings.config.section.default');
  }
};

export const resolveConfigKindsForTarget = (target: ProjectRunTarget | null): string[] => {
  if (!target) {
    return [];
  }
  const targetKind = (target.kind || '').trim();
  const command = (target.command || '').trim().toLowerCase();
  if (targetKind === 'java') {
    if (
      command.startsWith('mvn ')
      || command === 'mvn'
      || command.startsWith('./mvnw ')
      || command === './mvnw'
    ) {
      return ['maven_config', 'maven_jvm_config'];
    }
    if (
      command.startsWith('gradle ')
      || command === 'gradle'
      || command.startsWith('./gradlew ')
      || command === './gradlew'
    ) {
      return ['gradle_properties', 'gradle_user_properties'];
    }
  }
  return TARGET_CONFIG_KIND_MAP[targetKind] || [];
};

export const buildInjectedEnvHint = (toolchainKinds: string[], t: TranslateFn): string => {
  const keys = toolchainKinds
    .map((kind) => TOOLCHAIN_AUTO_ENV_KEYS[kind])
    .filter(Boolean)
    .slice(0, 3);
  if (keys.length === 0) {
    return t('runSettings.injectedEnv.none');
  }
  return t('runSettings.injectedEnv.some', { keys: keys.map((key) => `\`${key}\``).join(' / ') });
};
