import type {
  ProjectRunResolutionSuggestion,
  ProjectRunTarget,
  ProjectRunToolchainOption,
} from '../../../types';

const TOOLCHAIN_LABELS: Record<string, string> = {
  java_home: 'JDK',
  java: 'Java',
  mvn: 'Maven',
  mvn_settings: 'Maven Settings',
  gradle: 'Gradle',
  gradle_user_home: 'Gradle User Home',
  cargo: 'Cargo',
  rustc: 'Rust 编译器',
  go: 'Go',
  node: 'Node.js',
  npm: 'npm',
  pnpm: 'pnpm',
  yarn: 'yarn',
  python: 'Python',
};

const formatToolchainLabel = (kind: string): string => TOOLCHAIN_LABELS[kind] || kind;

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
}: {
  diagnosis: string | null;
  selectedTarget: ProjectRunTarget | null;
  runTargets: ProjectRunTarget[];
  selectedToolchainOptions: Record<string, ProjectRunToolchainOption | null>;
  availableOptionsByKind: Record<string, ProjectRunToolchainOption[]>;
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

  const javaHomeOptions = availableOptionsByKind.java_home || [];
  const currentJavaHome = selectedToolchainOptions.java_home;
  const alternativeJavaHome = javaHomeOptions.find((item) => item.id !== currentJavaHome?.id) || null;

  if (
    /jdk.*不匹配|invalid target release|unsupported class file major version|release version .* not supported|source option .* no longer supported|target option .* no longer supported/i.test(text)
  ) {
    maybePush(buildToolchainSuggestion(
      'switch-java-home',
      alternativeJavaHome ? `切换到其它 JDK：${alternativeJavaHome.label}` : '检查并切换 JDK 版本',
      'java_home',
      alternativeJavaHome || currentJavaHome,
    ));
  }

  if (/java_home|jdk 目录下未发现 bin\/java|选择的是 jdk\/jre 根目录/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'select-java-home',
      '重新选择有效的 JDK 目录',
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
      alternativeMavenSettings ? `切换 Maven Settings：${alternativeMavenSettings.label}` : '检查 Maven Settings 文件',
      'mvn_settings',
      alternativeMavenSettings || currentMavenSettings,
    ));
  }

  if (/maven 依赖下载失败|could not resolve dependencies|transfer failed|authentication failed|proxy/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'review-maven-settings',
      '优先检查 Maven Settings 与仓库认证',
      'mvn_settings',
      currentMavenSettings || alternativeMavenSettings,
    ));
  }

  if (/gradle wrapper 没有执行权限|gradlew.*permission denied/i.test(text)) {
    const gradleTarget = siblingTargets.find((item) => !item.command.includes('gradlew')) || null;
    maybePush(buildTargetSuggestion(
      'switch-gradle-target',
      gradleTarget ? `切换到另一个运行入口：${gradleTarget.label}` : '切换到不依赖 gradlew 的运行入口',
      gradleTarget,
    ));
  }

  if (/could not find or load main class|main method not found|no main manifest attribute/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-entrypoint',
      siblingTargets[0] ? `尝试切换到其它入口：${siblingTargets[0].label}` : '检查并切换运行入口',
      siblingTargets[0] || null,
    ));
  }

  if (/命令已退出，未检测到持续运行进程|进程已退出，退出码/i.test(text) && siblingTargets.length > 0) {
    maybePush(buildTargetSuggestion(
      'switch-sibling-target',
      `尝试另一个同语言入口：${siblingTargets[0].label}`,
      siblingTargets[0],
    ));
  }

  if (/rust 可执行入口不存在或 bin 名称不匹配|no bin target named|a bin target must be available|no targets specified in the manifest/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-rust-target',
      siblingTargets[0] ? `切换到另一个 Rust 入口：${siblingTargets[0].label}` : '检查 Rust 入口与 Cargo 配置',
      siblingTargets[0] || null,
    ));
  }

  if (/rust 编译失败|could not compile|error\[e\d+\]/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'review-rust-toolchain',
      '检查 Cargo / Rust 编译环境',
      'cargo',
      (availableOptionsByKind.cargo || []).find((item) => item.id !== selectedToolchainOptions.cargo?.id)
        || selectedToolchainOptions.cargo,
    ));
  }

  if (/go 入口或模块配置有误|no go files|go\.mod file not found|cannot find main module/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-go-target',
      siblingTargets[0] ? `切换到另一个 Go 入口：${siblingTargets[0].label}` : '检查 Go 入口与 go.mod 配置',
      siblingTargets[0] || null,
    ));
  }

  if (/python 解释器或依赖环境有误|modulenotfounderror|no module named|pytest: command not found/i.test(text)) {
    maybePush(buildToolchainSuggestion(
      'select-python-runtime',
      '切换或检查 Python 解释器',
      'python',
      (availableOptionsByKind.python || []).find((item) => item.id !== selectedToolchainOptions.python?.id)
        || selectedToolchainOptions.python,
    ));
  }

  if (/端口已被占用|eaddrinuse|address already in use/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-port-target',
      siblingTargets[0] ? `先尝试另一个运行入口：${siblingTargets[0].label}` : '检查端口占用或修改项目端口配置',
      siblingTargets[0] || null,
    ));
  }

  if (/missing script:|cannot find module|enoent|eaddrinuse/i.test(text)) {
    maybePush(buildTargetSuggestion(
      'switch-node-target',
      siblingTargets[0] ? `切换到另一个 Node.js 入口：${siblingTargets[0].label}` : '检查脚本命令、端口或前端入口配置',
      siblingTargets[0] || null,
    ));
  }

  if (/command not found|缺少运行环境|no such file or directory/i.test(text)) {
    const requiredKinds = selectedTarget?.requiredToolchains || [];
    for (const kind of requiredKinds) {
      const current = selectedToolchainOptions[kind];
      const alternative = (availableOptionsByKind[kind] || []).find((item) => item.id !== current?.id) || null;
      maybePush(buildToolchainSuggestion(
        `resolve-${kind}`,
        current
          ? `切换 ${formatToolchainLabel(kind)} 到其它已发现环境`
          : `为 ${formatToolchainLabel(kind)} 选择一个可用环境`,
        kind,
        alternative || current,
      ));
    }
  }

  return suggestions.slice(0, 4);
};
