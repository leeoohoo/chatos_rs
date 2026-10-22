import { useEffect, useMemo, useState } from "react";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { usePluginSettingsStore } from "../application/pluginSettingsStore";
import { filterPlugins, type ManagedPlugin } from "../domain/pluginSettings";
import type { PluginSettingsPort } from "../ports/PluginSettingsPort";
import "./pluginSettings.css";

export function PluginSettingsView({ ownerUserId, port, onOpenInstallationAssistant }: {
  ownerUserId: string;
  port: PluginSettingsPort;
  onOpenInstallationAssistant(): void;
}) {
  const snapshot = usePluginSettingsStore((state) => state.snapshot);
  const isLoading = usePluginSettingsStore((state) => state.isLoading);
  const operatingPluginId = usePluginSettingsStore((state) => state.operatingPluginId);
  const error = usePluginSettingsStore((state) => state.error);
  const notice = usePluginSettingsStore((state) => state.notice);
  const load = usePluginSettingsStore((state) => state.load);
  const updateEnabled = usePluginSettingsStore((state) => state.updateEnabled);
  const uninstall = usePluginSettingsStore((state) => state.uninstall);
  const clearMessage = usePluginSettingsStore((state) => state.clearMessage);
  const reset = usePluginSettingsStore((state) => state.reset);
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const english = language === "en-US";
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<ManagedPlugin | null>(null);

  useEffect(() => {
    void load(port, ownerUserId);
    return () => reset();
  }, [load, ownerUserId, port, reset]);

  const plugins = useMemo(() => filterPlugins(snapshot?.plugins ?? [], query), [query, snapshot?.plugins]);
  if (!snapshot && isLoading) return <PluginStatus loading title={english ? "Syncing plugins" : "正在同步 Plugin"} detail={english ? "Reading Marketplace and the native installation catalog." : "正在读取 Marketplace 与本机安装目录。"} />;
  if (!snapshot) return <PluginStatus title={english ? "Plugins unavailable" : "Plugin 暂时不可用"} detail={error ?? (english ? "Unable to read the plugin catalog." : "无法读取 Plugin 目录。")} action={english ? "Retry" : "重试"} onAction={() => void load(port, ownerUserId)} />;

  return <div className="plugin-settings-page">
    {(error || notice) && <div className={`plugin-settings-banner ${error ? "is-error" : ""}`} role={error ? "alert" : "status"}><span>{error ? "!" : "✓"}</span><p>{error ?? notice}</p><button onClick={clearMessage} type="button">×</button></div>}

    <section className="plugin-settings-hero">
      <div><span aria-hidden="true">◈</span><div><h2>{english ? "Plugins & Skills" : "插件与 Skills"}</h2><p>{english ? "Search by plugin, Skill, publisher, or category." : "按 Plugin、Skill、发布者或分类查找。"}</p></div></div>
      <div className="plugin-settings-actions"><button onClick={onOpenInstallationAssistant} type="button">◇ {english ? "Installation Assistant" : "安装助手"}</button><button disabled={isLoading} onClick={() => void load(port, ownerUserId)} type="button">↻ {english ? "Check for Updates" : "检查更新"}</button></div>
    </section>

    <section className="plugin-settings-search">
      <span aria-hidden="true">⌕</span>
      <input aria-label={english ? "Search plugins" : "搜索 Plugin"} onChange={(event) => setQuery(event.target.value)} placeholder={english ? "Search plugins, Skills, or publishers" : "搜索 Plugin、Skill 或发布者"} value={query} />
      <output>{plugins.length} {english ? "items" : "项"}</output>
    </section>

    {(!snapshot.secureMarketplaceInstallAvailable || !snapshot.manualInstallAvailable) && <section className="plugin-settings-platform-note"><span aria-hidden="true">i</span><div><strong>{english ? "Windows installation safety gate" : "Windows 安装安全门禁"}</strong><p>{english ? "Marketplace install, update, and manual import stay unavailable until the Windows native layer has the same signed download, archive confinement, per-file integrity, dependency isolation, and rollback chain as macOS. Installed plugins can still be inspected, enabled, disabled, and uninstalled here." : "Marketplace 安装、更新与手动导入需等 Windows 原生层具备与 macOS 相同的签名下载、压缩包路径隔离、逐文件完整性、依赖隔离和回滚链后才会开放。当前页面可以检查、启停及卸载已有 Plugin。"}</p></div></section>}

    {(!snapshot.marketplaceAvailable || !snapshot.localCatalogAvailable) && <section className="plugin-settings-partial"><strong>{english ? "Partial catalog" : "目录加载不完整"}</strong><p>{snapshot.marketplaceError ?? snapshot.localCatalogError}</p></section>}

    <section className="plugin-settings-list" aria-label={english ? "Plugin catalog" : "Plugin 目录"}>
      {plugins.length === 0 ? <div className="plugin-settings-empty"><span>◇</span><strong>{english ? "No matching plugins" : "没有匹配的 Plugin"}</strong><p>{query ? (english ? "Try another search term." : "请尝试其他搜索词。") : (english ? "No plugins are currently available." : "当前没有可用的 Plugin。")}</p></div> : plugins.map((plugin) => {
        const busy = operatingPluginId === plugin.id;
        const isExpanded = expanded === plugin.id;
        return <article className={`plugin-settings-card ${plugin.enabled ? "" : "is-disabled"}`} key={plugin.id}>
          <header>
            <span className="plugin-settings-icon" aria-hidden="true">{plugin.hasUi ? "▣" : "◇"}</span>
            <div className="plugin-settings-title"><div><h3>{plugin.displayName}</h3>{plugin.hasUi && <em>{english ? "App" : "应用"}</em>}{plugin.installed && <RuntimeBadge english={english} status={plugin.runtimeStatus} />}{plugin.installationIssue && <em className="is-warning">{english ? "Reinstall required" : "需要安全重装"}</em>}</div><p>{plugin.publisher} · {plugin.installedVersion ? (english ? `Installed ${plugin.installedVersion}` : `已安装 ${plugin.installedVersion}`) : plugin.latestVersion}{plugin.updateAvailable && (english ? ` · Latest ${plugin.latestVersion}` : ` · 最新 ${plugin.latestVersion}`)}</p></div>
            {busy && <i className="mini-spinner" />}
            {plugin.installed && !plugin.installationIssue && <label className="plugin-settings-switch"><input aria-label={`${plugin.displayName} enabled`} checked={plugin.enabled} disabled={Boolean(operatingPluginId)} onChange={(event) => void updateEnabled(port, plugin.id, event.target.checked)} type="checkbox" /><span /></label>}
          </header>
          <p className="plugin-settings-description">{plugin.description}</p>
          <div className="plugin-settings-meta"><span>{plugin.category}</span><span>{plugin.source === "marketplace" ? "Marketplace" : (english ? "Local" : "本机")}</span><span>{plugin.mcpServerCount} MCP</span><span>{plugin.skills.length} Skills</span>{!plugin.windowsCompatible && <span className="is-warning">{english ? "Not for Windows" : "不支持 Windows"}</span>}</div>
          {isExpanded && <PluginDetails english={english} plugin={plugin} />}
          <footer><button onClick={() => setExpanded(isExpanded ? null : plugin.id)} type="button">{isExpanded ? (english ? "Hide Details" : "收起详情") : (english ? "Skills & Permissions" : "Skills 与权限")}</button><div>{plugin.updateAvailable && <button disabled type="button">{english ? "Update unavailable" : "更新未准入"}</button>}{!plugin.installed && <button className="is-primary" disabled type="button">{plugin.windowsCompatible ? (english ? "Install unavailable" : "安装未准入") : (english ? "Unsupported" : "平台不支持")}</button>}{plugin.installed && <button className="is-danger" disabled={Boolean(operatingPluginId)} onClick={() => setUninstallTarget(plugin)} type="button">{english ? "Uninstall" : "卸载"}</button>}</div></footer>
        </article>;
      })}
    </section>

    {uninstallTarget && <div className="plugin-settings-confirm"><section aria-modal="true" role="alertdialog"><span>!</span><h3>{english ? "Uninstall plugin?" : "卸载 Plugin？"}</h3><p>{english ? `${uninstallTarget.displayName} will be disabled, its runtime stopped, and its native installation files removed.` : `${uninstallTarget.displayName} 将被停用，运行进程会终止，本机安装文件也会删除。`}</p><footer><button onClick={() => setUninstallTarget(null)} type="button">{english ? "Cancel" : "取消"}</button><button className="is-danger" disabled={Boolean(operatingPluginId)} onClick={async () => { if (await uninstall(port, uninstallTarget.id)) setUninstallTarget(null); }} type="button">{english ? "Uninstall" : "卸载"}</button></footer></section></div>}
  </div>;
}

function PluginDetails({ english, plugin }: { english: boolean; plugin: ManagedPlugin }) {
  return <div className="plugin-settings-details">
    <section><h4>{english ? "Bundled Skills" : "内置 Skills"}</h4>{plugin.skills.length ? <div>{plugin.skills.map((skill) => <code key={skill}>{skill}</code>)}</div> : <p>{english ? "No Skills declared." : "未声明 Skill。"}</p>}</section>
    <section><h4>{english ? "Permission declarations" : "权限声明"}</h4>{plugin.permissions.length ? <ul>{plugin.permissions.map((permission) => <li key={permission.id}><div><code>{permission.id}</code>{permission.required && <em>{english ? "Required" : "必需"}</em>}</div><p>{permission.reason ?? (english ? "No reason supplied." : "未提供说明。")}</p><small>{english ? "Declared · Windows runtime check pending" : "已声明 · Windows 运行检查待完成"}</small></li>)}</ul> : <p>{english ? "No native permissions declared." : "未声明原生权限。"}</p>}</section>
  </div>;
}

function RuntimeBadge({ english, status }: { english: boolean; status: ManagedPlugin["runtimeStatus"] }) {
  const label = status === "disabled" ? (english ? "Disabled" : "已停用") : status === "not_checked" ? (english ? "Health check pending" : "等待健康检查") : (english ? "Not installed" : "未安装");
  return <em className={status === "not_checked" ? "is-pending" : ""}>{label}</em>;
}

function PluginStatus({ title, detail, loading, action, onAction }: { title: string; detail: string; loading?: boolean; action?: string; onAction?(): void }) {
  return <div className="plugin-settings-status"><span aria-hidden="true">{loading ? <i className="mini-spinner" /> : "!"}</span><h2>{title}</h2><p>{detail}</p>{action && <button onClick={onAction} type="button">{action}</button>}</div>;
}
