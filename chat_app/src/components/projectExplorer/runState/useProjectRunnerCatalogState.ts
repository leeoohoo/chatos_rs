import { useCallback, useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import {
  normalizeProjectRunCatalog,
  normalizeProjectRunEnvironment,
} from '../../../lib/domain/projectExplorer';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import type {
  Project,
  ProjectRunCustomToolchain,
  ProjectRunEnvironment,
  ProjectRunTarget,
  ProjectRunToolchainOption,
} from '../../../types';

interface UseProjectRunnerCatalogStateOptions {
  client: ApiClient;
  project: Project | null;
}

const resolveCommandPreview = (
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
    if (resolved === 'mvn' || resolved.startsWith('mvn ') || resolved === './mvnw' || resolved.startsWith('./mvnw ')) {
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

const buildCustomToolchainDrafts = (
  environment: ProjectRunEnvironment | null,
  requiredKinds: string[],
): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const kind of requiredKinds) {
    out[kind] = environment?.customToolchains[kind]?.path || '';
  }
  return out;
};

const serializeEnvVarsDraft = (envVars: Record<string, string>): string => (
  Object.entries(envVars)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `${key}=${value}`)
    .join('\n')
);

const parseEnvVarsDraft = (draft: string): Record<string, string> => {
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

const buildEnvPreview = (
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

const buildEnvironmentHints = (
  target: ProjectRunTarget | null,
  selectedOptions: Record<string, ProjectRunToolchainOption | null>,
): string[] => {
  if (!target) {
    return [];
  }

  const hints: string[] = [];
  if (target.kind === 'java') {
    if (selectedOptions.java_home?.path) {
      hints.push(`启动前会自动注入 JAVA_HOME=${selectedOptions.java_home.path}`);
    }
    if (selectedOptions.mvn_settings?.path) {
      hints.push(`Maven 命令会自动追加 -s ${selectedOptions.mvn_settings.path}`);
    }
    if (selectedOptions.mvn?.path) {
      hints.push(`系统 Maven 命令会替换为 ${selectedOptions.mvn.path}`);
    }
    if (selectedOptions.gradle_user_home?.path) {
      hints.push(`Gradle 会自动注入 GRADLE_USER_HOME=${selectedOptions.gradle_user_home.path}`);
    }
    if (selectedOptions.gradle?.path) {
      hints.push(`系统 Gradle 命令会替换为 ${selectedOptions.gradle.path}`);
    }
  }
  if (target.kind === 'python' && selectedOptions.python?.path) {
    hints.push(`Python 命令会优先使用 ${selectedOptions.python.path}`);
  }
  if (target.kind === 'node' && selectedOptions.node?.path) {
    hints.push(`Node 命令会优先使用 ${selectedOptions.node.path}`);
  }
  if (target.kind === 'node' && selectedOptions.npm?.path) {
    hints.push(`npm 命令会优先使用 ${selectedOptions.npm.path}`);
  }
  if (target.kind === 'node' && selectedOptions.pnpm?.path) {
    hints.push(`pnpm 命令会优先使用 ${selectedOptions.pnpm.path}`);
  }
  if (target.kind === 'node' && selectedOptions.yarn?.path) {
    hints.push(`yarn 命令会优先使用 ${selectedOptions.yarn.path}`);
  }
  if (target.kind === 'rust') {
    if (selectedOptions.cargo?.path) {
      hints.push(`Cargo 命令会优先使用 ${selectedOptions.cargo.path}`);
    }
    if (selectedOptions.rustc?.path) {
      hints.push(`编译阶段会优先使用 ${selectedOptions.rustc.path}`);
    }
  }
  if (target.kind === 'go' && selectedOptions.go?.path) {
    hints.push(`Go 命令会优先使用 ${selectedOptions.go.path}`);
  }

  return hints;
};

const buildEnvVarsPlaceholder = (target: ProjectRunTarget | null): string => {
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

export const useProjectRunnerCatalogState = ({
  client,
  project,
}: UseProjectRunnerCatalogStateOptions) => {
  const [selectedRunTargetId, setSelectedRunTargetId] = useState<string | null>(null);
  const [runTargets, setRunTargets] = useState<ProjectRunTarget[]>([]);
  const [runCatalogLoading, setRunCatalogLoading] = useState(false);
  const [runCatalogError, setRunCatalogError] = useState<string | null>(null);
  const [runEnvironment, setRunEnvironment] = useState<ProjectRunEnvironment | null>(null);
  const [runEnvironmentLoading, setRunEnvironmentLoading] = useState(false);
  const [runEnvironmentError, setRunEnvironmentError] = useState<string | null>(null);
  const [customToolchainDrafts, setCustomToolchainDrafts] = useState<Record<string, string>>({});
  const [envVarsDraft, setEnvVarsDraft] = useState('');

  const loadRunEnvironment = useCallback(async () => {
    if (!project?.id) {
      setRunEnvironment(null);
      setRunEnvironmentLoading(false);
      setRunEnvironmentError(null);
      return;
    }

    setRunEnvironmentLoading(true);
    setRunEnvironmentError(null);
    try {
      const raw = await client.getProjectRunEnvironment(project.id);
      const normalized = normalizeProjectRunEnvironment(raw);
      setRunEnvironment(normalized);
      setEnvVarsDraft(serializeEnvVarsDraft(normalized.envVars));
    } catch (error) {
      setRunEnvironment(null);
      setRunEnvironmentError(error instanceof Error ? error.message : '加载运行环境失败');
    } finally {
      setRunEnvironmentLoading(false);
    }
  }, [client, project?.id]);

  const loadRunCatalog = useCallback(async () => {
    if (!project?.id) {
      setRunTargets([]);
      setRunCatalogLoading(false);
      setRunCatalogError(null);
      setSelectedRunTargetId(null);
      return;
    }

    setRunCatalogLoading(true);
    setRunCatalogError(null);
    try {
      const raw = await client.analyzeProjectRun(project.id);
      const catalog = normalizeProjectRunCatalog(raw);
      setRunTargets(catalog.targets);
      setRunCatalogError(catalog.errorMessage || null);
      setSelectedRunTargetId((prev) => {
        if (prev && catalog.targets.some((item) => item.id === prev)) {
          return prev;
        }
        return catalog.defaultTargetId || catalog.targets[0]?.id || null;
      });
    } catch (error) {
      setRunTargets([]);
      setRunCatalogError(error instanceof Error ? error.message : '分析运行目标失败');
      setSelectedRunTargetId(null);
    } finally {
      setRunCatalogLoading(false);
    }
  }, [client, project?.id]);

  const selectRunTarget = useCallback(async (targetId: string) => {
    const normalizedTargetId = targetId.trim();
    if (!project?.id || !normalizedTargetId) {
      return;
    }

    setSelectedRunTargetId(normalizedTargetId);
    try {
      const raw = await client.setProjectRunDefault(project.id, normalizedTargetId);
      const catalog = normalizeProjectRunCatalog(raw);
      setRunTargets(catalog.targets);
      setRunCatalogError(catalog.errorMessage || null);
      setSelectedRunTargetId(catalog.defaultTargetId || normalizedTargetId);
    } catch (error) {
      setRunCatalogError(error instanceof Error ? error.message : '设置默认运行目标失败');
    }
  }, [client, project?.id]);

  const refreshRunnerState = useCallback(async () => {
    await Promise.all([
      loadRunCatalog(),
      loadRunEnvironment(),
    ]);
  }, [loadRunCatalog, loadRunEnvironment]);

  const resetRunnerCatalogState = useCallback(() => {
    setRunTargets([]);
    setRunCatalogLoading(false);
    setRunCatalogError(null);
    setSelectedRunTargetId(null);
    setRunEnvironment(null);
    setRunEnvironmentLoading(false);
    setRunEnvironmentError(null);
    setEnvVarsDraft('');
  }, []);

  const selectedRunTarget = useMemo(
    () => runTargets.find((item) => item.id === selectedRunTargetId) || runTargets[0] || null,
    [runTargets, selectedRunTargetId],
  );

  const availableToolchainKinds = useMemo(() => (
    selectedRunTarget?.requiredToolchains || []
  ), [selectedRunTarget?.requiredToolchains]);

  useEffect(() => {
    setCustomToolchainDrafts((prev) => ({
      ...buildCustomToolchainDrafts(runEnvironment, availableToolchainKinds),
      ...prev,
    }));
  }, [availableToolchainKinds, runEnvironment]);

  const selectedToolchainOptions = useMemo<Record<string, ProjectRunToolchainOption | null>>(() => {
    const environment = runEnvironment;
    const out: Record<string, ProjectRunToolchainOption | null> = {};
    for (const kind of availableToolchainKinds) {
      const options = environment?.optionsByKind[kind] || [];
      const selectedId = environment?.selectedToolchains[kind];
      out[kind] = options.find((item) => item.id === selectedId) || options[0] || null;
    }
    return out;
  }, [availableToolchainKinds, runEnvironment]);

  const missingToolchainKinds = useMemo(() => (
    availableToolchainKinds.filter((kind) => (runEnvironment?.optionsByKind[kind] || []).length === 0)
  ), [availableToolchainKinds, runEnvironment]);

  const persistEnvironment = useCallback(async (
    nextSelectedToolchains: Record<string, string>,
    nextCustomToolchains: Record<string, ProjectRunCustomToolchain>,
    nextEnvVars: Record<string, string>,
  ) => {
    if (!project?.id) {
      return;
    }
    const raw = await client.updateProjectRunEnvironment(project.id, {
      selected_toolchains: nextSelectedToolchains,
      custom_toolchains: Object.fromEntries(
        Object.entries(nextCustomToolchains).map(([kind, value]) => [
          kind,
          {
            kind: value.kind,
            label: value.label,
            path: value.path,
          },
        ]),
      ),
      env_vars: nextEnvVars,
    });
    const normalized = normalizeProjectRunEnvironment(raw);
    setRunEnvironment(normalized);
    setEnvVarsDraft(serializeEnvVarsDraft(normalized.envVars));
    setRunEnvironmentError(null);
  }, [client, project?.id]);

  const updateSelectedToolchain = useCallback(async (kind: string, optionId: string) => {
    const normalizedKind = kind.trim();
    const normalizedOptionId = optionId.trim();
    if (!project?.id || !normalizedKind || !normalizedOptionId) {
      return;
    }

    const nextSelectedToolchains = {
      ...(runEnvironment?.selectedToolchains || {}),
      [normalizedKind]: normalizedOptionId,
    };

    setRunEnvironment((prev) => prev ? {
      ...prev,
      selectedToolchains: nextSelectedToolchains,
    } : prev);

    try {
      await persistEnvironment(
        nextSelectedToolchains,
        runEnvironment?.customToolchains || {},
        runEnvironment?.envVars || {},
      );
    } catch (error) {
      setRunEnvironmentError(error instanceof Error ? error.message : '更新运行环境失败');
      await loadRunEnvironment();
    }
  }, [
    loadRunEnvironment,
    persistEnvironment,
    project?.id,
    runEnvironment?.customToolchains,
    runEnvironment?.selectedToolchains,
  ]);

  const updateCustomToolchainDraft = useCallback((kind: string, value: string) => {
    const normalizedKind = kind.trim();
    if (!normalizedKind) {
      return;
    }
    setCustomToolchainDrafts((prev) => ({
      ...prev,
      [normalizedKind]: value,
    }));
  }, []);

  const saveCustomToolchain = useCallback(async (kind: string) => {
    const normalizedKind = kind.trim();
    const draftPath = (customToolchainDrafts[normalizedKind] || '').trim();
    if (!project?.id || !normalizedKind || !draftPath) {
      return;
    }

    const nextSelectedOptionId = `${normalizedKind}:${draftPath}`;
    const nextCustomToolchains = {
      ...(runEnvironment?.customToolchains || {}),
      [normalizedKind]: {
        kind: normalizedKind,
        label: `手动指定: ${draftPath.split('/').filter(Boolean).slice(-2).join('/') || draftPath}`,
        path: draftPath,
      },
    };
    const nextSelectedToolchains = {
      ...(runEnvironment?.selectedToolchains || {}),
      [normalizedKind]: nextSelectedOptionId,
    };

    setRunEnvironment((prev) => prev ? {
      ...prev,
      selectedToolchains: nextSelectedToolchains,
      customToolchains: nextCustomToolchains,
    } : prev);

    try {
      await persistEnvironment(
        nextSelectedToolchains,
        nextCustomToolchains,
        runEnvironment?.envVars || {},
      );
      await loadRunEnvironment();
    } catch (error) {
      setRunEnvironmentError(error instanceof Error ? error.message : '保存自定义工具链失败');
      await loadRunEnvironment();
    }
  }, [
    customToolchainDrafts,
    loadRunEnvironment,
    persistEnvironment,
    project?.id,
    runEnvironment?.customToolchains,
    runEnvironment?.selectedToolchains,
  ]);

  const saveEnvVarsDraft = useCallback(async () => {
    if (!project?.id) {
      return;
    }

    const nextEnvVars = parseEnvVarsDraft(envVarsDraft);
    setRunEnvironment((prev) => prev ? {
      ...prev,
      envVars: nextEnvVars,
    } : prev);

    try {
      await persistEnvironment(
        runEnvironment?.selectedToolchains || {},
        runEnvironment?.customToolchains || {},
        nextEnvVars,
      );
      await loadRunEnvironment();
    } catch (error) {
      setRunEnvironmentError(error instanceof Error ? error.message : '保存环境变量失败');
      await loadRunEnvironment();
    }
  }, [
    envVarsDraft,
    loadRunEnvironment,
    persistEnvironment,
    project?.id,
    runEnvironment?.customToolchains,
    runEnvironment?.selectedToolchains,
  ]);

  const commandPreview = useMemo(() => {
    const command = resolveCommandPreview(selectedRunTarget?.command || '', selectedToolchainOptions);
    const envPrefix = serializeEnvVarsDraft(runEnvironment?.envVars || {});
    if (!envPrefix) {
      return command;
    }
    return `${envPrefix}\n${command}`.trim();
  }, [runEnvironment?.envVars, selectedRunTarget?.command, selectedToolchainOptions]);

  const envPreview = useMemo(
    () => buildEnvPreview(runEnvironment?.envVars || {}, selectedToolchainOptions),
    [runEnvironment?.envVars, selectedToolchainOptions],
  );

  const environmentHints = useMemo(
    () => buildEnvironmentHints(selectedRunTarget, selectedToolchainOptions),
    [selectedRunTarget, selectedToolchainOptions],
  );

  const envVarsPlaceholder = useMemo(
    () => buildEnvVarsPlaceholder(selectedRunTarget),
    [selectedRunTarget],
  );

  useProjectRunRealtime({
    enabled: Boolean(project?.id),
    projectId: project?.id || null,
    onCatalogUpdated: async () => {
      await loadRunCatalog();
    },
  });

  const runStatus = useMemo(() => {
    if (!project?.id) {
      return 'idle';
    }
    if (runCatalogLoading) {
      return 'loading';
    }
    if (runCatalogError) {
      return 'error';
    }
    if (runTargets.length > 0) {
      return 'ready';
    }
    return 'empty';
  }, [project?.id, runCatalogError, runCatalogLoading, runTargets.length]);

  useEffect(() => {
    if (runTargets.length === 0) {
      setSelectedRunTargetId(null);
      return;
    }
    setSelectedRunTargetId((prev) => prev || runTargets[0].id);
  }, [runTargets]);

  return {
    runStatus,
    runTargets,
    runCatalogLoading,
    runCatalogError,
    runEnvironment,
    runEnvironmentLoading,
    runEnvironmentError,
    availableToolchainKinds,
    selectedToolchainOptions,
    missingToolchainKinds,
    customToolchainDrafts,
    envVarsDraft,
    commandPreview,
    envPreview,
    environmentHints,
    envVarsPlaceholder,
    selectedRunTargetId,
    selectRunTarget,
    updateSelectedToolchain,
    updateCustomToolchainDraft,
    saveCustomToolchain,
    setEnvVarsDraft,
    saveEnvVarsDraft,
    loadRunCatalog,
    loadRunEnvironment,
    refreshRunnerState,
    resetRunnerCatalogState,
  };
};
