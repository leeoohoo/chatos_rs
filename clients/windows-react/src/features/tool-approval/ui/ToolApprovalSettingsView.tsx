import { useEffect, useState, type ReactNode } from "react";
import { useToolApprovalSettingsStore } from "../application/toolApprovalSettingsStore";
import { useToolApprovalStore } from "../application/toolApprovalStore";
import {
  allowsToolApprovalDecision,
  riskPresentation,
  type ToolApproval,
  type ToolApprovalDecision,
} from "../domain/toolApproval";
import {
  approvalModePresentation,
  type ToolApprovalMode,
} from "../domain/toolApprovalSettings";
import type { ToolApprovalPort } from "../ports/ToolApprovalPort";
import type { ToolApprovalSettingsPort } from "../ports/ToolApprovalSettingsPort";

const HISTORY_PAGE_SIZE = 10;

export function ToolApprovalSettingsView({
  ownerUserId,
  hostReady,
  approvalPort,
  settingsPort,
}: {
  ownerUserId: string;
  hostReady: boolean;
  approvalPort: ToolApprovalPort;
  settingsPort: ToolApprovalSettingsPort;
}) {
  const settings = useToolApprovalSettingsStore((state) => state.settings);
  const isLoading = useToolApprovalSettingsStore((state) => state.isLoading);
  const isMutating = useToolApprovalSettingsStore((state) => state.isMutating);
  const loadError = useToolApprovalSettingsStore((state) => state.loadError);
  const mutationError = useToolApprovalSettingsStore((state) => state.mutationError);
  const activate = useToolApprovalSettingsStore((state) => state.activate);
  const suspend = useToolApprovalSettingsStore((state) => state.suspend);
  const load = useToolApprovalSettingsStore((state) => state.load);
  const updateMode = useToolApprovalSettingsStore((state) => state.updateMode);
  const updateModel = useToolApprovalSettingsStore((state) => state.updateModel);
  const clearMutationError = useToolApprovalSettingsStore((state) => state.clearMutationError);
  const reset = useToolApprovalSettingsStore((state) => state.reset);
  const approvals = useToolApprovalStore((state) => state.approvals);
  const approvalsHaveLoaded = useToolApprovalStore((state) => state.hasLoaded);
  const approvalsAreLoading = useToolApprovalStore((state) => state.isLoading);
  const approvalLoadError = useToolApprovalStore((state) => state.loadError);
  const refreshApprovals = useToolApprovalStore((state) => state.refresh);
  const [proposedMode, setProposedMode] = useState<ToolApprovalMode | null>(null);
  const [historyPage, setHistoryPage] = useState(0);

  useEffect(() => {
    if (hostReady) void activate(settingsPort, ownerUserId);
    else suspend(ownerUserId);
  }, [activate, hostReady, ownerUserId, settingsPort, suspend]);

  useEffect(() => () => reset(), [ownerUserId, reset]);

  useEffect(() => {
    const pageCount = Math.max(1, Math.ceil((settings?.history.length ?? 0) / HISTORY_PAGE_SIZE));
    setHistoryPage((page) => Math.min(page, pageCount - 1));
  }, [settings?.history.length]);

  if (!hostReady) {
    return <SettingsStatus title="本地服务尚未就绪" detail="审批设置将在 Host 完成握手后开放。" />;
  }
  if (!settings && isLoading) {
    return <SettingsStatus title="正在读取审批设置" detail="正在加载本机策略、模型与审批记录。" loading />;
  }
  if (!settings) {
    return (
      <SettingsStatus
        title="审批设置暂时不可用"
        detail={loadError ?? "无法读取本机审批设置。"}
        actionLabel="重试"
        onAction={() => void load(settingsPort)}
      />
    );
  }

  const selectedModel = settings.models.find((model) => model.id === settings.selectedModelId);
  const history = settings.history.slice(
    historyPage * HISTORY_PAGE_SIZE,
    (historyPage + 1) * HISTORY_PAGE_SIZE,
  );
  const historyPages = Math.max(1, Math.ceil(settings.history.length / HISTORY_PAGE_SIZE));
  const selectMode = (mode: ToolApprovalMode) => {
    if (mode === settings.defaultMode || isMutating) return;
    if (mode === "request_approval") {
      void updateMode(settingsPort, mode, false);
    } else {
      setProposedMode(mode);
    }
  };

  return (
    <div className="settings-page" aria-label="命令审批设置">
      {(loadError || mutationError) && (
        <div className="settings-alert" role="alert">
          <span aria-hidden="true">!</span>
          <p>{mutationError ?? loadError}</p>
          {mutationError ? (
            <button onClick={clearMutationError} type="button">关闭</button>
          ) : (
            <button disabled={isMutating || isLoading} onClick={() => void load(settingsPort)} type="button">重试</button>
          )}
        </div>
      )}

      {approvalLoadError && (
        <div className="settings-alert" role="status">
          <span aria-hidden="true">!</span>
          <p>{approvalLoadError}</p>
          <button disabled={approvalsAreLoading} onClick={() => void refreshApprovals(approvalPort)} type="button">重试队列</button>
        </div>
      )}

      <SettingsCard
        icon="✓"
        title="默认审批级别"
        subtitle="项目没有单独策略时使用这里的默认值"
      >
        <div className="approval-mode-picker" role="group" aria-label="审批模式">
          {(Object.keys(approvalModePresentation) as ToolApprovalMode[]).map((mode) => (
            <button
              aria-pressed={settings.defaultMode === mode}
              className={settings.defaultMode === mode ? "is-selected" : ""}
              disabled={isMutating}
              key={mode}
              onClick={() => selectMode(mode)}
              type="button"
            >{approvalModePresentation[mode].label}</button>
          ))}
        </div>
        <p className="settings-help">{approvalModePresentation[settings.defaultMode].description}</p>
      </SettingsCard>

      <SettingsCard
        icon="✦"
        title="本机自动审批 Agent"
        subtitle="模型仅从当前账号已启用且凭据可用的配置中选择"
      >
        <div className="approval-model-grid">
          <label>
            <span>模型</span>
            <select
              disabled={isMutating}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const model = settings.models.find((item) => item.id === modelId);
                void updateModel(
                  settingsPort,
                  modelId,
                  model?.supportsReasoning ? model.defaultThinkingLevel : null,
                );
              }}
              value={settings.selectedModelId ?? ""}
            >
              <option value="">未配置</option>
              {settings.models.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.displayName} · {model.modelName}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>推理等级</span>
            <select
              disabled={isMutating || !selectedModel?.supportsReasoning}
              onChange={(event) => void updateModel(
                settingsPort,
                settings.selectedModelId,
                event.target.value || null,
              )}
              value={settings.selectedThinkingLevel ?? ""}
            >
              {!selectedModel?.supportsReasoning && <option value="">不适用</option>}
              {selectedModel?.thinkingLevels.map((level) => (
                <option key={level} value={level}>{thinkingLabel(level)}</option>
              ))}
            </select>
          </label>
        </div>
        <div className={`reviewer-status ${settings.automaticReviewerReady ? "is-ready" : "is-fallback"}`}>
          <span aria-hidden="true">{settings.automaticReviewerReady ? "✓" : "!"}</span>
          <p>{settings.automaticReviewerDetail}</p>
        </div>
      </SettingsCard>

      <SettingsCard
        icon="◷"
        title="等待处理"
        subtitle="任务会停在这里，直到策略、用户或审批模型作出决定"
      >
        {!approvalsHaveLoaded && approvalsAreLoading ? (
          <div className="settings-empty"><span className="mini-spinner" aria-hidden="true" /> 正在读取等待队列…</div>
        ) : approvalLoadError && approvals.length === 0 ? (
          <div className="settings-empty">等待队列暂时不可用，请重试。</div>
        ) : approvals.length === 0 ? (
          <div className="settings-empty">✓ 当前没有等待审批的操作</div>
        ) : (
          <div className="approval-settings-list">
            {approvals.map((approval) => (
              <PendingApprovalRow
                approval={approval}
                approvalPort={approvalPort}
                key={approval.invocationId}
                onResolved={() => void load(settingsPort)}
              />
            ))}
          </div>
        )}
      </SettingsCard>

      <SettingsCard
        icon="↺"
        title="审批记录"
        subtitle="最近的本机工具决策审计"
        trailing={<button className="settings-link-button" disabled={isMutating || isLoading} onClick={() => void load(settingsPort)} type="button">刷新</button>}
      >
        {history.length === 0 ? (
          <div className="settings-empty">还没有审批记录。</div>
        ) : (
          <div className="approval-history-list">
            {history.map((entry) => (
              <div className="approval-history-row" key={entry.id}>
                <span className={`history-decision history-decision--${entry.decision === "decline" ? "decline" : "accept"}`}>
                  {entry.decision === "decline" ? "×" : "✓"}
                </span>
                <div>
                  {entry.purpose && <strong>{entry.purpose}</strong>}
                  <code>{entry.command}</code>
                  <small>{entry.source} · {formatTime(entry.createdAtUnixMs)}</small>
                </div>
                <span>{decisionLabel(entry.decision)}</span>
              </div>
            ))}
          </div>
        )}
        {historyPages > 1 && (
          <div className="settings-pagination">
            <button disabled={historyPage === 0} onClick={() => setHistoryPage((page) => page - 1)} type="button">上一页</button>
            <span>{historyPage + 1} / {historyPages}</span>
            <button disabled={historyPage + 1 >= historyPages} onClick={() => setHistoryPage((page) => page + 1)} type="button">下一页</button>
          </div>
        )}
      </SettingsCard>

      {proposedMode && (
        <div className="settings-dialog-backdrop">
          <section aria-label="确认更改审批模式" aria-modal="true" className="settings-dialog" role="dialog">
            <span className="settings-dialog__icon" aria-hidden="true">!</span>
            <h2>{proposedMode === "full_control" ? "确认授予完全控制？" : "确认启用自动审批？"}</h2>
            <p>{proposedMode === "full_control"
              ? "任务将可以直接执行高风险操作。请只在完全信任当前设备、项目与任务时使用。"
              : "审批模型 runtime 不可用时仍会请求你的确认，不会静默放行。"}</p>
            <div>
              <button onClick={() => setProposedMode(null)} type="button">取消</button>
              <button
                className="danger-button"
                disabled={isMutating}
                onClick={() => {
                  const mode = proposedMode;
                  setProposedMode(null);
                  void updateMode(settingsPort, mode, true);
                }}
                type="button"
              >{proposedMode === "full_control" ? "授予完全控制" : "启用自动审批"}</button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function PendingApprovalRow({
  approval,
  approvalPort,
  onResolved,
}: {
  approval: ToolApproval;
  approvalPort: ToolApprovalPort;
  onResolved(): void;
}) {
  const actionId = useToolApprovalStore((state) => state.actionIds[approval.invocationId]);
  const error = useToolApprovalStore((state) => state.approvalErrors[approval.invocationId]);
  const decide = useToolApprovalStore((state) => state.decide);
  const risk = riskPresentation(approval.risk);
  const isResolving = actionId != null;
  const resolve = async (decision: ToolApprovalDecision) => {
    if (await decide(approvalPort, approval, decision)) onResolved();
  };
  return (
    <article className="approval-settings-row">
      <header>
        <span className={`approval-risk approval-risk--${approval.risk}`}>{risk.symbol} {risk.label}</span>
        <span>{approval.source}</span>
        <time>{formatTime(approval.createdAtUnixMs)}</time>
      </header>
      {approval.purpose && <strong>{approval.purpose}</strong>}
      <details open>
        <summary>命令详情</summary>
        <pre>{approval.command}</pre>
      </details>
      <code>{approval.cwd}</code>
      {approval.reason && <p>{approval.reason}</p>}
      {error && <p className="approval-settings-row__error" role="alert">{error}</p>}
      <footer>
        {isResolving && <span className="button-spinner" aria-label="正在处理审批" />}
        {allowsToolApprovalDecision(approval, "decline") && (
          <button disabled={isResolving} onClick={() => void resolve("decline")} type="button">拒绝</button>
        )}
        <span />
        {allowsToolApprovalDecision(approval, "accept") && (
          <button disabled={isResolving} onClick={() => void resolve("accept")} type="button">仅本次允许</button>
        )}
        {allowsToolApprovalDecision(approval, "acceptForSession") && (
          <button className="primary-button" disabled={isResolving} onClick={() => void resolve("acceptForSession")} type="button">本次会话允许</button>
        )}
      </footer>
    </article>
  );
}

function SettingsCard({
  icon,
  title,
  subtitle,
  trailing,
  children,
}: {
  icon: string;
  title: string;
  subtitle: string;
  trailing?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="settings-card">
      <header className="settings-card__header">
        <span aria-hidden="true">{icon}</span>
        <div><h2>{title}</h2><p>{subtitle}</p></div>
        {trailing}
      </header>
      <div className="settings-card__body">{children}</div>
    </section>
  );
}

function SettingsStatus({
  title,
  detail,
  loading = false,
  actionLabel,
  onAction,
}: {
  title: string;
  detail: string;
  loading?: boolean;
  actionLabel?: string;
  onAction?(): void;
}) {
  return (
    <div className="project-state">
      <div className="project-state__card">
        {loading ? <span className="mini-spinner" aria-hidden="true" /> : <div className="project-state__icon" aria-hidden="true">!</div>}
        <h2>{title}</h2>
        <p>{detail}</p>
        {actionLabel && onAction && <button className="primary-button" onClick={onAction} type="button">{actionLabel}</button>}
      </div>
    </div>
  );
}

function thinkingLabel(level: string) {
  return ({ none: "关闭", low: "低", medium: "中", high: "高", xhigh: "极高" } as Record<string, string>)[level] ?? level;
}

function decisionLabel(decision: ToolApprovalDecision) {
  return {
    decline: "已拒绝",
    accept: "仅本次允许",
    acceptForSession: "本次会话允许",
  }[decision];
}

function formatTime(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}
