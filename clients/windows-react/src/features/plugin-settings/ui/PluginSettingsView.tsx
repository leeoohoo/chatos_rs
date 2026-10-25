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
  const install = usePluginSettingsStore((state) => state.install);
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
    <header className="plugin-settings-page__title">
      <h2>{english ? "Plugins & Skills" : "插件与 Skills"}</h2>
      {isLoading && <div><span className="mini-spinner" />{english ? "Checking…" : "检查中…"}</div>}
    </header>
    {(error || notice) && <div className={`plugin-settings-banner ${error ? "is-error" : ""}`} role={error ? "alert" : "status"}><span>{error ? "!" : "✓"}</span><p>{error ?? notice}</p><button onClick={clearMessage} type="button">×</button></div>}

    <section className="plugin-settings-finder">
      <header><span aria-hidden="true">⌕</span><h3>{english ? "Find Plugins" : "查找 Plugin"}</h3></header>
      <div className="plugin-settings-finder__body">
        <div className="plugin-settings-search"><span aria-hidden="true">⌕</span><input aria-label={english ? "Search plugins" : "搜索 Plugin"} onChange={(event) => setQuery(event.target.value)} placeholder={english ? "Search plugins, Skills, or publishers" : "搜索 Plugin、Skill 或发布者"} value={query} /><output>{plugins.length} {english ? "items" : "项"}</output></div>
        <div className="plugin-settings-actions"><button onClick={onOpenInstallationAssistant} type="button"><span aria-hidden="true">◇</span> {english ? "Installation Assistant" : "安装助手"}</button><button disabled={isLoading} onClick={() => void load(port, ownerUserId)} type="button"><span aria-hidden="true">↻</span> {english ? "Check for Updates" : "检查更新"}</button></div>
      </div>
      <p className="plugin-settings-finder__footnote">{english ? "Filter by name, Skill, publisher, or category." : "按名称、Skill、发布者或分类筛选。"}</p>
    </section>

    {!snapshot.secureMarketplaceInstallAvailable && <section className="plugin-settings-platform-note"><span aria-hidden="true">i</span><div><strong>{english ? "Windows installation safety gate" : "Windows 安装安全门禁"}</strong><p>{english ? "Marketplace installation is unavailable because the native verification chain is not ready." : "Windows 原生安装校验链尚未就绪，Marketplace 安装暂不可用。"}</p></div></section>}
    {snapshot.secureMarketplaceInstallAvailable && !snapshot.manualInstallAvailable && <section className="plugin-settings-platform-note"><span aria-hidden="true">i</span><div><strong>{english ? "Verified Marketplace installation" : "受信 Marketplace 安装"}</strong><p>{english ? "Signed Marketplace plugins can be installed and updated. The Installation Assistant also accepts an explicitly named local ChatOS Plugin folder or credential-free HTTPS Git repository after fingerprint verification; direct settings-page import and other formats remain unavailable." : "已支持安装和更新经过签名校验的 Marketplace Plugin；安装助手也可在用户明确给出本机 ChatOS Plugin 目录或无凭据 HTTPS Git 仓库后执行指纹复验安装。设置页直接导入及其他格式仍未开放。"}</p></div></section>}

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
          <footer><button onClick={() => setExpanded(isExpanded ? null : plugin.id)} type="button">{isExpanded ? (english ? "Hide Details" : "收起详情") : (english ? "Skills & Permissions" : "Skills 与权限")}</button><div>{plugin.updateAvailable && <button disabled={Boolean(operatingPluginId) || !plugin.installActionAvailable} onClick={() => void install(port, plugin.id)} type="button">{plugin.installActionAvailable ? plugin.installationIssue ? (english ? "Reinstall" : "安全重装") : (english ? "Update" : "安全更新") : (english ? "Update unavailable" : "更新未准入")}</button>}{!plugin.installed && <button className="is-primary" disabled={Boolean(operatingPluginId) || !plugin.installActionAvailable} onClick={() => void install(port, plugin.id)} type="button">{!plugin.windowsCompatible ? (english ? "Unsupported" : "平台不支持") : plugin.installActionAvailable ? (english ? "Install" : "安全安装") : (english ? "Install unavailable" : "安装未准入")}</button>}{plugin.installed && <button className="is-danger" disabled={Boolean(operatingPluginId)} onClick={() => setUninstallTarget(plugin)} type="button">{english ? "Uninstall" : "卸载"}</button>}</div></footer>
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
