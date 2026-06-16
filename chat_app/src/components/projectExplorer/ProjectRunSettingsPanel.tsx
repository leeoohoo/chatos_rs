import React from 'react';

import { useI18n, type TranslateFn } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { useAuthStoreSelector } from '../../lib/auth/authStore';
import { useTheme } from '../../hooks/useTheme';
import type {
  ProjectRunEnvironment,
  ProjectRunInstance,
  ProjectRunResolutionSuggestion,
  ProjectRunState,
  ProjectRunTarget,
  ProjectRunToolchainOption,
  Terminal,
} from '../../types';
import EmbeddedTerminalView from '../terminal/EmbeddedTerminalView';

interface ProjectRunSettingsPanelProps {
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
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRunnerDelete: () => void;
  onRefreshRunnerState: () => void;
}

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

const TOOLCHAIN_AUTO_ENV_KEYS: Record<string, string> = {
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

const TOOLCHAIN_DISPLAY_NAMES: Record<string, string> = {
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

const TARGET_CONFIG_KIND_MAP: Record<string, string[]> = {
  java: ['maven_config', 'maven_jvm_config', 'gradle_properties', 'gradle_user_properties'],
  node: ['package_json', 'node_lockfile', 'node_workspace', 'node_runtime_config'],
  python: ['python_manifest', 'python_runtime_config'],
  go: ['go_manifest'],
  rust: ['cargo_manifest', 'cargo_runtime_config', 'cargo_toolchain'],
};

const getRunStatusLabel = (status: string, t: TranslateFn): string => {
  const key = `runSettings.status.${status}`;
  const label = t(key);
  return label === key ? status : label;
};

const formatRunTargetSource = (target: ProjectRunTarget, t: TranslateFn): string => {
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

const formatRunTargetOptionHint = (target: ProjectRunTarget, t: TranslateFn): string => {
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

const resolveTargetDisplayName = (kind?: string | null): string => {
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

const formatToolchainKind = (kind: string): string => TOOLCHAIN_LABELS[kind] || kind;

const formatToolchainSource = (source: string | null | undefined, t: TranslateFn): string => {
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

const resolveManualHint = (kind: string, t: TranslateFn): string => {
  if (kind === 'mvn_settings') {
    return t('runSettings.manualHint.mavenSettings');
  }
  if (kind === 'gradle_user_home') {
    return t('runSettings.manualHint.gradleHome');
  }
  return t('runSettings.manualHint.default');
};

const resolveConfigFilesEmptyText = (
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

const resolveConfigSectionTitle = (target: ProjectRunTarget | null, t: TranslateFn): string => {
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

const resolveConfigKindsForTarget = (target: ProjectRunTarget | null): string[] => {
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

const buildInjectedEnvHint = (toolchainKinds: string[], t: TranslateFn): string => {
  const keys = toolchainKinds
    .map((kind) => TOOLCHAIN_AUTO_ENV_KEYS[kind])
    .filter(Boolean)
    .slice(0, 3);
  if (keys.length === 0) {
    return t('runSettings.injectedEnv.none');
  }
  return t('runSettings.injectedEnv.some', { keys: keys.map((key) => `\`${key}\``).join(' / ') });
};

export const ProjectRunSettingsPanel: React.FC<ProjectRunSettingsPanelProps> = ({
  projectName,
  projectRootPath,
  runStatus,
  runCatalogLoading,
  runEnvironment,
  runEnvironmentLoading,
  runEnvironmentError,
  configFiles,
  validationIssues,
  runTargets,
  availableToolchainKinds,
  selectedToolchainOptions,
  missingToolchainKinds,
  customToolchainDrafts,
  envVarsDraft,
  commandPreview,
  envPreview,
  environmentHints,
  envVarsPlaceholder,
  showTerminalUi,
  selectedRunTargetId,
  starting,
  stopping,
  restarting,
  deleting,
  runnerMessage,
  runnerError,
  runnerDiagnosis,
  runnerSuggestions = [],
  projectRunState,
  projectRunInstances,
  selectedRunInstanceId,
  projectRunTerminal,
  projectRunTerminalBusy,
  onSelectRunTarget,
  onSelectRunInstance,
  onSelectToolchain,
  onApplySuggestion,
  onCustomToolchainDraftChange,
  onSaveCustomToolchain,
  onEnvVarsDraftChange,
  onSaveEnvVarsDraft,
  onRunnerStart,
  onRunnerStop,
  onRunnerRestart,
  onRunnerDelete,
  onRefreshRunnerState,
}) => {
  const { t } = useI18n();
  const terminalClient = useApiClient();
  const accessToken = useAuthStoreSelector((state) => state.accessToken);
  const { actualTheme } = useTheme();
  const selectedTarget = runTargets.find((target) => target.id === selectedRunTargetId) || runTargets[0] || null;
  const selectedTargetConfigKinds = resolveConfigKindsForTarget(selectedTarget);
  const selectedConfigFiles = configFiles.filter((file) => (
    selectedTargetConfigKinds.length === 0 || selectedTargetConfigKinds.includes(file.kind)
  ));
  const selectedTargetIssues = validationIssues.filter((issue) => (
    !selectedTarget?.id || !issue.targetId || issue.targetId === selectedTarget.id
  ));
  const otherTargetIssues = validationIssues.filter((issue) => (
    selectedTarget?.id && issue.targetId && issue.targetId !== selectedTarget.id
  ));
  const statusLabel = getRunStatusLabel(runStatus, t);
  const statusTone = runStatus === 'ready'
    ? 'text-emerald-700 border-emerald-500/30 bg-emerald-500/10'
    : runStatus === 'error'
      ? 'text-destructive border-destructive/30 bg-destructive/10'
      : 'text-muted-foreground border-border bg-background';

  return (
    <div className="rounded-lg border border-border bg-card">
      <div className="border-b border-border px-4 py-3">
        <div className="min-w-0">
          <div className="truncate text-base font-semibold text-foreground">
            {projectName || t('runSettings.projectSettings')}
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground">
            {projectRootPath || t('runSettings.noProjectRoot')}
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-[11px]">
            <span className={`rounded border px-2 py-1 ${statusTone}`}>
              {t('runSettings.runStatus', { status: statusLabel })}
            </span>
            <span className="rounded border border-border px-2 py-1 text-muted-foreground">
              {t('runSettings.runTargetsCount', { count: runTargets.length })}
            </span>
            {selectedTarget?.language && (
              <span className="rounded border border-border px-2 py-1 text-muted-foreground">
                {t('runSettings.language', { language: selectedTarget.language })}
              </span>
            )}
          </div>
        </div>

        {(runnerError || runnerMessage || runEnvironmentError || runnerDiagnosis) && (
          <div className="mt-3 space-y-2 rounded border border-border/70 bg-background/60 px-3 py-2 text-xs">
            {runnerError && (
              <div className="text-destructive">{runnerError}</div>
            )}
            {runEnvironmentError && (
              <div className="text-destructive">{runEnvironmentError}</div>
            )}
            {runnerMessage && (
              <div className="text-emerald-700">{runnerMessage}</div>
            )}
            {runnerDiagnosis && !runnerError?.includes(runnerDiagnosis) && (
              <div className="text-amber-700">
                {t('runSettings.latestExitDiagnosis', { diagnosis: runnerDiagnosis })}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="space-y-4 p-4">
        {runnerDiagnosis && (
          <div className="rounded border border-amber-500/30 bg-amber-500/5 p-3">
            <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.latestRunDiagnosis')}</div>
            <div className="text-sm text-amber-800">{runnerDiagnosis}</div>
            <div className="mt-2 text-[11px] text-muted-foreground">
              {t('runSettings.diagnosisDescription')}
            </div>
            {runnerSuggestions.length > 0 && (
              <div className="mt-3 space-y-2">
                <div className="text-[11px] text-muted-foreground">{t('runSettings.suggestions')}</div>
                <div className="flex flex-wrap gap-2">
                  {runnerSuggestions.map((suggestion) => (
                    <button
                      key={suggestion.id}
                      type="button"
                      onClick={() => onApplySuggestion(suggestion)}
                      className="rounded border border-amber-500/40 bg-background px-3 py-1.5 text-xs text-amber-800 hover:bg-amber-500/10"
                      title={suggestion.detail || suggestion.label}
                    >
                      {suggestion.label}
                    </button>
                  ))}
                </div>
                {runnerSuggestions.some((item) => item.detail) && (
                  <div className="space-y-1 text-[11px] text-muted-foreground">
                    {runnerSuggestions.map((suggestion) => (
                      suggestion.detail ? (
                        <div key={`${suggestion.id}:detail`} className="break-all">
                          {suggestion.label}: {suggestion.detail}
                        </div>
                      ) : null
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        <div className="rounded border border-border/70 bg-background/50 p-3">
          <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.preflight')}</div>
          {selectedTargetIssues.length === 0 && otherTargetIssues.length === 0 ? (
            <div className="text-sm text-emerald-700">
              {t('runSettings.preflightClean')}
            </div>
          ) : (
            <div className="space-y-3">
              {selectedTargetIssues.map((issue, index) => (
                <div key={`${issue.kind}:${issue.path || index}`} className="rounded border border-destructive/30 bg-destructive/5 p-3">
                  <div className="text-sm text-destructive">{issue.message}</div>
                  {issue.path && (
                    <div className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                      {issue.path}
                    </div>
                  )}
                  {issue.hint && (
                    <div className="mt-2 text-[11px] text-muted-foreground">
                      {t('runSettings.issueHint', { hint: issue.hint })}
                    </div>
                  )}
                </div>
              ))}
              {otherTargetIssues.length > 0 && (
                <details className="rounded border border-border/60 bg-card">
                  <summary className="cursor-pointer list-none px-3 py-2 text-xs text-muted-foreground">
                    {t('runSettings.otherTargetIssues', { count: otherTargetIssues.length })}
                  </summary>
                  <div className="border-t border-border/60 space-y-3 px-3 py-3">
                    {otherTargetIssues.map((issue, index) => (
                      <div key={`${issue.kind}:${issue.targetId || issue.path || index}`} className="rounded border border-border/60 bg-background p-3">
                        <div className="text-sm text-foreground">
                          {issue.targetLabel ? `[${issue.targetLabel}] ` : ''}
                          {issue.message}
                        </div>
                        {issue.path && (
                          <div className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                            {issue.path}
                          </div>
                        )}
                        {issue.hint && (
                          <div className="mt-2 text-[11px] text-muted-foreground">
                            {t('runSettings.issueHint', { hint: issue.hint })}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </details>
              )}
            </div>
          )}
        </div>

        <div className="rounded border border-border/70 bg-background/50 p-3">
          <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.runTargets')}</div>
          {runTargets.length === 0 ? (
            <div className="text-sm text-muted-foreground">{t('runSettings.noRunTargets')}</div>
          ) : (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <select
                  value={selectedRunTargetId || runTargets[0]?.id || ''}
                  onChange={(event) => onSelectRunTarget(event.target.value)}
                  disabled={starting || stopping || restarting || deleting || runCatalogLoading}
                  className="h-9 min-w-[280px] max-w-[520px] rounded border border-border bg-background px-2 text-sm text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {runTargets.map((target) => (
                    <option key={target.id} value={target.id}>
                      {[target.label, formatRunTargetOptionHint(target, t)].filter(Boolean).join(' · ')}
                    </option>
                  ))}
                </select>
                <div className="truncate text-xs text-muted-foreground" title={selectedTarget?.cwd || ''}>
                  {selectedTarget?.cwd || ''}
                </div>
              </div>

              {selectedTarget && (
                <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                  <span className="rounded border border-border px-2 py-1">
                    {formatRunTargetSource(selectedTarget, t)}
                  </span>
                  {selectedTarget.entrypoint && (
                    <span className="rounded border border-border px-2 py-1" title={selectedTarget.entrypoint}>
                      {t('runSettings.entrypoint', { entrypoint: selectedTarget.entrypoint })}
                    </span>
                  )}
                  {selectedTarget.manifestPath && (
                    <span className="rounded border border-border px-2 py-1" title={selectedTarget.manifestPath}>
                      {t('runSettings.manifest', { manifest: selectedTarget.manifestPath })}
                    </span>
                  )}
                  <span className="rounded border border-border px-2 py-1">
                    {t('runSettings.command', { command: selectedTarget.command })}
                  </span>
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-3">
                <button
                  type="button"
                  onClick={onRunnerStart}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || runCatalogLoading}
                  className="h-8 rounded border border-emerald-500/40 px-3 text-xs text-emerald-700 hover:bg-emerald-500/10 disabled:cursor-not-allowed disabled:opacity-50"
                  title={commandPreview}
                >
                  {starting ? t('runSettings.starting') : t('runSettings.startNew')}
                </button>
                <button
                  type="button"
                  onClick={onRunnerStop}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || !selectedRunInstanceId}
                  className="h-8 rounded border border-rose-500/40 px-3 text-xs text-rose-700 hover:bg-rose-500/10 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {stopping ? t('runSettings.stopping') : t('runSettings.stopCurrent')}
                </button>
                <button
                  type="button"
                  onClick={onRunnerRestart}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || runCatalogLoading || !selectedRunInstanceId}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                  title={commandPreview}
                >
                  {restarting ? t('runSettings.restarting') : t('runSettings.restartCurrent')}
                </button>
                <button
                  type="button"
                  onClick={onRunnerDelete}
                  disabled={starting || stopping || restarting || deleting || !selectedRunInstanceId}
                  className="h-8 rounded border border-destructive/40 px-3 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {deleting ? t('runSettings.deleting') : t('runSettings.deleteCurrent')}
                </button>
                <button
                  type="button"
                  onClick={onRefreshRunnerState}
                  disabled={runCatalogLoading || runEnvironmentLoading || deleting}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {runCatalogLoading || runEnvironmentLoading ? t('runSettings.refreshing') : t('runSettings.refreshStatus')}
                </button>
              </div>
            </div>
          )}
        </div>

        {showTerminalUi ? (
          <div className="rounded border border-border/70 bg-background/50 p-3">
            <div className="mb-2 flex items-center justify-between gap-3">
              <div className="text-[11px] text-muted-foreground">{t('runSettings.instances')}</div>
              <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                <span className="rounded border border-border px-2 py-1">
                  {t('runSettings.instanceCount', { count: projectRunInstances.length })}
                </span>
                <span className="rounded border border-border px-2 py-1">
                  {t('runSettings.projectStatus', { status: projectRunState?.status || 'idle' })}
                </span>
              </div>
            </div>

            {projectRunInstances.length === 0 ? (
              <div className="text-sm text-muted-foreground">
                {t('runSettings.noInstances')}
              </div>
            ) : (
              <div className="space-y-3">
                <div className="flex flex-wrap gap-2">
                  {projectRunInstances.map((instance, index) => {
                    const selected = instance.terminalId === selectedRunInstanceId;
                    return (
                      <button
                        key={instance.terminalId}
                        type="button"
                        onClick={() => onSelectRunInstance(instance.terminalId)}
                        className={[
                          'rounded border px-3 py-2 text-left text-xs transition-colors',
                          selected
                            ? 'border-primary bg-primary/10 text-foreground'
                            : 'border-border bg-card text-muted-foreground hover:bg-accent',
                        ].join(' ')}
                      >
                        <div className="font-medium text-foreground">
                          {t('runSettings.instance', { index: index + 1 })}
                        </div>
                        <div className="mt-1">
                          {instance.running ? (instance.busy ? t('runSettings.running') : t('runSettings.idle')) : instance.status}
                        </div>
                      </button>
                    );
                  })}
                </div>

                <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                  <span className="rounded border border-border px-2 py-1">
                    {t('runSettings.terminalStatus', { status: projectRunTerminal?.status || 'idle' })}
                  </span>
                  <span className="rounded border border-border px-2 py-1">
                    {t('runSettings.process', {
                      status: projectRunTerminal
                        ? (projectRunTerminalBusy ? t('runSettings.running') : (projectRunTerminal.status === 'running' ? t('runSettings.idle') : t('runSettings.notRunning')))
                        : t('runSettings.notRunning'),
                    })}
                  </span>
                  {projectRunTerminal?.name && (
                    <span className="rounded border border-border px-2 py-1">
                      {projectRunTerminal.name}
                    </span>
                  )}
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="rounded border border-border/70 bg-background/50 p-3 text-sm text-muted-foreground">
            {t('runSettings.terminalUiDisabledInSettings')}
          </div>
        )}

        <details className="rounded border border-border/70 bg-background/50" open={false}>
          <summary className="cursor-pointer list-none px-3 py-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="text-[11px] text-muted-foreground">{t('runSettings.runEnvironment')}</div>
              <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                <span className="rounded border border-border px-2 py-1">
                  {t('runSettings.toolchainItems', { count: availableToolchainKinds.length })}
                </span>
                {missingToolchainKinds.length > 0 && (
                  <span className="rounded border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-amber-700">
                    {t('runSettings.missing', { count: missingToolchainKinds.length })}
                  </span>
                )}
                <span className="rounded border border-border px-2 py-1">
                  {t('runSettings.collapsedDefault')}
                </span>
              </div>
            </div>
          </summary>
          <div className="border-t border-border/60 px-3 py-3">
            {availableToolchainKinds.length === 0 ? (
              <div className="text-sm text-muted-foreground">{t('runSettings.noToolchainNeeded')}</div>
            ) : (
              <div className="grid gap-3 md:grid-cols-2">
                {availableToolchainKinds.map((kind) => {
                  const options = runEnvironment?.optionsByKind[kind] || [];
                  const selectedOption = selectedToolchainOptions[kind];
                  const isMissing = missingToolchainKinds.includes(kind);
                  const manualDraft = customToolchainDrafts[kind] || '';
                  const showManualInput = isMissing || selectedOption?.source === 'manual';
                  return (
                    <div key={kind} className="rounded border border-border/60 bg-card p-3">
                      <div className="mb-1 flex items-center justify-between gap-3">
                        <div className="text-xs font-medium text-foreground">{formatToolchainKind(kind)}</div>
                        {options.length > 0 && (
                          <div className="text-[11px] text-muted-foreground">
                            {t('runSettings.foundOptions', { count: options.length })}
                          </div>
                        )}
                      </div>
                      <select
                        value={selectedOption?.id || options[0]?.id || ''}
                        onChange={(event) => onSelectToolchain(kind, event.target.value)}
                        disabled={options.length === 0 || starting || stopping || restarting || deleting || runEnvironmentLoading}
                        className="h-9 w-full rounded border border-border bg-background px-2 text-sm text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                        title={selectedOption?.path || kind}
                      >
                        {options.length === 0 ? (
                          <option value="">{t('runSettings.notFoundToolchain', { name: formatToolchainKind(kind) })}</option>
                        ) : (
                          options.map((option) => (
                            <option key={option.id} value={option.id}>
                              {option.label} · {formatToolchainSource(option.source, t)}
                            </option>
                          ))
                        )}
                      </select>
                      <div className="mt-2 space-y-2">
                        <div className="truncate text-[11px] text-muted-foreground" title={selectedOption?.path || ''}>
                          {selectedOption?.path || (isMissing ? t('runSettings.missingToolchainPath', { name: formatToolchainKind(kind) }) : '')}
                        </div>
                        {selectedOption && (
                          <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                            <span className="rounded border border-border px-2 py-1">
                              {t('runSettings.source', { source: formatToolchainSource(selectedOption.source, t) })}
                            </span>
                            {selectedOption.version && (
                              <span className="rounded border border-border px-2 py-1">
                                {t('runSettings.versionHint', { version: selectedOption.version })}
                              </span>
                            )}
                          </div>
                        )}
                      </div>

                      <details className="mt-3 rounded border border-dashed border-border/70 bg-background/40">
                        <summary className="cursor-pointer list-none px-3 py-2 text-xs text-muted-foreground">
                          {showManualInput ? t('runSettings.manualPath') : t('runSettings.manualPathAlt')}
                        </summary>
                        <div className="border-t border-border/60 px-3 py-3">
                          <div className="mb-2 text-[11px] text-muted-foreground">
                            {resolveManualHint(kind, t)}
                          </div>
                          <div className="flex items-center gap-2">
                            <input
                              value={manualDraft}
                              onChange={(event) => onCustomToolchainDraftChange(kind, event.target.value)}
                              placeholder={t('runSettings.manualPathPlaceholder', { name: formatToolchainKind(kind) })}
                              className="h-9 flex-1 rounded border border-border bg-background px-3 text-sm text-foreground"
                            />
                            <button
                              type="button"
                              onClick={() => onSaveCustomToolchain(kind)}
                              disabled={!manualDraft.trim() || runEnvironmentLoading}
                              className="h-9 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              {t('runSettings.saveAndSelect')}
                            </button>
                          </div>
                        </div>
                      </details>
                    </div>
                  );
                })}
              </div>
            )}

            <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
              <div className="mb-2 text-[11px] text-muted-foreground">{resolveConfigSectionTitle(selectedTarget, t)}</div>
              {selectedConfigFiles.length === 0 ? (
                <div className="text-sm text-muted-foreground">
                  {resolveConfigFilesEmptyText(selectedTarget, availableToolchainKinds, t)}
                </div>
              ) : (
                <div className="space-y-3">
                  {selectedConfigFiles.map((file) => (
                    <div key={`${file.kind}:${file.path}`} className="rounded border border-border/60 bg-card p-3">
                      <div className="flex flex-wrap items-center gap-2 text-xs">
                        <span className="font-medium text-foreground">{file.label}</span>
                        <span className="rounded border border-border px-2 py-1 text-[11px] text-muted-foreground">
                          {t('runSettings.source', { source: formatToolchainSource(file.source, t) })}
                        </span>
                      </div>
                      <div className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
                        {file.path}
                      </div>
                      {file.preview && (
                        <div className="mt-2 rounded border border-border/60 bg-background px-2 py-2 font-mono text-[11px] text-foreground">
                          {file.preview}
                        </div>
                      )}
                    </div>
                  ))}
                  <div className="text-[11px] text-muted-foreground">
                    {t('runSettings.configReadonlyHint')}
                  </div>
                </div>
              )}
            </div>

            <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
              <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.envVars')}</div>
              {environmentHints.length > 0 && (
                <div className="mb-3 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                  {environmentHints.map((hint) => (
                    <span key={hint} className="rounded border border-border px-2 py-1">
                      {hint}
                    </span>
                  ))}
                </div>
              )}
              <textarea
                value={envVarsDraft}
                onChange={(event) => onEnvVarsDraftChange(event.target.value)}
                placeholder={envVarsPlaceholder}
                className="min-h-[120px] w-full rounded border border-border bg-background px-3 py-2 font-mono text-xs text-foreground"
              />
              <div className="mt-2 flex items-center justify-between gap-3">
                <div className="text-[11px] text-muted-foreground">
                  {t('runSettings.envVarsHelp', { hint: buildInjectedEnvHint(availableToolchainKinds, t) })}
                </div>
                <button
                  type="button"
                  onClick={onSaveEnvVarsDraft}
                  disabled={runEnvironmentLoading}
                  className="h-9 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {t('runSettings.saveEnvVars')}
                </button>
              </div>
              <div className="mt-3 rounded border border-border/60 bg-card p-3">
                <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.envPreview')}</div>
                <pre className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-foreground">
                  {envPreview || t('runSettings.noEnvPreview')}
                </pre>
              </div>
            </div>

            <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
              <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.commandPreview')}</div>
              <pre className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-foreground">
                {commandPreview || t('runSettings.noCommand')}
              </pre>
            </div>
          </div>
        </details>

        {showTerminalUi && runEnvironment && (
          <div className="rounded border border-border/70 bg-background/50 p-3">
            <div className="mb-2 text-[11px] text-muted-foreground">{t('runSettings.terminal')}</div>
            <div className="h-[420px] overflow-hidden rounded border border-border/60 bg-card">
              <EmbeddedTerminalView
                terminal={projectRunTerminal}
                emptyText={t('runSettings.terminalEmpty')}
                client={terminalClient}
                accessToken={accessToken}
                actualTheme={actualTheme}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ProjectRunSettingsPanel;
