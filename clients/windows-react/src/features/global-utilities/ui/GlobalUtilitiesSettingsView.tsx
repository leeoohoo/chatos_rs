import { useEffect, useState } from "react";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import {
  isGlobalUtilityPreferencesDirty,
  useGlobalUtilityPreferencesStore,
} from "../application/globalUtilityPreferencesStore";
import {
  defaultGlobalHotKeys,
  formatShortcut,
  globalUtilityActions,
  type GlobalShortcut,
  type GlobalUtilityAction,
  type ShortcutModifier,
} from "../domain/globalUtilities";
import type { GlobalUtilityPreferencesPort } from "../ports/GlobalUtilityPreferencesPort";
import "./globalUtilities.css";

interface GlobalUtilitiesSettingsViewProps {
  ownerUserId: string;
  port: GlobalUtilityPreferencesPort;
}

const copy = {
  screenshot: { zh: ["截屏", "选择区域、标注或创建长截图。", "▣"], en: ["Screenshot", "Select a region, annotate, or create a scrolling capture.", "▣"] },
  screenRecording: { zh: ["录屏", "打开 Windows 系统录屏入口。", "◉"], en: ["Screen Recording", "Open the Windows system recording surface.", "◉"] },
  clipboardHistory: { zh: ["剪贴板历史", "查找并恢复之前复制的本机内容。", "▤"], en: ["Clipboard History", "Find and restore content previously copied on this device.", "▤"] },
  quickSearch: { zh: ["快速搜索", "从任意应用快速进入 ChatOS 本地搜索。", "⌕"], en: ["Quick Search", "Enter ChatOS local search from any application.", "⌕"] },
} satisfies Record<GlobalUtilityAction, { zh: [string, string, string]; en: [string, string, string] }>;

export function GlobalUtilitiesSettingsView({ ownerUserId, port }: GlobalUtilitiesSettingsViewProps) {
  const state = useGlobalUtilityPreferencesStore();
  const { clearError, save, updateDraft } = state;
  const english = useGeneralSettingsStore((settings) => settings.settings.interfaceLanguage) === "en-US";
  const [showWarning, setShowWarning] = useState(false);
  const [recording, setRecording] = useState<GlobalUtilityAction | null>(null);
  const dirty = isGlobalUtilityPreferencesDirty(state);

  useEffect(() => {
    if (!dirty || state.isLoading || state.isSaving) return undefined;
    const timer = window.setTimeout(() => void save(port), 180);
    return () => window.clearTimeout(timer);
  }, [dirty, port, save, state.isLoading, state.isSaving, state.preferences]);

  useEffect(() => {
    if (!recording) return undefined;
    const listener = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecording(null);
        return;
      }
      const shortcut = shortcutFromEvent(event);
      if (!shortcut) return;
      updateDraft({ hotKeys: { ...state.preferences.hotKeys, [recording]: shortcut } });
      setRecording(null);
    };
    window.addEventListener("keydown", listener, true);
    return () => window.removeEventListener("keydown", listener, true);
  }, [recording, state.preferences.hotKeys, updateDraft]);

  const setMasterEnabled = (enabled: boolean) => {
    if (enabled && !state.preferences.hasAcknowledgedShortcutConflicts) {
      setShowWarning(true);
      return;
    }
    updateDraft({ isEnabled: enabled });
  };

  return (
    <section aria-label={english ? "Global utility settings" : "全局工具设置"} className="global-utilities">
      <header className="global-utilities__hero">
        <span aria-hidden="true">⌘</span>
        <div><p>GLOBAL UTILITIES</p><h2>{english ? "Global Utilities" : "全局工具"}</h2><small>{english ? "Local productivity tools that remain available after the main window closes." : "主窗口关闭后仍可使用的本机效率入口。"}</small></div>
        {(state.isLoading || state.isSaving) && <div className="global-utilities__progress"><span className="mini-spinner" />{state.isLoading ? (english ? "Loading…" : "正在读取…") : (english ? "Saving…" : "正在保存…")}</div>}
      </header>

      {state.error && <div className="global-utilities__error" role="alert"><span>!</span><p>{state.error}</p><button aria-label={english ? "Dismiss error" : "关闭错误"} onClick={clearError} type="button">×</button></div>}

      <SettingsCard icon="⌘" title={english ? "Global Utilities" : "全局工具"} subtitle={english ? "Control all registered shortcuts with one switch." : "通过一个总开关管理所有已注册的系统快捷键。"}>
        <PreferenceRow detail={english ? "Turning this off unregisters every shortcut so ChatOS no longer intercepts keys in the background." : "关闭后会注销全部全局快捷键，不会在后台拦截按键。"} title={english ? "Enable global utilities" : "启用全局快捷工具"}>
          <Switch checked={state.preferences.isEnabled} disabled={state.isLoading} label={english ? "Enable global utilities" : "启用全局快捷工具"} onChange={setMasterEnabled} />
        </PreferenceRow>
      </SettingsCard>

      <SettingsCard icon="⌨" title={english ? "Global Shortcuts" : "全局快捷键"} subtitle={english ? "Select a shortcut, then press a new modified key combination." : "点击快捷键按钮后，直接按下新的组合键。"}>
        {globalUtilityActions.map((action) => {
          const item = copy[action][english ? "en" : "zh"];
          const registration = state.preferences.registrations[action];
          const enabledKey = `${action}Enabled` as const;
          return <PreferenceRow key={action} detail={item[1]} icon={item[2]} title={item[0]}>
            <div className="global-utilities__shortcut-controls">
              <RegistrationStatus english={english} registration={registration} />
              <button
                aria-label={english ? `Record ${item[0]} shortcut` : `录制${item[0]}快捷键`}
                className={`global-utilities__recorder ${recording === action ? "is-recording" : ""}`}
                disabled={!state.preferences.isEnabled || !state.preferences[enabledKey] || state.isLoading}
                onClick={() => setRecording(action)}
                type="button"
              >{recording === action ? (english ? "Press keys…" : "请按组合键…") : formatShortcut(state.preferences.hotKeys[action])}</button>
              <Switch checked={state.preferences[enabledKey]} disabled={state.isLoading} label={item[0]} onChange={(enabled) => updateDraft({ [enabledKey]: enabled })} />
            </div>
          </PreferenceRow>;
        })}
        <div className="global-utilities__footer"><button onClick={() => updateDraft({ hotKeys: defaultGlobalHotKeys, screenshotEnabled: true, screenRecordingEnabled: true, clipboardHistoryEnabled: true, quickSearchEnabled: true })} type="button">{english ? "Restore Defaults" : "恢复默认快捷键"}</button><span>{english ? "Esc cancels recording." : "按 Esc 可取消录制。"}</span></div>
      </SettingsCard>

      <SettingsCard icon="●" title={english ? "Privacy" : "隐私"} subtitle={english ? "Capture, clipboard, and local search data stays on this PC by default." : "截图、录屏、剪贴板和本地搜索默认只在这台电脑上处理。"}>
        <div className="global-utilities__privacy"><span aria-hidden="true">▣</span><p>{english ? "Screenshots, recordings, and clipboard content are never uploaded without an explicit action." : "未经你明确点击，截图、录屏和剪贴板内容不会上传到 ChatOS 服务端。"}</p></div>
      </SettingsCard>

      {showWarning && <div aria-label={english ? "Shortcut conflict warning" : "快捷键冲突提示"} aria-modal="true" className="global-utilities__modal" role="dialog"><div><span aria-hidden="true">⌨</span><h3>{english ? "Enable global shortcuts?" : "启用全局快捷键？"}</h3><p>{english ? "Ctrl+A, Ctrl+Q, Ctrl+E, and Ctrl+Space may override common shortcuts in other applications. You can rebind or disable them at any time." : "Control+A、Control+Q、Control+E 和 Control+Space 会覆盖部分应用的常用按键。你可以随时改键或关闭。"}</p><footer><button onClick={() => setShowWarning(false)} type="button">{english ? "Cancel" : "取消"}</button><button className="is-primary" onClick={() => { updateDraft({ hasAcknowledgedShortcutConflicts: true, isEnabled: true }); setShowWarning(false); }} type="button">{english ? "Enable" : "启用"}</button></footer></div></div>}
    </section>
  );
}

function SettingsCard({ children, icon, subtitle, title }: { children: React.ReactNode; icon: string; subtitle: string; title: string }) {
  return <section className="global-utilities-card"><header><span aria-hidden="true">{icon}</span><div><h3>{title}</h3><p>{subtitle}</p></div></header><div className="global-utilities-card__body">{children}</div></section>;
}

function PreferenceRow({ children, detail, icon, title }: { children: React.ReactNode; detail: string; icon?: string; title: string }) {
  return <div className="global-utilities-row">{icon && <span className="global-utilities-row__icon" aria-hidden="true">{icon}</span>}<div><strong>{title}</strong><p>{detail}</p></div><div>{children}</div></div>;
}

function Switch({ checked, disabled, label, onChange }: { checked: boolean; disabled: boolean; label: string; onChange(value: boolean): void }) {
  return <label className="global-utilities__switch"><input aria-label={label} checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} type="checkbox" /><span /></label>;
}

function RegistrationStatus({ english, registration }: { english: boolean; registration: ReturnType<typeof useGlobalUtilityPreferencesStore.getState>["preferences"]["registrations"][GlobalUtilityAction] }) {
  const label = registration.state === "registered"
    ? registration.usesFallback ? (english ? `Fallback ${formatShortcut(registration.active!)}` : `已回退为 ${formatShortcut(registration.active!)}`) : (english ? "Registered" : "已注册")
    : registration.state === "conflict" ? (english ? "In use" : "已被占用")
    : registration.state === "unsupported" ? (english ? "Unsupported" : "不受支持")
    : (english ? "Not registered" : "未注册");
  return <span className={`global-utilities__status is-${registration.state}`}>{label}</span>;
}

function shortcutFromEvent(event: KeyboardEvent): GlobalShortcut | null {
  const modifiers: ShortcutModifier[] = [];
  if (event.ctrlKey) modifiers.push("control");
  if (event.altKey) modifiers.push("alt");
  if (event.shiftKey) modifiers.push("shift");
  if (event.metaKey) modifiers.push("super");
  if (modifiers.length === 0) return null;
  const key = normalizeKey(event.key);
  return key ? { key, modifiers } : null;
}

function normalizeKey(key: string) {
  if (/^[a-z0-9]$/i.test(key)) return key.toUpperCase();
  if (key === " ") return "Space";
  if (/^F(?:[1-9]|1\d|2[0-4])$/.test(key)) return key;
  if (["Enter", "Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown", "Delete", "Backspace", "Insert"].includes(key)) return key;
  return null;
}
