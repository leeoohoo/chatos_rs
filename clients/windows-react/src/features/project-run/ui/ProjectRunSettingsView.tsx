import { useEffect, useState, type ReactNode } from "react";
import { useProjectRunStore, type ProjectRunNotice } from "../application/projectRunStore";
import {
  requiredProjectRunToolchains,
  selectedProjectRunTarget,
  type ProjectRunEnvironment,
  type ProjectRunInstance,
  type ProjectRunToolchainOption,
} from "../domain/projectRun";
import type { ProjectRunPort } from "../ports/ProjectRunPort";

export function ProjectRunSettingsView({
  ownerUserId,
  projectId,
  projectName,
  port,
  onProjectRemoved,
}: {
  ownerUserId: string;
  projectId: string;
  projectName: string;
  port: ProjectRunPort;
  onProjectRemoved(projectId: string): void;
}) {
  const store = useProjectRunStore();
  const [expandedInstances, setExpandedInstances] = useState<Set<string>>(new Set());
  const [environmentExpanded, setEnvironmentExpanded] = useState(false);
  const [removeConfirmation, setRemoveConfirmation] = useState(false);

  useEffect(() => {
    let mounted = true;
    void store.activate(port, ownerUserId, projectId).then(() => {
      if (mounted) void useProjectRunStore.getState().monitor(port);
    });
    return () => {
      mounted = false;
      useProjectRunStore.getState().deactivate();
    };
  }, [ownerUserId, port, projectId]); // Store actions are stable; subscribing to the whole store must not restart activation.

  useEffect(() => {
    setExpandedInstances(new Set());
    setEnvironmentExpanded(false);
    setRemoveConfirmation(false);
  }, [projectId]);

  const selectedTarget = selectedProjectRunTarget(store.catalog);
  const requiredToolchains = requiredProjectRunToolchains(store.catalog);
  const busy = store.isMutating;
  const retry = () => {
    void store.refresh(port).then(() => void useProjectRunStore.getState().monitor(port));
  };

  if (!store.catalog && store.isLoading) {
    return <ProjectRunStatus loading title="正在分析项目" detail="正在识别运行目标和本机工具链。" />;
  }
  if (!store.catalog || !store.environment || !store.runState) {
    return (
      <ProjectRunStatus
        actionLabel="重试"
        detail={store.errorMessage ?? "无法读取项目运行设置。"}
        onAction={retry}
        title="项目设置暂时不可用"
      />
    );
  }

  return (
    <div className="project-run-scroll" aria-label="项目设置">
      <div className="project-run-page">
        <header className="project-run-header">
          <div>
            <h2>{projectName}</h2>
            <p><span aria-hidden="true">▱</span> 本机项目目录</p>
            <div className="project-run-badges">
              <StatusBadge tone={store.runState.isRunning ? "success" : "neutral"}>
                {runStatusLabel(store.runState.status)}
              </StatusBadge>
              <StatusBadge tone="neutral">运行目标 {store.catalog.targets.length}</StatusBadge>
            </div>
          </div>
          <div className="project-run-header__actions">
            <button disabled={busy} onClick={() => void store.analyze(port)} type="button">✦ 重新分析</button>
            <button disabled={busy || store.isLoading} onClick={retry} type="button">↻ 刷新</button>
          </div>
        </header>

        {(store.errorMessage || store.monitorWarning) && (
          <Notice tone="error" onDismiss={store.dismissError}>
            {store.errorMessage ?? store.monitorWarning}
          </Notice>
        )}
        {store.notice && <Notice tone="success" onDismiss={store.dismissNotice}>{noticeLabel(store.notice)}</Notice>}

        <SettingsCard icon="●" title="宠物快捷访问" subtitle="从桌面宠物快速进入常用项目">
          <div className="project-run-setting-row">
            <div><strong>设为常用项目</strong><p>开启后，单击桌面宠物即可查看最近消息并直接发送新消息。</p></div>
            <label className="apple-switch">
              <input
                aria-label="设为宠物常用项目"
                checked={store.isFavorite}
                disabled={busy}
                onChange={(event) => void store.setFavorite(port, event.target.checked)}
                type="checkbox"
              />
              <span aria-hidden="true" />
            </label>
          </div>
        </SettingsCard>

        <SettingsCard icon="✓" title="运行预检" subtitle="启动前核对目标与本机依赖">
          {store.environment.validationIssues.length === 0 ? (
            <div className="project-run-ready"><span aria-hidden="true">✓</span><strong>没有发现阻塞问题</strong></div>
          ) : (
            <div className="project-run-issues">
              {store.environment.validationIssues.map((issue) => (
                <article className={`project-run-issue project-run-issue--${issue.kind}`} key={issue.id}>
                  <span aria-hidden="true">{issue.kind === "warning" ? "!" : "×"}</span>
                  <div><strong>{issue.message}</strong>{issue.path && <code>{issue.path}</code>}{issue.hint && <p>{issue.hint}</p>}</div>
                </article>
              ))}
            </div>
          )}
        </SettingsCard>

        <SettingsCard icon="▶" title="运行目标" subtitle="选择默认入口并启动独立实例">
          {store.catalog.targets.length === 0 ? (
            <EmptySection symbol="▷" title="没有识别到运行目标" detail="点击“重新分析”扫描项目入口和清单文件。" />
          ) : (
            <>
              <label className="project-run-select-field">
                <span>默认目标</span>
                <select
                  aria-label="默认运行目标"
                  disabled={busy}
                  onChange={(event) => void store.selectTarget(port, event.target.value)}
                  value={selectedTarget?.id ?? ""}
                >
                  {store.catalog.targets.map((target) => <option key={target.id} value={target.id}>{target.label}</option>)}
                </select>
              </label>
              {selectedTarget && (
                <>
                  <dl className="project-run-definition">
                    <Definition label="命令" value={selectedTarget.command} />
                    <Definition label="工作目录" value={selectedTarget.cwd} />
                    {selectedTarget.language && <Definition label="语言" value={selectedTarget.language} />}
                    <Definition label="来源" value={selectedTarget.source} />
                    {selectedTarget.manifestPath && <Definition label="清单" value={selectedTarget.manifestPath} />}
                    {selectedTarget.entrypoint && <Definition label="入口" value={selectedTarget.entrypoint} />}
                  </dl>
                  <div className="project-run-card-actions">
                    <button
                      className="primary-button"
                      disabled={busy || store.environment.validationIssues.length > 0}
                      onClick={() => void store.start(port)}
                      type="button"
                    >{store.mutationKind === "start" ? "启动中…" : "▶ 启动新实例"}</button>
                  </div>
                </>
              )}
            </>
          )}
        </SettingsCard>

        <SettingsCard icon="▤" title="运行实例" subtitle="每次启动都会创建独立的本机进程">
          {store.runState.instances.length === 0 ? (
            <EmptySection symbol="▷" title="还没有运行实例" detail="在上方选择运行目标，然后点击“启动新实例”。" />
          ) : (
            <div className="project-run-instances">
              {store.runState.instances.map((instance) => (
                <RunInstanceCard
                  busy={busy}
                  expanded={expandedInstances.has(instance.id)}
                  instance={instance}
                  key={instance.id}
                  onDelete={() => void store.deleteInstance(port, instance.id)}
                  onStop={() => void store.stop(port, instance.id)}
                  onToggle={() => setExpandedInstances((current) => toggleSet(current, instance.id))}
                />
              ))}
            </div>
          )}
        </SettingsCard>

        <SettingsCard icon="⌘" title="启动所需环境" subtitle="工具链由原生层发现，凭据不会进入 WebView">
          <ReadinessSummary environment={store.environment} targetLabel={selectedTarget?.label ?? "当前运行目标"} />
          {requiredToolchains.length === 0 ? (
            <div className="project-run-ready"><span aria-hidden="true">✓</span><strong>此目标不依赖额外开发工具，可以直接启动。</strong></div>
          ) : (
            <div className="project-run-toolchains">
              {requiredToolchains.map((kind) => (
                <ToolchainRow
                  key={kind}
                  kind={kind}
                  options={store.environment!.toolchainOptions[kind] ?? []}
                  selectedId={store.selectedToolchains[kind] ?? ""}
                  onChange={(value) => store.setToolchain(kind, value)}
                />
              ))}
            </div>
          )}
          {requiredToolchains.length > 0 && (
            <div className="project-run-inline-actions"><p>“自动”会使用原生层发现的默认程序。</p><button disabled={busy} onClick={() => void store.saveEnvironment(port)} type="button">保存工具选择</button></div>
          )}
          <div className="project-run-disclosure">
            <button aria-expanded={environmentExpanded} onClick={() => setEnvironmentExpanded(!environmentExpanded)} type="button">
              <span>{environmentExpanded ? "⌄" : "›"}</span><strong>高级设置：环境变量</strong><small>只有项目明确要求 PORT、NODE_ENV 等变量时才需要设置。</small>
            </button>
            {environmentExpanded && (
              <div className="project-run-environment-editor">
                <p>值只会传给本机项目进程并保存在系统凭据库中；已保存的 Secret 不会回显到页面。</p>
                {store.environmentVariables.length === 0 && <p className="project-run-empty-copy">当前没有自定义环境变量。</p>}
                {store.environmentVariables.map((variable) => (
                  <div className="project-run-environment-row" key={variable.id}>
                    <input
                      aria-label="环境变量名"
                      onChange={(event) => store.setEnvironmentVariableKey(variable.id, event.target.value)}
                      placeholder="变量名，例如 PORT"
                      spellCheck={false}
                      value={variable.key}
                    />
                    <input
                      aria-label={`${variable.key || "新"} 环境变量值`}
                      onChange={(event) => store.setEnvironmentVariableValue(variable.id, event.target.value)}
                      placeholder={variable.hasStoredValue && !variable.valueDirty ? "已安全保存 · 输入以替换" : "输入安全值"}
                      spellCheck={false}
                      type="password"
                      value={variable.value}
                    />
                    <button aria-label={`删除环境变量 ${variable.key || "新变量"}`} disabled={busy} onClick={() => store.removeEnvironmentVariable(variable.id)} type="button">−</button>
                  </div>
                ))}
                <div className="project-run-card-actions project-run-card-actions--split">
                  <button disabled={busy || store.environmentVariables.length >= 128} onClick={store.addEnvironmentVariable} type="button">＋ 添加环境变量</button>
                  <button className="primary-button" disabled={busy} onClick={() => void store.saveEnvironment(port)} type="button">
                    {store.mutationKind === "environment" ? "保存中…" : "保存环境变量"}
                  </button>
                </div>
              </div>
            )}
          </div>
        </SettingsCard>

        <SettingsCard icon="▧" title="项目配置文件" subtitle="原生 analyzer 提供的安全预览">
          {store.environment.configurationFiles.length === 0 ? (
            <p className="project-run-empty-copy">没有发现与当前运行目标相关的配置文件。</p>
          ) : (
            <div className="project-run-configurations">
              {store.environment.configurationFiles.map((file) => (
                <details key={file.id}>
                  <summary><strong>{file.label}</strong><code>{file.path}</code></summary>
                  {file.preview ? <pre>{file.preview}</pre> : <p>文件过大或不可安全预览。</p>}
                </details>
              ))}
            </div>
          )}
        </SettingsCard>

        <SettingsCard icon="−" title="项目设置" subtitle="管理 ChatOS 中的项目记录">
          <div className="project-run-setting-row">
            <div><strong>从 ChatOS 删除项目</strong><p>只删除项目记录和运行状态，不会删除本机项目文件夹。</p></div>
            <button className="project-run-danger-button" disabled={busy} onClick={() => setRemoveConfirmation(true)} type="button">删除项目</button>
          </div>
        </SettingsCard>
      </div>

      {removeConfirmation && (
        <div className="settings-dialog-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) setRemoveConfirmation(false); }}>
          <section aria-label="删除项目？" aria-modal="true" className="settings-dialog" role="alertdialog">
            <span className="settings-dialog__icon" aria-hidden="true">!</span>
            <h2>删除项目？</h2>
            <p>“{projectName}”会从 ChatOS 中移除，本机项目文件夹不会被删除。</p>
            <div>
              <button disabled={busy} onClick={() => setRemoveConfirmation(false)} type="button">取消</button>
              <button
                className="danger-button"
                disabled={busy}
                onClick={() => void store.removeProject(port).then((removed) => {
                  if (removed) onProjectRemoved(projectId);
                  else setRemoveConfirmation(false);
                })}
                type="button"
              >{store.mutationKind === "removeProject" ? "删除中…" : "删除"}</button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function SettingsCard({ icon, title, subtitle, children }: { icon: string; title: string; subtitle: string; children: ReactNode }) {
  return (
    <section className="settings-card project-run-card">
      <header className="settings-card__header"><span aria-hidden="true">{icon}</span><div><h2>{title}</h2><p>{subtitle}</p></div></header>
      <div className="settings-card__body">{children}</div>
    </section>
  );
}

function Notice({ tone, onDismiss, children }: { tone: "error" | "success"; onDismiss(): void; children: ReactNode }) {
  return (
    <div className={`project-run-notice project-run-notice--${tone}`} role={tone === "error" ? "alert" : "status"}>
      <span aria-hidden="true">{tone === "success" ? "✓" : "!"}</span><p>{children}</p><button aria-label="关闭提示" onClick={onDismiss} type="button">×</button>
    </div>
  );
}

function StatusBadge({ tone, children }: { tone: "success" | "neutral" | "warning" | "error"; children: ReactNode }) {
  return <span className={`project-run-badge project-run-badge--${tone}`}>{children}</span>;
}

function Definition({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd><code>{value}</code></dd></div>;
}

function RunInstanceCard({ instance, expanded, busy, onToggle, onStop, onDelete }: {
  instance: ProjectRunInstance;
  expanded: boolean;
  busy: boolean;
  onToggle(): void;
  onStop(): void;
  onDelete(): void;
}) {
  const tone = instance.isRunning ? "success" : instance.status === "failed" ? "error" : instance.status === "stopping" ? "warning" : "neutral";
  return (
    <article className="project-run-instance">
      <header>
        <span className={`project-run-instance__icon project-run-instance__icon--${tone}`} aria-hidden="true">{instance.isRunning ? "⌁" : ">_"}</span>
        <div><strong>{instance.name}</strong><p><time>{formatRunDate(instance.startedAtUnixMs)}</time><code>{instance.cwd}</code></p></div>
        <StatusBadge tone={tone}>{instanceStatusLabel(instance.status)}</StatusBadge>
        {instance.isRunning && <button disabled={busy} onClick={onStop} type="button">■ 停止</button>}
        <button className="project-run-danger-link" disabled={busy} onClick={onDelete} type="button">删除</button>
      </header>
      <button className="project-run-log-toggle" aria-expanded={expanded} onClick={onToggle} type="button">{expanded ? "⌄ 收起运行日志" : "› 展开运行日志"}</button>
      {expanded && (
        <div className="project-run-log">
          <header><strong>实时运行日志</strong><span>{instance.exitCode != null ? `退出码 ${instance.exitCode}` : instance.isRunning ? "实时更新" : "已结束"}</span></header>
          <pre>{instance.log || "进程已经启动，暂时还没有日志输出。"}</pre>
          {instance.logTruncated && <p>日志已达到 512 KiB 上限，只保留最近输出。</p>}
        </div>
      )}
    </article>
  );
}

function ReadinessSummary({ environment, targetLabel }: { environment: ProjectRunEnvironment; targetLabel: string }) {
  const ready = environment.validationIssues.length === 0;
  return (
    <div className={`project-run-readiness project-run-readiness--${ready ? "ready" : "attention"}`}>
      <span aria-hidden="true">{ready ? "✓" : "!"}</span>
      <div><strong>{ready ? "运行环境已准备好" : "启动前还需要处理环境问题"}</strong><p>{ready ? `已为“${targetLabel}”找到所需的本机工具，通常无需修改自动选择。` : `请先处理上方预检中“${targetLabel}”缺少的工具。`}</p></div>
    </div>
  );
}

function ToolchainRow({ kind, options, selectedId, onChange }: {
  kind: string;
  options: ProjectRunToolchainOption[];
  selectedId: string;
  onChange(value: string): void;
}) {
  const resolved = selectedId ? options.find((option) => option.id === selectedId) : options[0];
  return (
    <article className="project-run-toolchain">
      <span className={resolved ? "is-ready" : "is-missing"} aria-hidden="true">{resolved ? "✓" : "×"}</span>
      <div><strong>{toolchainTitle(kind)}</strong><small>{resolved ? `${resolved.label}${resolved.version ? ` · ${resolved.version}` : ""}` : "未找到可用程序"}</small></div>
      <select aria-label={`选择 ${toolchainTitle(kind)}`} onChange={(event) => onChange(event.target.value)} value={selectedId}>
        <option value="">{options[0] ? `自动 · ${options[0].label}` : "未找到"}</option>
        {options.map((option) => <option key={option.id} value={option.id}>{option.label}{option.version ? ` · ${option.version}` : ""}</option>)}
      </select>
    </article>
  );
}

function EmptySection({ symbol, title, detail }: { symbol: string; title: string; detail: string }) {
  return <div className="project-run-empty"><span aria-hidden="true">{symbol}</span><strong>{title}</strong><p>{detail}</p></div>;
}

function ProjectRunStatus({ title, detail, loading = false, actionLabel, onAction }: {
  title: string;
  detail: string;
  loading?: boolean;
  actionLabel?: string;
  onAction?(): void;
}) {
  return (
    <div className="project-state"><div className="project-state__card">
      {loading ? <span className="mini-spinner" aria-hidden="true" /> : <div className="project-state__icon" aria-hidden="true">⌘</div>}
      <h2>{title}</h2><p>{detail}</p>
      {actionLabel && onAction && <button className="primary-button" onClick={onAction} type="button">{actionLabel}</button>}
    </div></div>
  );
}

function toggleSet(current: Set<string>, id: string) {
  const next = new Set(current);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

function runStatusLabel(status: string) {
  return status === "running" ? "运行中" : status === "idle" ? "空闲" : status;
}

function instanceStatusLabel(status: ProjectRunInstance["status"]) {
  return ({ running: "运行中", stopping: "正在停止", exited: "已结束", failed: "运行失败" } as const)[status];
}

function noticeLabel(notice: ProjectRunNotice) {
  return ({
    analyzed: "项目运行目标已重新分析。",
    defaultTargetSaved: "默认运行目标已保存。",
    environmentSaved: "工具链和环境变量已保存。",
    instanceStarted: "运行实例已启动。",
    stopRequested: "已向实例发送停止信号。",
    instanceDeleted: "运行实例已删除。",
    favoriteSaved: "宠物快捷访问偏好已保存。",
  } as const)[notice];
}

function toolchainTitle(kind: string) {
  return ({ java_home: "JDK", java: "JDK / Java", mvn: "Maven", gradle: "Gradle", python: "Python", node: "Node.js", npm: "npm", pnpm: "pnpm", yarn: "Yarn", cargo: "Cargo", go: "Go", swift: "Swift" } as Record<string, string>)[kind.toLowerCase()] ?? kind;
}

function formatRunDate(value: number) {
  return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(value));
}
