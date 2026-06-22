import type { TranslateFn } from '../../../i18n/I18nProvider';
import type {
  ProjectRunResolutionSuggestion,
  ProjectRunTarget,
  ProjectRunToolchainOption,
} from '../../../types';

const fallbackTranslate: TranslateFn = (key, params) => {
  const values = params || {};
  switch (key) {
    case 'runSettings.toolchain.rustc':
      return 'Rust compiler';
    case 'runSettings.suggestion.switchOtherJdk':
      return `Switch to another JDK: ${values.label}`;
    case 'runSettings.suggestion.checkJdk':
      return 'Check and switch JDK version';
    case 'runSettings.suggestion.selectValidJdk':
      return 'Select a valid JDK directory again';
    case 'runSettings.suggestion.switchMavenSettings':
      return `Switch Maven Settings: ${values.label}`;
    case 'runSettings.suggestion.checkMavenSettings':
      return 'Check Maven Settings file';
    case 'runSettings.suggestion.reviewMavenSettings':
      return 'Check Maven Settings and repository credentials first';
    case 'runSettings.suggestion.switchOtherEntrypoint':
      return `Switch to another run entry: ${values.label}`;
    case 'runSettings.suggestion.switchNoGradlew':
      return 'Switch to a run entry that does not depend on gradlew';
    case 'runSettings.suggestion.tryOtherEntrypoint':
      return `Try switching to another entry: ${values.label}`;
    case 'runSettings.suggestion.checkEntrypoint':
      return 'Check and switch run entry';
    case 'runSettings.suggestion.trySiblingTarget':
      return `Try another entry for the same language: ${values.label}`;
    case 'runSettings.suggestion.switchRustEntrypoint':
      return `Switch to another Rust entry: ${values.label}`;
    case 'runSettings.suggestion.checkRustEntrypoint':
      return 'Check Rust entrypoint and Cargo configuration';
    case 'runSettings.suggestion.reviewRustToolchain':
      return 'Check Cargo / Rust build environment';
    case 'runSettings.suggestion.switchGoEntrypoint':
      return `Switch to another Go entry: ${values.label}`;
    case 'runSettings.suggestion.checkGoEntrypoint':
      return 'Check Go entrypoint and go.mod configuration';
    case 'runSettings.suggestion.selectPython':
      return 'Switch or check Python interpreter';
    case 'runSettings.suggestion.tryPortTarget':
      return `Try another run entry first: ${values.label}`;
    case 'runSettings.suggestion.checkPort':
      return 'Check port usage or change project port configuration';
    case 'runSettings.suggestion.switchNodeEntrypoint':
      return `Switch to another Node.js entry: ${values.label}`;
    case 'runSettings.suggestion.checkNodeEntrypoint':
      return 'Check script command, port, or frontend entry configuration';
    case 'runSettings.suggestion.switchToolchain':
      return `Switch ${values.toolchain} to another discovered environment`;
    case 'runSettings.suggestion.selectToolchain':
      return `Select an available environment for ${values.toolchain}`;
    default:
      return key;
  }
};

const TOOLCHAIN_LABELS: Record<string, string> = {
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

const formatToolchainLabel = (kind: string, t: TranslateFn): string => (
  kind === 'rustc' ? t('runSettings.toolchain.rustc') : TOOLCHAIN_LABELS[kind] || kind
);

const buildToolchainSuggestion = (
  id: string,
  label: string,
  toolchainKind: string,
  option: ProjectRunToolchainOption | null | undefined,
): ProjectRunResolutionSuggestion | null => {
  if (!option?.id) {
    return null;
  }
  return {
    id,
    label,
    detail: option.path || option.label || null,
    actionKind: 'select_toolchain',
    toolchainKind,
    optionId: option.id,
  };
};

const buildTargetSuggestion = (
  id: string,
  label: string,
  target: ProjectRunTarget | null | undefined,
): ProjectRunResolutionSuggestion | null => {
  if (!target?.id) {
    return null;
  }
  return {
    id,
    label,
    detail: target.entrypoint || target.command || null,
    actionKind: 'switch_target',
    targetId: target.id,
  };
};

export const buildProjectRunResolutionSuggestions = ({
  diagnosis,
  selectedTarget,
  runTargets,
  selectedToolchainOptions,
  availableOptionsByKind,
  t = fallbackTranslate,
}: {
  diagnosis: string | null;
  selectedTarget: ProjectRunTarget | null;
  runTargets: ProjectRunTarget[];
  selectedToolchainOptions: Record<string, ProjectRunToolchainOption | null>;
  availableOptionsByKind: Record<string, ProjectRunToolchainOption[]>;
  t?: TranslateFn;
}): ProjectRunResolutionSuggestion[] => {
  const text = (diagnosis || '').trim().toLowerCase();
  if (!text) {
    return [];
  }

  const suggestions: ProjectRunResolutionSuggestion[] = [];
  const selectedLanguage = (selectedTarget?.language || selectedTarget?.kind || '').trim().toLowerCase();
  const siblingTargets = runTargets.filter((item) => (
    item.id !== selectedTarget?.id
    && String(item.language || item.kind || '').trim().toLowerCase() === selectedLanguage
  ));

  const maybePush = (value: ProjectRunResolutionSuggestion | null) => {
    if (!value) {
      return;
    }
    if (suggestions.some((item) => item.id === value.id)) {
      return;
    }
    suggestions.push(value);
  };
  const includesFailure = (key: string) => text.includes(t(key).toLowerCase());

  const javaHomeOptions = availableOptionsByKind.java_home || [];
  const currentJavaHome = selectedToolchainOptions.java_home;
  const alternativeJavaHome = javaHomeOptions.find((item) => item.id !== currentJavaHome?.id) || null;

  if (
    includesFailure('runSettings.failure.jdkMismatch')
    || /invalid target release|unsupported class file major version|release version .* not supported|source option .* no longer supported|target option .* no longer supported/i.test(text)
  ) {
    maybePush(buildToolchainSuggestion(
      'switch-java-home',
      alternativeJavaHome ? t('runSettings.suggestion.switchOtherJdk', { label: alternativeJavaHome.label }) : t('runSettings.suggestion.checkJdk'),
      'java_home',
      alternativeJavaHome || currentJavaHome,
    ));
  }

  if (/java_home|jdk.*bin\/java|jdk\/jre root/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'select-java-home',
      t('runSettings.suggestion.selectValidJdk'),
      'java_home',
      alternativeJavaHome || currentJavaHome,
    ));
  }

  const mavenSettingsOptions = availableOptionsByKind.mvn_settings || [];
  const currentMavenSettings = selectedToolchainOptions.mvn_settings;
  const alternativeMavenSettings = mavenSettingsOptions.find((item) => item.id !== currentMavenSettings?.id) || null;
  if (/maven settings|settings\.xml|non-parseable settings/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'switch-maven-settings',
      alternativeMavenSettings ? t('runSettings.suggestion.switchMavenSettings', { label: alternativeMavenSettings.label }) : t('runSettings.suggestion.checkMavenSettings'),
      'mvn_settings',
      alternativeMavenSettings || currentMavenSettings,
    ));
  }

  if (includesFailure('runSettings.failure.mavenDependencies') || /could not resolve dependencies|transfer failed|authentication failed|proxy/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'review-maven-settings',
      t('runSettings.suggestion.reviewMavenSettings'),
      'mvn_settings',
      currentMavenSettings || alternativeMavenSettings,
    ));
  }

  if (includesFailure('runSettings.failure.gradleWrapperPermission') || /gradlew.*permission denied/i.test(text)) {
    const gradleTarget = siblingTargets.find((item) => !item.command.includes('gradlew')) || null;
    maybePush(buildTargetSuggestion(
      'switch-gradle-target',
      gradleTarget ? t('runSettings.suggestion.switchOtherEntrypoint', { label: gradleTarget.label }) : t('runSettings.suggestion.switchNoGradlew'),
      gradleTarget,
    ));
  }

  if (/could not find or load main class|main method not found|no main manifest attribute/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-entrypoint',
      siblingTargets[0] ? t('runSettings.suggestion.tryOtherEntrypoint', { label: siblingTargets[0].label }) : t('runSettings.suggestion.checkEntrypoint'),
      siblingTargets[0] || null,
    ));
  }

  if ((includesFailure('runSettings.failure.longRunningExited') || text.includes(t('runSettings.exit.code', { code: '' }).toLowerCase().replace(/\s*$/, ''))) && siblingTargets.length > 0) {
    maybePush(buildTargetSuggestion(
      'switch-sibling-target',
      t('runSettings.suggestion.trySiblingTarget', { label: siblingTargets[0].label }),
      siblingTargets[0],
    ));
  }

  if (includesFailure('runSettings.failure.rustEntrypoint') || /no bin target named|a bin target must be available|no targets specified in the manifest/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-rust-target',
      siblingTargets[0] ? t('runSettings.suggestion.switchRustEntrypoint', { label: siblingTargets[0].label }) : t('runSettings.suggestion.checkRustEntrypoint'),
      siblingTargets[0] || null,
    ));
  }

  if (includesFailure('runSettings.failure.rustCompile') || /could not compile|error\[e\d+\]/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'review-rust-toolchain',
      t('runSettings.suggestion.reviewRustToolchain'),
      'cargo',
      (availableOptionsByKind.cargo || []).find((item) => item.id !== selectedToolchainOptions.cargo?.id)
        || selectedToolchainOptions.cargo,
    ));
  }

  if (includesFailure('runSettings.failure.goEntrypoint') || /no go files|go\.mod file not found|cannot find main module/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-go-target',
      siblingTargets[0] ? t('runSettings.suggestion.switchGoEntrypoint', { label: siblingTargets[0].label }) : t('runSettings.suggestion.checkGoEntrypoint'),
      siblingTargets[0] || null,
    ));
  }

  if (includesFailure('runSettings.failure.pythonRuntime') || /modulenotfounderror|no module named|pytest: command not found/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'select-python-runtime',
      t('runSettings.suggestion.selectPython'),
      'python',
      (availableOptionsByKind.python || []).find((item) => item.id !== selectedToolchainOptions.python?.id)
        || selectedToolchainOptions.python,
    ));
  }

  if (includesFailure('runSettings.failure.portInUse') || /eaddrinuse|address already in use/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-port-target',
      siblingTargets[0] ? t('runSettings.suggestion.tryPortTarget', { label: siblingTargets[0].label }) : t('runSettings.suggestion.checkPort'),
      siblingTargets[0] || null,
    ));
  }

  if (/missing script:|cannot find module|enoent|eaddrinuse/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-node-target',
      siblingTargets[0] ? t('runSettings.suggestion.switchNodeEntrypoint', { label: siblingTargets[0].label }) : t('runSettings.suggestion.checkNodeEntrypoint'),
      siblingTargets[0] || null,
    ));
  }

  if (/command not found|missing runtime|no such file or directory/i.test(text)) {
    const requiredKinds = selectedTarget?.requiredToolchains || [];
    for (const kind of requiredKinds) {
      const current = selectedToolchainOptions[kind];
      const alternative = (availableOptionsByKind[kind] || []).find((item) => item.id !== current?.id) || null;
      maybePush(buildToolchainSuggestion(
        `resolve-${kind}`,
        current
          ? t('runSettings.suggestion.switchToolchain', { toolchain: formatToolchainLabel(kind, t) })
          : t('runSettings.suggestion.selectToolchain', { toolchain: formatToolchainLabel(kind, t) }),
        kind,
        alternative || current,
      ));
    }
  }

  return suggestions.slice(0, 4);
};
