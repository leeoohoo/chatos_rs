import { useEffect, useState, type ReactNode } from "react";
import { useAccessControlStore } from "../application/accessControlStore";
import {
  networkAccessLabel,
  type AccessControlBackend,
} from "../domain/accessControl";
import type { AccessControlPort } from "../ports/AccessControlPort";

export function AccessControlView({
  ownerUserId,
  hostReady,
  port,
}: {
  ownerUserId: string;
  hostReady: boolean;
  port: AccessControlPort;
}) {
  const snapshot = useAccessControlStore((state) => state.snapshot);
  const isLoading = useAccessControlStore((state) => state.isLoading);
  const error = useAccessControlStore((state) => state.error);
  const activate = useAccessControlStore((state) => state.activate);
  const suspend = useAccessControlStore((state) => state.suspend);
  const refresh = useAccessControlStore((state) => state.refresh);
  const updateApproval = useAccessControlStore((state) => state.updateApproval);
  const reset = useAccessControlStore((state) => state.reset);
  const [pendingApproval, setPendingApproval] = useState<{ policy: "on_request" | "never"; reviewer: "user" | "auto_review"; title: string; detail: string } | null>(null);

  useEffect(() => {
    if (hostReady) void activate(port, ownerUserId);
    else suspend(ownerUserId);
  }, [activate, hostReady, ownerUserId, port, suspend]);

  useEffect(() => () => reset(), [ownerUserId, reset]);

  if (!hostReady) {
    return <AccessControlStatus title="本地服务尚未就绪" detail="访问控制状态将在 Host 完成握手后开放。" />;
  }
  if (!snapshot && isLoading) {
    return <AccessControlStatus title="正在读取访问控制" detail="正在核对 Windows 实际执行边界。" loading />;
  }
  if (!snapshot) {
    return (
      <AccessControlStatus
        actionLabel="重试"
        detail={error ?? "无法读取 Windows 访问控制状态。"}
        onAction={() => void refresh(port)}
        title="访问控制暂时不可用"
      />
    );
  }

  const { settings } = snapshot;
  const updatePolicy = (policy: "on_request" | "never") => {
    const reviewer = policy === "never" ? "user" : settings.defaultApprovalReviewer === "auto_review" ? "auto_review" : "user";
    if (policy === "never") {
      setPendingApproval({ policy, reviewer, title: "启用完全控制？", detail: "完全控制会让符合策略的命令不再逐次询问。高风险命令仍受运行上下文和工具边界约束。" });
      return;
    }
    void updateApproval(port, policy, reviewer, false);
  };
  const updateReviewer = (reviewer: "user" | "auto_review") => {
    if (reviewer === "auto_review") {
      setPendingApproval({ policy: "on_request", reviewer, title: "启用审批模型？", detail: "低风险且证据完整的操作可由已配置的审批模型处理；高风险或不确定操作仍会交给用户。" });
      return;
    }
    void updateApproval(port, "on_request", reviewer, false);
  };
  return (
    <div className="settings-page access-control-page" aria-label="访问控制">
      <header className="settings-page__title">
        <h2>权限控制</h2>
        {isLoading && <div><span className="mini-spinner" aria-hidden="true" />刷新中…</div>}
      </header>
      {error && (
        <div className="settings-alert" role="alert">
          <span aria-hidden="true">!</span>
          <p>{error}</p>
          <button disabled={isLoading} onClick={() => void refresh(port)} type="button">重试</button>
        </div>
      )}

      <SettingsCard
        icon="▣"
        title="默认任务权限"
        subtitle="新任务在没有项目级覆盖时使用这套策略"
        trailing={<button className="settings-link-button" disabled={isLoading} onClick={() => void refresh(port)} type="button">刷新</button>}
      >
        <div className="access-control-readonly">
          <span aria-hidden="true">✓</span>
          <p><strong>审批策略可编辑</strong><small>文件和网络选项保持锁定，未实现的隔离能力不会伪装成已启用。</small></p>
        </div>
        {settings.permissionConfigurationError && (
          <div className="access-control-warning" role="note">
            <span aria-hidden="true">!</span>
            <p>{settings.permissionConfigurationError}</p>
          </div>
        )}
        <dl className="access-control-policy-list">
          <PolicyRow label="本机权限控制" value={<input aria-label="本机权限控制" checked={settings.enabled} disabled type="checkbox" />} />
          <PolicyRow label="文件权限" value={<select aria-label="文件权限" disabled value={settings.defaultPermissionProfileId}><option value={settings.defaultPermissionProfileId}>{settings.defaultPermissionProfileName}</option></select>} />
          <PolicyRow label="审批策略" value={<select aria-label="审批策略" disabled={isLoading || !settings.editable} onChange={(event) => updatePolicy(event.target.value as "on_request" | "never")} value={settings.defaultApprovalPolicy}><option value="on_request">按需审批</option><option value="never">不请求审批</option></select>} />
          <PolicyRow label="审批人" value={<select aria-label="审批人" disabled={isLoading || !settings.editable || settings.defaultApprovalPolicy === "never"} onChange={(event) => updateReviewer(event.target.value as "user" | "auto_review")} value={settings.defaultApprovalReviewer === "policy" ? "user" : settings.defaultApprovalReviewer}><option value="user">用户</option><option value="auto_review">审批模型</option></select>} />
          <PolicyRow label="网络访问" value={<select aria-label="网络访问" disabled value={settings.defaultNetworkAccess}><option value={settings.defaultNetworkAccess}>{networkAccessLabel(settings.defaultNetworkAccess)}</option></select>} />
          <PolicyRow label="当前 Profile" value={settings.defaultPermissionProfileName} />
          <PolicyRow code label="策略版本" value={settings.policyRevision} />
        </dl>
      </SettingsCard>

      <SettingsCard
        icon="⌘"
        title="运行后端"
        subtitle="当前版本只允许使用本机进程后端"
      >
        <div className="access-control-backends">
          {snapshot.backends.map((backend) => (
            <BackendRow backend={backend} key={backend.backend} />
          ))}
        </div>
      </SettingsCard>
      {pendingApproval && <div className="access-control-confirm"><section aria-modal="true" role="alertdialog"><span aria-hidden="true">!</span><h3>{pendingApproval.title}</h3><p>{pendingApproval.detail}</p>{error && <p className="is-error" role="alert">{error}</p>}<footer><button onClick={() => setPendingApproval(null)} type="button">取消</button><button className="is-primary" disabled={isLoading} onClick={async () => { const pending = pendingApproval; if (await updateApproval(port, pending.policy, pending.reviewer, true)) setPendingApproval(null); }} type="button">确认更改</button></footer></section></div>}
    </div>
  );
}

function PolicyRow({ label, value, code = false }: { label: string; value: ReactNode; code?: boolean }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{code ? <code>{value}</code> : value}</dd>
    </div>
  );
}

function BackendRow({ backend }: { backend: AccessControlBackend }) {
  return (
    <article className="access-control-backend">
      <span className={`access-control-backend__icon access-control-backend__icon--${backend.status}`} aria-hidden="true">
        {backend.status === "ready" ? "✓" : "!"}
      </span>
      <div>
        <header>
          <h3>{backend.label}</h3>
          <span className={`access-control-backend__status access-control-backend__status--${backend.status}`}>{backend.statusLabel}</span>
        </header>
        <p>{backend.message}</p>
        <div className="access-control-capabilities">
          <Capability label="文件隔离" enabled={backend.filesystemIsolation} />
          <Capability label="网络隔离" enabled={backend.networkIsolation} />
          <Capability label="进程树控制" enabled={backend.processTreeControl} />
        </div>
      </div>
    </article>
  );
}

function Capability({ label, enabled }: { label: string; enabled: boolean }) {
  return <span className={enabled ? "is-enabled" : "is-unavailable"}>{enabled ? "✓" : "—"} {label}</span>;
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
        <h3>{title}</h3>
        {trailing}
      </header>
      <div className="settings-card__body">{children}</div>
      <p className="settings-card__footnote">{subtitle}</p>
    </section>
  );
}

function AccessControlStatus({
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
        {loading ? <span className="mini-spinner" aria-hidden="true" /> : <div className="project-state__icon" aria-hidden="true">▣</div>}
        <h2>{title}</h2>
        <p>{detail}</p>
        {actionLabel && onAction && <button className="primary-button" onClick={onAction} type="button">{actionLabel}</button>}
      </div>
    </div>
  );
}
