import { useEffect } from "react";
import { useSystemPermissionsStore } from "../application/systemPermissionsStore";
import {
  isPermissionReady,
  permissionTone,
  type SystemPermission,
} from "../domain/systemPermissions";
import type { SystemPermissionsPort } from "../ports/SystemPermissionsPort";

export function SystemPermissionsView({
  ownerUserId,
  hostReady,
  port,
}: {
  ownerUserId: string;
  hostReady: boolean;
  port: SystemPermissionsPort;
}) {
  const snapshot = useSystemPermissionsStore((state) => state.snapshot);
  const isLoading = useSystemPermissionsStore((state) => state.isLoading);
  const requestingPermissionId = useSystemPermissionsStore((state) => state.requestingPermissionId);
  const loadError = useSystemPermissionsStore((state) => state.loadError);
  const mutationError = useSystemPermissionsStore((state) => state.mutationError);
  const activate = useSystemPermissionsStore((state) => state.activate);
  const suspend = useSystemPermissionsStore((state) => state.suspend);
  const refresh = useSystemPermissionsStore((state) => state.refresh);
  const request = useSystemPermissionsStore((state) => state.request);
  const clearMutationError = useSystemPermissionsStore((state) => state.clearMutationError);
  const reset = useSystemPermissionsStore((state) => state.reset);

  useEffect(() => {
    if (hostReady) void activate(port, ownerUserId);
    else suspend(ownerUserId);
  }, [activate, hostReady, ownerUserId, port, suspend]);

  useEffect(() => () => reset(), [ownerUserId, reset]);

  if (!hostReady) {
    return <PermissionStatus title="本地服务尚未就绪" detail="运行与权限状态将在 Host 完成握手后开放。" />;
  }
  if (!snapshot && isLoading) {
    return <PermissionStatus title="正在检查系统权限" detail="正在读取 Windows 原生能力边界。" loading />;
  }
  if (!snapshot) {
    return (
      <PermissionStatus
        actionLabel="重试"
        detail={loadError ?? "无法读取 Windows 系统权限状态。"}
        onAction={() => void refresh(port)}
        title="系统权限暂时不可用"
      />
    );
  }

  return (
    <div className="settings-page runtime-permissions-page" aria-label="运行与权限">
      <header className="settings-page__title">
        <h2>运行与系统权限</h2>
        {(isLoading || requestingPermissionId != null) && (
          <div>
            <span className="mini-spinner" aria-hidden="true" />
            {requestingPermissionId != null ? "正在打开…" : "检测中…"}
          </div>
        )}
      </header>
      {(loadError || mutationError) && (
        <div className="settings-alert" role="alert">
          <span aria-hidden="true">!</span>
          <p>{mutationError ?? loadError}</p>
          {mutationError ? (
            <button onClick={clearMutationError} type="button">关闭</button>
          ) : (
            <button disabled={isLoading || requestingPermissionId != null} onClick={() => void refresh(port)} type="button">重试</button>
          )}
        </div>
      )}

      <section className="settings-card">
        <header className="settings-card__header">
          <span aria-hidden="true">⬡</span>
          <h3>系统权限</h3>
          <button
            className="settings-link-button"
            disabled={isLoading || requestingPermissionId != null}
            onClick={() => void refresh(port)}
            type="button"
          >重新检测</button>
        </header>
        <div className="settings-card__body">
          <p className="settings-help">每项权限都会说明准确的授权目标和当前状态。</p>
          <div className="system-permission-list">
            {snapshot.items.map((permission) => (
              <PermissionRow
                isRequesting={requestingPermissionId === permission.id}
                key={permission.id}
                onRequest={() => void request(port, permission.id)}
                permission={permission}
              />
            ))}
          </div>
        </div>
        <p className="settings-card__footnote">{snapshot.platformLabel} 下 Skills 与 MCP 的系统访问状态</p>
      </section>
    </div>
  );
}

function PermissionRow({
  permission,
  isRequesting,
  onRequest,
}: {
  permission: SystemPermission;
  isRequesting: boolean;
  onRequest(): void;
}) {
  const tone = permissionTone(permission.status);
  return (
    <article className="system-permission-row">
      <span className={`system-permission-row__icon system-permission-row__icon--${tone}`} aria-hidden="true">
        {permissionSymbol(permission.id)}
      </span>
      <div className="system-permission-row__content">
        <header>
          <h3>{permission.label}</h3>
          <span className={`permission-status permission-status--${tone}`}>{permission.statusLabel}</span>
        </header>
        <p>{permission.summary}</p>
        {permission.note && <small>{permission.note}</small>}
        {permission.lastError && <small className="system-permission-row__error">{permission.lastError}</small>}
      </div>
      {permission.canRequest && !isPermissionReady(permission.status) && (
        <button disabled={isRequesting} onClick={onRequest} type="button">
          {isRequesting ? "正在打开…" : permission.requestLabel}
        </button>
      )}
    </article>
  );
}

function PermissionStatus({
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
        {loading ? <span className="mini-spinner" aria-hidden="true" /> : <div className="project-state__icon" aria-hidden="true">⌁</div>}
        <h2>{title}</h2>
        <p>{detail}</p>
        {actionLabel && onAction && <button className="primary-button" onClick={onAction} type="button">{actionLabel}</button>}
      </div>
    </div>
  );
}

function permissionSymbol(id: string) {
  if (id.includes("terminal")) return ">_";
  if (id.includes("computer") || id.includes("accessibility")) return "⌘";
  if (id.includes("screen")) return "▣";
  if (id.includes("file") || id.includes("folder")) return "▱";
  return "◇";
}
