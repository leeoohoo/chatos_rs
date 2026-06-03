import React from 'react';

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

const STATUS_LABELS: Record<string, string> = {
  ready: '已就绪',
  loading: '分析中',
  empty: '未发现目标',
  error: '分析失败',
  idle: '未选择项目',
};

const formatRunTargetSource = (target: ProjectRunTarget): string => {
  const kind = (target.kind || '').trim();
  const entrypoint = (target.entrypoint || '').trim();
  const manifestPath = (target.manifestPath || '').trim();

  if (kind === 'node') {
    if (entrypoint.startsWith('package.json:scripts.')) {
      return `来源: package.json 脚本 ${entrypoint.replace('package.json:scripts.', '')}`;
    }
    return manifestPath ? '来源: package.json' : '来源: Node.js 自动分析';
  }
  if (kind === 'python') {
    if (entrypoint.endsWith('.py')) {
      return `来源: Python 脚本 ${entrypoint}`;
    }
    if (target.command.includes('pytest')) {
      return '来源: Python 测试入口';
    }
    return manifestPath ? '来源: Python 项目清单' : '来源: Python 自动分析';
  }
  if (kind === 'go') {
    if (entrypoint.startsWith('./cmd/')) {
      return `来源: Go cmd 入口 ${entrypoint}`;
    }
    if (entrypoint === '.') {
      return '来源: Go 根目录 main 包';
    }
    return manifestPath ? '来源: go.mod' : '来源: Go 自动分析';
  }
  if (kind === 'rust') {
    if (entrypoint.startsWith('src/bin/')) {
      return `来源: Rust bin 入口 ${entrypoint}`;
    }
    if (entrypoint === 'src/main.rs') {
      return '来源: Rust 默认主程序 src/main.rs';
    }
    return manifestPath ? '来源: Cargo 项目清单' : '来源: Rust 自动分析';
  }
  if (kind === 'java') {
    if (entrypoint) {
      return `来源: Java 主类 ${entrypoint}`;
    }
    if (manifestPath.endsWith('pom.xml')) {
      return '来源: Maven 项目';
    }
    if (manifestPath) {
      return '来源: Gradle 项目';
    }
  }
  return target.source === 'auto' ? '来源: 自动分析' : `来源: ${target.source}`;
};

const formatRunTargetOptionHint = (target: ProjectRunTarget): string => {
  const kind = (target.kind || '').trim();
  const entrypoint = (target.entrypoint || '').trim();

  if (kind === 'node' && entrypoint.startsWith('package.json:scripts.')) {
    return `脚本 ${entrypoint.replace('package.json:scripts.', '')}`;
  }
  if (kind === 'python' && entrypoint) {
    return entrypoint;
  }
  if (kind === 'go' && entrypoint) {
    return entrypoint === '.' ? '根目录 main 包' : entrypoint;
  }
  if (kind === 'rust' && entrypoint.startsWith('src/bin/')) {
    return entrypoint.replace('src/bin/', '').replace('/main.rs', '').replace('.rs', '');
  }
  if (kind === 'rust' && entrypoint === 'src/main.rs') {
    return '默认主程序';
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

const formatToolchainSource = (source?: string | null): string => {
  switch ((source || '').trim()) {
    case 'sandbox':
      return '项目沙箱';
    case 'project-local':
      return '项目内';
    case 'env':
      return '环境变量';
    case 'path':
      return 'PATH';
    case 'system':
      return '系统安装';
    case 'manual':
      return '手动指定';
    default:
      return '自动发现';
  }
};

const resolveManualHint = (kind: string): string => {
  if (kind === 'mvn_settings') {
    return '没有发现时，通常优先检查 ~/.m2/settings.xml 或项目内 .mvn/settings.xml';
  }
  if (kind === 'gradle_user_home') {
    return '没有发现时，通常优先检查 ~/.gradle 或项目内 .gradle';
  }
  return '下拉框里会优先展示项目内环境、系统已安装版本和本机版本管理器里的候选；这里只有在自动发现不完整时才需要填写。';
};

const resolveConfigFilesEmptyText = (
  target: ProjectRunTarget | null,
  toolchainKinds: string[],
): string => {
  const projectLabel = resolveTargetDisplayName(target?.kind);

  const toolchainLabels = toolchainKinds
    .map((kind) => TOOLCHAIN_DISPLAY_NAMES[kind] || formatToolchainKind(kind))
    .filter((value, index, list) => Boolean(value) && list.indexOf(value) === index);

  if (projectLabel && toolchainLabels.length > 0) {
    return `当前没有发现额外的 ${projectLabel} 项目配置文件；本次运行会主要依赖 ${toolchainLabels.join(' / ')} 环境。`;
  }
  if (projectLabel) {
    return `当前没有发现额外的 ${projectLabel} 项目配置文件。`;
  }
  return '当前没有发现额外的项目运行配置文件。';
};

const resolveConfigSectionTitle = (target: ProjectRunTarget | null): string => {
  switch ((target?.kind || '').trim()) {
    case 'java':
      return 'Java 项目配置文件';
    case 'rust':
      return 'Cargo / Rust 配置文件';
    case 'python':
      return 'Python 项目配置文件';
    case 'node':
      return 'Node.js 项目配置文件';
    case 'go':
      return 'Go 项目配置文件';
    default:
      return '项目配置文件';
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

const buildInjectedEnvHint = (toolchainKinds: string[]): string => {
  const keys = toolchainKinds
    .map((kind) => TOOLCHAIN_AUTO_ENV_KEYS[kind])
    .filter(Boolean)
    .slice(0, 3);
  if (keys.length === 0) {
    return '这里填的是项目自定义变量；上面运行环境里已选择的工具链路径会自动注入，不需要在这里重复填写。';
  }
  return `这里填的是项目自定义变量；像 ${keys.map((key) => `\`${key}\``).join('、')} 这类会根据上面的下拉自动注入。`;
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
  const statusLabel = STATUS_LABELS[runStatus] || runStatus;
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
            {projectName || '项目设置'}
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground">
            {projectRootPath || '未配置项目目录'}
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-[11px]">
            <span className={`rounded border px-2 py-1 ${statusTone}`}>
              运行状态: {statusLabel}
            </span>
            <span className="rounded border border-border px-2 py-1 text-muted-foreground">
              运行目标: {runTargets.length}
            </span>
            {selectedTarget?.language && (
              <span className="rounded border border-border px-2 py-1 text-muted-foreground">
                语言: {selectedTarget.language}
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
                最近一次退出诊断: {runnerDiagnosis}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="space-y-4 p-4">
        {runnerDiagnosis && (
          <div className="rounded border border-amber-500/30 bg-amber-500/5 p-3">
            <div className="mb-2 text-[11px] text-muted-foreground">最近一次运行诊断</div>
            <div className="text-sm text-amber-800">{runnerDiagnosis}</div>
            <div className="mt-2 text-[11px] text-muted-foreground">
              这是根据最近一次启动后终端退出日志自动提炼出的原因，用来帮助快速定位本地环境或入口配置问题。
            </div>
            {runnerSuggestions.length > 0 && (
              <div className="mt-3 space-y-2">
                <div className="text-[11px] text-muted-foreground">建议操作</div>
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
          <div className="mb-2 text-[11px] text-muted-foreground">运行前检查</div>
          {selectedTargetIssues.length === 0 && otherTargetIssues.length === 0 ? (
            <div className="text-sm text-emerald-700">
              当前未发现明显的本地运行配置问题。
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
                      建议: {issue.hint}
                    </div>
                  )}
                </div>
              ))}
              {otherTargetIssues.length > 0 && (
                <details className="rounded border border-border/60 bg-card">
                  <summary className="cursor-pointer list-none px-3 py-2 text-xs text-muted-foreground">
                    查看其它运行入口的问题 ({otherTargetIssues.length})
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
                            建议: {issue.hint}
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
          <div className="mb-2 text-[11px] text-muted-foreground">运行目标</div>
          {runTargets.length === 0 ? (
            <div className="text-sm text-muted-foreground">当前项目尚未分析出可运行目标。</div>
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
                      {[target.label, formatRunTargetOptionHint(target)].filter(Boolean).join(' · ')}
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
                    {formatRunTargetSource(selectedTarget)}
                  </span>
                  {selectedTarget.entrypoint && (
                    <span className="rounded border border-border px-2 py-1" title={selectedTarget.entrypoint}>
                      入口: {selectedTarget.entrypoint}
                    </span>
                  )}
                  {selectedTarget.manifestPath && (
                    <span className="rounded border border-border px-2 py-1" title={selectedTarget.manifestPath}>
                      清单: {selectedTarget.manifestPath}
                    </span>
                  )}
                  <span className="rounded border border-border px-2 py-1">
                    命令: {selectedTarget.command}
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
                  {starting ? '启动中...' : '启动新实例'}
                </button>
                <button
                  type="button"
                  onClick={onRunnerStop}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || !selectedRunInstanceId}
                  className="h-8 rounded border border-rose-500/40 px-3 text-xs text-rose-700 hover:bg-rose-500/10 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {stopping ? '停止中...' : '停止当前实例'}
                </button>
                <button
                  type="button"
                  onClick={onRunnerRestart}
                  disabled={runStatus !== 'ready' || starting || stopping || restarting || deleting || runCatalogLoading || !selectedRunInstanceId}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                  title={commandPreview}
                >
                  {restarting ? '重启中...' : '重启当前实例'}
                </button>
                <button
                  type="button"
                  onClick={onRunnerDelete}
                  disabled={starting || stopping || restarting || deleting || !selectedRunInstanceId}
                  className="h-8 rounded border border-destructive/40 px-3 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {deleting ? '删除中...' : '删除当前实例'}
                </button>
                <button
                  type="button"
                  onClick={onRefreshRunnerState}
                  disabled={runCatalogLoading || runEnvironmentLoading || deleting}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {runCatalogLoading || runEnvironmentLoading ? '刷新中...' : '刷新状态'}
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="rounded border border-border/70 bg-background/50 p-3">
          <div className="mb-2 flex items-center justify-between gap-3">
            <div className="text-[11px] text-muted-foreground">运行实例</div>
            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
              <span className="rounded border border-border px-2 py-1">
                实例数: {projectRunInstances.length}
              </span>
              <span className="rounded border border-border px-2 py-1">
                项目状态: {projectRunState?.status || 'idle'}
              </span>
            </div>
          </div>

          {projectRunInstances.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              还没有运行中的项目实例。先在上面选择入口，再点击“启动新实例”。
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
                        实例 {index + 1}
                      </div>
                      <div className="mt-1">
                        {instance.running ? (instance.busy ? '运行中' : '空闲') : instance.status}
                      </div>
                    </button>
                  );
                })}
              </div>

              <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                <span className="rounded border border-border px-2 py-1">
                  终端状态: {projectRunTerminal?.status || 'idle'}
                </span>
                <span className="rounded border border-border px-2 py-1">
                  进程: {projectRunTerminal ? (projectRunTerminalBusy ? '运行中' : (projectRunTerminal.status === 'running' ? '空闲' : '未运行')) : '未运行'}
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

        <details className="rounded border border-border/70 bg-background/50" open={false}>
          <summary className="cursor-pointer list-none px-3 py-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="text-[11px] text-muted-foreground">运行环境</div>
              <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                <span className="rounded border border-border px-2 py-1">
                  工具链项: {availableToolchainKinds.length}
                </span>
                {missingToolchainKinds.length > 0 && (
                  <span className="rounded border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-amber-700">
                    缺失: {missingToolchainKinds.length}
                  </span>
                )}
                <span className="rounded border border-border px-2 py-1">
                  默认收起
                </span>
              </div>
            </div>
          </summary>
          <div className="border-t border-border/60 px-3 py-3">
            {availableToolchainKinds.length === 0 ? (
              <div className="text-sm text-muted-foreground">当前运行目标不需要额外工具链配置。</div>
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
                            已发现 {options.length} 个可用环境
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
                          <option value="">未发现 {formatToolchainKind(kind)}</option>
                        ) : (
                          options.map((option) => (
                            <option key={option.id} value={option.id}>
                              {option.label} · {formatToolchainSource(option.source)}
                            </option>
                          ))
                        )}
                      </select>
                      <div className="mt-2 space-y-2">
                        <div className="truncate text-[11px] text-muted-foreground" title={selectedOption?.path || ''}>
                          {selectedOption?.path || (isMissing ? `未发现 ${formatToolchainKind(kind)}，请补充本地路径` : '')}
                        </div>
                        {selectedOption && (
                          <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                            <span className="rounded border border-border px-2 py-1">
                              来源: {formatToolchainSource(selectedOption.source)}
                            </span>
                            {selectedOption.version && (
                              <span className="rounded border border-border px-2 py-1">
                                版本提示: {selectedOption.version}
                              </span>
                            )}
                          </div>
                        )}
                      </div>

                      <details className="mt-3 rounded border border-dashed border-border/70 bg-background/40">
                        <summary className="cursor-pointer list-none px-3 py-2 text-xs text-muted-foreground">
                          {showManualInput ? '补充本地路径' : '没有想要的版本？补充本地路径'}
                        </summary>
                        <div className="border-t border-border/60 px-3 py-3">
                          <div className="mb-2 text-[11px] text-muted-foreground">
                            {resolveManualHint(kind)}
                          </div>
                          <div className="flex items-center gap-2">
                            <input
                              value={manualDraft}
                              onChange={(event) => onCustomToolchainDraftChange(kind, event.target.value)}
                              placeholder={`手动指定 ${formatToolchainKind(kind)} 路径`}
                              className="h-9 flex-1 rounded border border-border bg-background px-3 text-sm text-foreground"
                            />
                            <button
                              type="button"
                              onClick={() => onSaveCustomToolchain(kind)}
                              disabled={!manualDraft.trim() || runEnvironmentLoading}
                              className="h-9 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              保存并选中
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
              <div className="mb-2 text-[11px] text-muted-foreground">{resolveConfigSectionTitle(selectedTarget)}</div>
              {selectedConfigFiles.length === 0 ? (
                <div className="text-sm text-muted-foreground">
                  {resolveConfigFilesEmptyText(selectedTarget, availableToolchainKinds)}
                </div>
              ) : (
                <div className="space-y-3">
                  {selectedConfigFiles.map((file) => (
                    <div key={`${file.kind}:${file.path}`} className="rounded border border-border/60 bg-card p-3">
                      <div className="flex flex-wrap items-center gap-2 text-xs">
                        <span className="font-medium text-foreground">{file.label}</span>
                        <span className="rounded border border-border px-2 py-1 text-[11px] text-muted-foreground">
                          来源: {formatToolchainSource(file.source)}
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
                    这些文件即使不在上面的下拉里手动选择，也会被当前运行目标对应的构建工具或运行时自动读取，所以这里做成只读说明，帮助用户看清这次启动真正会生效的配置。
                  </div>
                </div>
              )}
            </div>

            <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
              <div className="mb-2 text-[11px] text-muted-foreground">项目环境变量</div>
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
                  每行一个 `KEY=VALUE`，支持 `#` 注释。{buildInjectedEnvHint(availableToolchainKinds)}
                </div>
                <button
                  type="button"
                  onClick={onSaveEnvVarsDraft}
                  disabled={runEnvironmentLoading}
                  className="h-9 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  保存环境变量
                </button>
              </div>
              <div className="mt-3 rounded border border-border/60 bg-card p-3">
                <div className="mb-2 text-[11px] text-muted-foreground">启动前环境预览</div>
                <pre className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-foreground">
                  {envPreview || '当前未注入额外环境变量'}
                </pre>
              </div>
            </div>

            <div className="mt-4 rounded border border-border/70 bg-background/50 p-3">
              <div className="mb-2 text-[11px] text-muted-foreground">执行命令预览</div>
              <pre className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-foreground">
                {commandPreview || '暂无命令'}
              </pre>
            </div>
          </div>
        </details>

        <div className="rounded border border-border/70 bg-background/50 p-3">
          <div className="mb-2 text-[11px] text-muted-foreground">独立运行终端</div>
          <div className="h-[420px] overflow-hidden rounded border border-border/60 bg-card">
            <EmbeddedTerminalView
              terminal={projectRunTerminal}
              emptyText="先在上面启动实例或选择一个已有实例，再在这里查看它独立的终端"
              client={terminalClient}
              accessToken={accessToken}
              actualTheme={actualTheme}
            />
          </div>
        </div>
      </div>
    </div>
  );
};

export default ProjectRunSettingsPanel;
