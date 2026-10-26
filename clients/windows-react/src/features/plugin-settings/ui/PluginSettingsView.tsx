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
  const manualSelection = usePluginSettingsStore((state) => state.manualSelection);
  const manualInspection = usePluginSettingsStore((state) => state.manualInspection);
  const manualBusy = usePluginSettingsStore((state) => state.manualBusy);
  const load = usePluginSettingsStore((state) => state.load);
  const install = usePluginSettingsStore((state) => state.install);
  const updateEnabled = usePluginSettingsStore((state) => state.updateEnabled);
  const uninstall = usePluginSettingsStore((state) => state.uninstall);
  const updateCredentials = usePluginSettingsStore((state) => state.updateCredentials);
  const pickManualDirectory = usePluginSettingsStore((state) => state.pickManualDirectory);
  const inspectManualDirectory = usePluginSettingsStore((state) => state.inspectManualDirectory);
  const installManualDirectory = usePluginSettingsStore((state) => state.installManualDirectory);
  const discardManualDirectory = usePluginSettingsStore((state) => state.discardManualDirectory);
  const clearMessage = usePluginSettingsStore((state) => state.clearMessage);
  const reset = usePluginSettingsStore((state) => state.reset);
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const english = language === "en-US";
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<ManagedPlugin | null>(null);
  const [credentialTarget, setCredentialTarget] = useState<ManagedPlugin | null>(null);
  const [credentialValues, setCredentialValues] = useState<Record<string, string>>({});
  const [showsManualInstaller, setShowsManualInstaller] = useState(false);
  const [manualPresentationId] = useState(newManualPresentationId);

  useEffect(() => {
    void load(port, ownerUserId);
    return () => {
      void discardManualDirectory(port, manualPresentationId);
      reset();
    };
  }, [discardManualDirectory, load, manualPresentationId, ownerUserId, port, reset]);

  const closeManualInstaller = () => {
    setShowsManualInstaller(false);
    void discardManualDirectory(port, manualPresentationId);
  };

  const closeCredentialEditor = () => {
    setCredentialTarget(null);
    setCredentialValues({});
  };

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
        <div className="plugin-settings-actions"><button onClick={onOpenInstallationAssistant} type="button"><span aria-hidden="true">◇</span> {english ? "Installation Assistant" : "安装助手"}</button><button disabled={!snapshot.manualInstallAvailable} onClick={() => setShowsManualInstaller(true)} type="button"><span aria-hidden="true">＋</span> {english ? "Install Manually…" : "手动安装…"}</button><button disabled={isLoading} onClick={() => void load(port, ownerUserId)} type="button"><span aria-hidden="true">↻</span> {english ? "Check for Updates" : "检查更新"}</button></div>
      </div>
      <p className="plugin-settings-finder__footnote">{english ? "Filter by name, Skill, publisher, or category." : "按名称、Skill、发布者或分类筛选。"}</p>
    </section>

    {!snapshot.secureMarketplaceInstallAvailable && <section className="plugin-settings-platform-note"><span aria-hidden="true">i</span><div><strong>{english ? "Windows installation safety gate" : "Windows 安装安全门禁"}</strong><p>{english ? "Marketplace installation is unavailable because the native verification chain is not ready." : "Windows 原生安装校验链尚未就绪，Marketplace 安装暂不可用。"}</p></div></section>}
    {snapshot.secureMarketplaceInstallAvailable && <section className="plugin-settings-platform-note"><span aria-hidden="true">i</span><div><strong>{english ? "Verified plugin installation" : "受信 Plugin 安装"}</strong><p>{snapshot.manualInstallAvailable ? (english ? "Signed Marketplace packages and local folders are verified again immediately before installation. Git and npm sources remain available through the Installation Assistant." : "Marketplace 签名包与本机目录都会在安装前再次校验；Git 与 npm 来源继续由安装助手处理。") : (english ? "Signed Marketplace plugins can be installed and updated. Local folder import is currently unavailable." : "已支持安装和更新经过签名校验的 Marketplace Plugin；本机目录导入暂不可用。")}</p></div></section>}

    {(!snapshot.marketplaceAvailable || !snapshot.localCatalogAvailable) && <section className="plugin-settings-partial"><strong>{english ? "Partial catalog" : "目录加载不完整"}</strong><p>{snapshot.marketplaceError ?? snapshot.localCatalogError}</p></section>}

    <section className="plugin-settings-list" aria-label={english ? "Plugin catalog" : "Plugin 目录"}>
      {plugins.length === 0 ? <div className="plugin-settings-empty"><span>◇</span><strong>{english ? "No matching plugins" : "没有匹配的 Plugin"}</strong><p>{query ? (english ? "Try another search term." : "请尝试其他搜索词。") : (english ? "No plugins are currently available." : "当前没有可用的 Plugin。")}</p></div> : plugins.map((plugin) => {
        const busy = operatingPluginId === plugin.id;
        const isExpanded = expanded === plugin.id;
        const credentialsReady = plugin.credentials.every((credential) => credential.configured);
        return <article className={`plugin-settings-card ${plugin.enabled ? "" : "is-disabled"}`} key={plugin.id}>
          <header>
            <span className="plugin-settings-icon" aria-hidden="true">{plugin.hasUi ? "▣" : "◇"}</span>
            <div className="plugin-settings-title"><div><h3>{plugin.displayName}</h3>{plugin.hasUi && <em>{english ? "App" : "应用"}</em>}{plugin.installed && <RuntimeBadge english={english} status={plugin.runtimeStatus} />}{plugin.hookRuntimeStatus === "runner_unavailable" && <em className="is-warning">{english ? "Hook runner unavailable" : "Hook runner 未接入"}</em>}{plugin.credentials.length > 0 && !credentialsReady && <em className="is-warning">{english ? "Credentials required" : "需要配置凭据"}</em>}{plugin.installationIssue && <em className="is-warning">{english ? "Reinstall required" : "需要安全重装"}</em>}</div><p>{plugin.publisher} · {plugin.installedVersion ? (english ? `Installed ${plugin.installedVersion}` : `已安装 ${plugin.installedVersion}`) : plugin.latestVersion}{plugin.updateAvailable && (english ? ` · Latest ${plugin.latestVersion}` : ` · 最新 ${plugin.latestVersion}`)}</p></div>
            {busy && <i className="mini-spinner" />}
            {plugin.installed && !plugin.installationIssue && <label className="plugin-settings-switch"><input aria-label={`${plugin.displayName} enabled`} checked={plugin.enabled} disabled={Boolean(operatingPluginId)} onChange={(event) => void updateEnabled(port, plugin.id, event.target.checked)} type="checkbox" /><span /></label>}
          </header>
          <p className="plugin-settings-description">{plugin.description}</p>
          <div className="plugin-settings-meta"><span>{plugin.category}</span><span>{plugin.source === "marketplace" ? "Marketplace" : (english ? "Local" : "本机")}</span><span>{plugin.mcpServerCount} MCP</span><span>{plugin.skills.length} Skills</span>{plugin.hookCount > 0 && <span>{plugin.hookCount} Hook Bundle{plugin.hookCount === 1 ? "" : "s"}</span>}{!plugin.windowsCompatible && <span className="is-warning">{english ? "Not for Windows" : "不支持 Windows"}</span>}</div>
          {isExpanded && <PluginDetails english={english} plugin={plugin} />}
          <footer><button onClick={() => setExpanded(isExpanded ? null : plugin.id)} type="button">{isExpanded ? (english ? "Hide Details" : "收起详情") : (english ? "Skills & Permissions" : "Skills 与权限")}</button><div>{plugin.installed && plugin.credentials.length > 0 && <button disabled={Boolean(operatingPluginId)} onClick={() => { setCredentialValues({}); setCredentialTarget(plugin); }} type="button">{credentialsReady ? (english ? "Update Credentials…" : "更新凭据…") : (english ? "Configure Credentials…" : "配置凭据…")}</button>}{plugin.updateAvailable && <button disabled={Boolean(operatingPluginId) || !plugin.installActionAvailable} onClick={() => void install(port, plugin.id)} type="button">{plugin.installActionAvailable ? plugin.installationIssue ? (english ? "Reinstall" : "安全重装") : (english ? "Update" : "安全更新") : (english ? "Update unavailable" : "更新未准入")}</button>}{!plugin.installed && <button className="is-primary" disabled={Boolean(operatingPluginId) || !plugin.installActionAvailable} onClick={() => void install(port, plugin.id)} type="button">{!plugin.windowsCompatible ? (english ? "Unsupported" : "平台不支持") : plugin.installActionAvailable ? (english ? "Install" : "安全安装") : (english ? "Install unavailable" : "安装未准入")}</button>}{plugin.installed && <button className="is-danger" disabled={Boolean(operatingPluginId)} onClick={() => setUninstallTarget(plugin)} type="button">{english ? "Uninstall" : "卸载"}</button>}</div></footer>
        </article>;
      })}
    </section>

    {uninstallTarget && <div className="plugin-settings-confirm"><section aria-modal="true" role="alertdialog"><span>!</span><h3>{english ? "Uninstall plugin?" : "卸载 Plugin？"}</h3><p>{english ? `${uninstallTarget.displayName} will be disabled, its runtime stopped, and its native installation files removed.` : `${uninstallTarget.displayName} 将被停用，运行进程会终止，本机安装文件也会删除。`}</p><footer><button onClick={() => setUninstallTarget(null)} type="button">{english ? "Cancel" : "取消"}</button><button className="is-danger" disabled={Boolean(operatingPluginId)} onClick={async () => { if (await uninstall(port, uninstallTarget.id)) setUninstallTarget(null); }} type="button">{english ? "Uninstall" : "卸载"}</button></footer></section></div>}
    {credentialTarget && <div className="plugin-settings-credentials" onMouseDown={(event) => { if (event.currentTarget === event.target) closeCredentialEditor(); }}>
      <section aria-labelledby="plugin-credentials-title" aria-modal="true" role="dialog">
        <header><div><span aria-hidden="true">⌘</span><div><h3 id="plugin-credentials-title">{english ? "Plugin Credentials" : "配置 Plugin 凭据"}</h3><p>{credentialTarget.displayName}</p></div></div><button aria-label={english ? "Close credentials" : "关闭凭据配置"} onClick={closeCredentialEditor} type="button">×</button></header>
        <div className="plugin-settings-credentials__content">
          <p>{english ? "Secrets are written directly to Windows Credential Manager. Existing values are never returned to this page." : "凭据会直接写入 Windows 凭据管理器；已有值不会回传到这个页面。"}</p>
          {credentialTarget.credentials.map((credential) => <label key={credential.name}><span><strong>{credential.name}</strong><small>{credential.configured ? (english ? "Configured · leave blank to keep" : "已配置 · 留空则保持原值") : (english ? "Required" : "必填")}</small></span><input aria-label={credential.name} autoComplete="off" onChange={(event) => setCredentialValues((values) => ({ ...values, [credential.name]: event.target.value }))} type="password" value={credentialValues[credential.name] ?? ""} /></label>)}
        </div>
        <footer><button onClick={closeCredentialEditor} type="button">{english ? "Cancel" : "取消"}</button><button className="is-primary" disabled={Boolean(operatingPluginId) || credentialTarget.credentials.some((credential) => !credential.configured && !(credentialValues[credential.name]?.length))} onClick={async () => { const values = Object.fromEntries(credentialTarget.credentials.map((credential) => [credential.name, credentialValues[credential.name] || null])); if (await updateCredentials(port, credentialTarget.id, values)) closeCredentialEditor(); }} type="button">{operatingPluginId === credentialTarget.id ? (english ? "Saving…" : "保存中…") : (english ? "Save Securely" : "安全保存")}</button></footer>
      </section>
    </div>}
    {showsManualInstaller && <div className="plugin-settings-manual" onMouseDown={(event) => { if (event.currentTarget === event.target) closeManualInstaller(); }}>
      <section aria-labelledby="manual-plugin-title" aria-modal="true" role="dialog">
        <header><div><span aria-hidden="true">◇</span><div><h3 id="manual-plugin-title">{english ? "Install a Local Plugin" : "安装本机 Plugin"}</h3><p>{english ? "Choose a Plugin, MCP, Agent plugin, or standalone Skill folder." : "选择 Plugin、MCP、Agent Plugin 或独立 Skill 的根目录。"}</p></div></div><button aria-label={english ? "Close manual installer" : "关闭手动安装"} onClick={closeManualInstaller} type="button">×</button></header>
        <div className="plugin-settings-manual__content">
          <section className="plugin-settings-manual__source"><div><strong>{english ? "Source folder" : "来源目录"}</strong><p>{manualSelection?.directoryName ?? (english ? "No folder selected" : "尚未选择目录")}</p></div><button disabled={manualBusy != null} onClick={() => void pickManualDirectory(port, manualPresentationId)} type="button">{manualBusy === "pick" ? (english ? "Choosing…" : "选择中…") : (english ? "Choose Root Folder" : "选择根目录")}</button></section>
          <p className="plugin-settings-manual__privacy">{english ? "Only the folder name is shown here. The native client keeps its full path private." : "这里只显示目录名称；完整路径仅由原生客户端保管，不会暴露给 React 页面。"}</p>
          {error && <div className="plugin-settings-manual__error" role="alert"><span>!</span><p>{error}</p></div>}
          {manualInspection ? <section className="plugin-settings-manual__inspection">
            <header><span>✓</span><div><strong>{english ? "Preflight Passed" : "预检通过"}</strong><p>{manualInspection.displayName} · {manualInspection.version}</p></div></header>
            {manualInspection.description && <p>{manualInspection.description}</p>}
            <div className="plugin-settings-manual__components">{manualInspection.components.length ? manualInspection.components.map((component, index) => <span className={component.kind === "hook" ? "is-hook" : undefined} key={`${component.kind}-${component.displayName}-${index}`}><b>{component.kind === "skill" ? "Skill" : component.kind === "hook" ? "Hook" : "MCP"}</b>{component.displayName}{component.kind === "hook" && <small>{english ? `${component.hookCount} handlers · ${component.events.join(", ")} · Windows runner unavailable` : `${component.hookCount} 个处理器 · ${component.events.join("、")} · Windows runner 尚未接入`}</small>}</span>) : <small>{english ? "No MCP, Skill, or Hook components declared." : "未声明 MCP、Skill 或 Hook 组件。"}</small>}</div>
            <dl><dt>SHA-256</dt><dd>{manualInspection.fingerprint}</dd></dl>
          </section> : <section className="plugin-settings-manual__empty"><span aria-hidden="true">⌕</span><div><strong>{english ? "Preflight required" : "需要先预检"}</strong><p>{english ? "The package type, components, manifest, links, and content fingerprint will be checked before installation." : "安装前会检查包类型、组件、清单、链接与内容指纹。"}</p></div></section>}
        </div>
        <footer><button onClick={closeManualInstaller} type="button">{english ? "Cancel" : "取消"}</button><button disabled={!manualSelection || manualBusy != null} onClick={() => void inspectManualDirectory(port, manualPresentationId)} type="button">{manualBusy === "inspect" ? (english ? "Inspecting…" : "预检中…") : (english ? "Run Preflight" : "预检")}</button><button className="is-primary" disabled={!manualInspection || manualBusy != null} onClick={async () => { if (await installManualDirectory(port, manualPresentationId)) setShowsManualInstaller(false); }} type="button">{manualBusy === "install" ? (english ? "Installing…" : "安装中…") : (english ? "Install" : "确认安装")}</button></footer>
      </section>
    </div>}
  </div>;
}

let manualPresentationSequence = 0;

function newManualPresentationId() {
  manualPresentationSequence += 1;
  const random = globalThis.crypto?.randomUUID?.();
  return random
    ? `plugin-manual-${random}`
    : `plugin-manual-${Date.now().toString(36)}-${manualPresentationSequence.toString(36)}`;
}

function PluginDetails({ english, plugin }: { english: boolean; plugin: ManagedPlugin }) {
  return <div className="plugin-settings-details">
    <section><h4>{english ? "Bundled Skills" : "内置 Skills"}</h4>{plugin.skills.length ? <div>{plugin.skills.map((skill) => <code key={skill}>{skill}</code>)}</div> : <p>{english ? "No Skills declared." : "未声明 Skill。"}</p>}</section>
    {plugin.hookCount > 0 && <section><h4>{english ? "Lifecycle Hooks" : "生命周期 Hooks"}</h4><p>{plugin.installed ? (english ? `${plugin.hookCount} Hook bundle(s) were checked during installation. Windows AppContainer execution is not connected, so these Hooks are not running.` : `${plugin.hookCount} 个 Hook Bundle 已在安装时完成校验；Windows AppContainer 执行器尚未接入，因此这些 Hook 当前不会运行。`) : (english ? `${plugin.hookCount} Hook bundle(s) are declared. Full content preflight runs before installation; the Windows runner is unavailable.` : `清单声明了 ${plugin.hookCount} 个 Hook Bundle；完整内容会在安装前预检，Windows runner 当前不可用。`)}</p></section>}
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
