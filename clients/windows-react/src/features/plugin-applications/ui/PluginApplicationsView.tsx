import { useEffect, useState, type ReactNode } from "react";
import { usePluginApplicationStore } from "../application/pluginApplicationStore";
import {
  applicationId,
  requiresPluginProjectContext,
  type PluginApplication,
} from "../domain/pluginApplications";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";
import type { WorkspaceResourceItem } from "../../workspace-index/domain/workspaceIndex";
import { RestrictedPluginFrame } from "./RestrictedPluginFrame";

interface PluginApplicationsViewProps {
  hostReady: boolean;
  ownerUserId: string;
  port: PluginApplicationPort;
  onOpen(application: PluginApplication): void;
}

export function PluginApplicationsView({
  hostReady,
  ownerUserId,
  port,
  onOpen,
}: PluginApplicationsViewProps) {
  const applications = usePluginApplicationStore((state) => state.applications);
  const isLoading = usePluginApplicationStore((state) => state.isLoading);
  const errorMessage = usePluginApplicationStore((state) => state.catalogErrorMessage);
  const activate = usePluginApplicationStore((state) => state.activateCatalog);
  const suspend = usePluginApplicationStore((state) => state.suspendCatalog);
  const refresh = usePluginApplicationStore((state) => state.refreshCatalog);

  useEffect(() => {
    activate(port, ownerUserId, hostReady);
    return () => suspend(ownerUserId);
  }, [activate, hostReady, ownerUserId, port, suspend]);

  return (
    <section className="plugin-applications" aria-label="插件应用">
      <header className="plugin-applications__header">
        <div>
          <h2>应用</h2>
          <p>打开已安装并启用的插件应用。</p>
        </div>
        <button disabled={isLoading || !hostReady} onClick={() => void refresh(port)} type="button">
          <span aria-hidden="true">↻</span> 刷新
        </button>
      </header>

      {isLoading && applications.length === 0 ? (
        <PluginApplicationState icon={<span className="mini-spinner" />} title="正在读取插件应用…" />
      ) : errorMessage && applications.length === 0 ? (
        <PluginApplicationState
          icon="⚠"
          title="应用加载失败"
          description={errorMessage}
          action={<button disabled={!hostReady} onClick={() => void refresh(port)} type="button">重试</button>}
        />
      ) : applications.length === 0 ? (
        <PluginApplicationState
          icon="▦"
          title="还没有插件应用"
          description="请先在插件市场安装一个带页面的插件，并保持它处于启用状态。"
        />
      ) : (
        <>
          {errorMessage && (
            <div className="plugin-applications__warning" role="status">
              <span>⚠</span><p>{errorMessage}</p>
              <button disabled={!hostReady} onClick={() => void refresh(port)} type="button">重试</button>
            </div>
          )}
          <div className="plugin-application-grid">
            {applications.map((application) => (
              <button
                className="plugin-application-card"
                key={applicationId(application)}
                onClick={() => onOpen(application)}
                type="button"
              >
                <span className="plugin-application-card__top">
                  <PluginApplicationIcon application={application} />
                  <span aria-hidden="true" className="plugin-application-card__open">↗</span>
                </span>
                <span className="plugin-application-card__copy">
                  <strong>{application.displayName}</strong>
                  <small>{application.description || "插件应用"}</small>
                </span>
                <span className="plugin-application-card__runtime">
                  <span aria-hidden="true">{application.requiresLocalRuntime ? "ϟ" : "▤"}</span>
                  {application.requiresLocalRuntime ? "本地服务" : "内嵌页面"}
                </span>
              </button>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

interface PluginApplicationHostViewProps {
  application: PluginApplication;
  hostReady: boolean;
  ownerUserId: string;
  port: PluginApplicationPort;
  projects: WorkspaceResourceItem[];
  onBack(): void;
}

export function PluginApplicationHostView({
  application,
  hostReady,
  ownerUserId,
  port,
  projects,
  onBack,
}: PluginApplicationHostViewProps) {
  const activeApplication = usePluginApplicationStore((state) => state.activeApplication);
  const presentationId = usePluginApplicationStore((state) => state.presentationId);
  const launchGeneration = usePluginApplicationStore((state) => state.launchGeneration);
  const selectedContext = usePluginApplicationStore((state) => state.selectedContext);
  const phase = usePluginApplicationStore((state) => state.phase);
  const launch = usePluginApplicationStore((state) => state.launch);
  const errorMessage = usePluginApplicationStore((state) => state.launchErrorMessage);
  const activate = usePluginApplicationStore((state) => state.activateApplication);
  const deactivate = usePluginApplicationStore((state) => state.deactivateApplication);
  const selectContext = usePluginApplicationStore((state) => state.selectContext);
  const retry = usePluginApplicationStore((state) => state.retry);
  const chooseAnotherContext = usePluginApplicationStore((state) => state.chooseAnotherContext);
  const [reloadToken, setReloadToken] = useState(0);
  const id = applicationId(application);

  useEffect(() => {
    activate(port, ownerUserId, hostReady, application);
    return () => deactivate(port, ownerUserId, id);
  }, [activate, application, deactivate, hostReady, id, ownerUserId, port]);

  const selectedProject = selectedContext?.kind === "project"
    ? projects.find((project) => project.id === selectedContext.projectId)
    : undefined;
  const acceptedApplication = activeApplication && applicationId(activeApplication) === id;
  const showsContext = acceptedApplication
    && requiresPluginProjectContext(application)
    && phase !== "choosingContext";

  return (
    <section className="plugin-application-host" aria-label={`${application.displayName} 插件应用`}>
      <header className="plugin-application-host__toolbar">
        <button aria-label="返回应用列表" className="plugin-toolbar-button" onClick={onBack} type="button">‹</button>
        <PluginApplicationIcon application={application} compact />
        <strong>{application.displayName}</strong>
        {showsContext && (
          <>
            <span className="plugin-application-host__context">
              {selectedProject?.title ?? "公共项目"}
            </span>
            <button className="plugin-application-host__switch" onClick={() => chooseAnotherContext(port)} type="button">
              切换
            </button>
          </>
        )}
        <span className="plugin-application-host__spacer" />
        {launch && (
          <button
            aria-label="重新载入应用"
            className="plugin-toolbar-button"
            onClick={() => setReloadToken((value) => value + 1)}
            type="button"
          >↻</button>
        )}
      </header>

      <div className="plugin-application-host__content">
        {acceptedApplication && phase === "choosingContext" ? (
          <PluginContextPicker
            allowsDevice={application.missingContext === "device"}
            projects={projects}
            onDevice={() => selectContext(port, { kind: "device" })}
            onProject={(projectId) => selectContext(port, { kind: "project", projectId })}
          />
        ) : acceptedApplication && phase === "ready" && launch && presentationId ? (
          <RestrictedPluginFrame
            generation={launchGeneration}
            launch={launch}
            ownerUserId={ownerUserId}
            port={port}
            presentationId={presentationId}
            reloadToken={reloadToken}
          />
        ) : acceptedApplication && phase === "failed" ? (
          <PluginApplicationState
            icon="⚠"
            title="应用无法打开"
            description={errorMessage ?? "插件应用暂时无法打开。"}
            action={(
              <div className="plugin-state-actions">
                {selectedContext && hostReady && <button onClick={() => retry(port)} type="button">重试</button>}
                <button className="is-secondary" onClick={onBack} type="button">返回应用列表</button>
              </div>
            )}
          />
        ) : (
          <PluginApplicationState icon={<span className="mini-spinner" />} title="正在启动插件应用…" />
        )}
      </div>
    </section>
  );
}

function PluginContextPicker({
  allowsDevice,
  projects,
  onDevice,
  onProject,
}: {
  allowsDevice: boolean;
  projects: WorkspaceResourceItem[];
  onDevice(): void;
  onProject(projectId: string): void;
}) {
  return (
    <div className="plugin-context-picker">
      <header>
        <h2>选择应用项目</h2>
        <p>插件会为每个 ChatOS 用户和项目使用独立的数据目录。</p>
      </header>
      <div className="plugin-context-picker__list">
        {allowsDevice && (
          <ContextButton
            icon="▣"
            title="公共项目"
            subtitle="用于没有关联 ChatOS 项目的图形和数据"
            onClick={onDevice}
          />
        )}
        {projects.map((project) => (
          <ContextButton
            icon="▱"
            key={project.id}
            title={project.title}
            subtitle={project.subtitle ?? "ChatOS 项目"}
            onClick={() => onProject(project.id)}
          />
        ))}
        {!allowsDevice && projects.length === 0 && (
          <p className="plugin-context-picker__empty">还没有可用项目，请先创建一个项目。</p>
        )}
      </div>
    </div>
  );
}

function ContextButton({
  icon,
  title,
  subtitle,
  onClick,
}: {
  icon: string;
  title: string;
  subtitle: string;
  onClick(): void;
}) {
  return (
    <button className="plugin-context-button" onClick={onClick} type="button">
      <span aria-hidden="true" className="plugin-context-button__icon">{icon}</span>
      <span><strong>{title}</strong><small>{subtitle}</small></span>
      <span aria-hidden="true" className="plugin-context-button__chevron">›</span>
    </button>
  );
}

function PluginApplicationIcon({
  application,
  compact = false,
}: {
  application: PluginApplication;
  compact?: boolean;
}) {
  return (
    <span
      aria-hidden="true"
      className={`plugin-application-icon ${compact ? "plugin-application-icon--compact" : ""}`}
      style={application.brandColor ? { background: application.brandColor } : undefined}
    >▦</span>
  );
}

function PluginApplicationState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="plugin-application-state">
      <span className="plugin-application-state__icon" aria-hidden="true">{icon}</span>
      <h2>{title}</h2>
      {description && <p>{description}</p>}
      {action}
    </div>
  );
}
