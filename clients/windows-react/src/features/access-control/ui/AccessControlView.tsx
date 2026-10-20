import { useEffect, type ReactNode } from "react";
import { useAccessControlStore } from "../application/accessControlStore";
import {
  approvalPolicyLabel,
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
  const reset = useAccessControlStore((state) => state.reset);

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
  return (
    <div className="settings-page access-control-page" aria-label="访问控制">
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
        subtitle="新任务使用当前 Windows 本机执行边界"
        trailing={<button className="settings-link-button" disabled={isLoading} onClick={() => void refresh(port)} type="button">刷新</button>}
      >
        <div className="access-control-readonly">
          <span aria-hidden="true">✓</span>
          <p><strong>系统强制 · 只读</strong><small>未实现的隔离能力不会作为可编辑策略展示。</small></p>
        </div>
        {settings.permissionConfigurationError && (
          <div className="access-control-warning" role="note">
            <span aria-hidden="true">!</span>
            <p>{settings.permissionConfigurationError}</p>
          </div>
        )}
        <dl className="access-control-policy-list">
          <PolicyRow label="本机权限控制" value={settings.enabled ? "已启用" : "未启用"} />
          <PolicyRow label="文件权限" value={settings.defaultPermissionProfileName} />
          <PolicyRow label="审批策略" value={approvalPolicyLabel(snapshot)} />
          <PolicyRow label="网络访问" value={networkAccessLabel(settings.defaultNetworkAccess)} />
          <PolicyRow code label="策略版本" value={settings.policyRevision} />
        </dl>
      </SettingsCard>

      <SettingsCard
        icon="⌘"
        title="运行后端"
        subtitle="能力标记只代表当前版本已经实际执行的保护"
      >
        <div className="access-control-backends">
          {snapshot.backends.map((backend) => (
            <BackendRow backend={backend} key={backend.backend} />
          ))}
        </div>
      </SettingsCard>
    </div>
  );
}

function PolicyRow({ label, value, code = false }: { label: string; value: string; code?: boolean }) {
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
        <div><h2>{title}</h2><p>{subtitle}</p></div>
        {trailing}
      </header>
      <div className="settings-card__body">{children}</div>
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
