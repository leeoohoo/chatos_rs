import { useEffect } from "react";
import { isGeneralSettingsDirty, useGeneralSettingsStore } from "../application/generalSettingsStore";
import type { GeneralSettingsPort } from "../ports/GeneralSettingsPort";
import "./generalSettings.css";

interface GeneralSettingsViewProps {
  ownerUserId: string;
  username: string;
  displayName?: string | null;
  role: string;
  port: GeneralSettingsPort;
  onLogout(): Promise<void>;
}

export function GeneralSettingsView({ ownerUserId, username, displayName, role, port, onLogout }: GeneralSettingsViewProps) {
  const state = useGeneralSettingsStore();
  const { activate, clearError, save, updateDraft } = state;
  const dirty = isGeneralSettingsDirty(state);
  const english = state.settings.interfaceLanguage === "en-US";

  useEffect(() => {
    void activate(port, ownerUserId);
  }, [activate, ownerUserId, port]);

  useEffect(() => {
    if (!dirty || state.isLoading || state.isSaving) return undefined;
    const timer = window.setTimeout(() => void save(port), 250);
    return () => window.clearTimeout(timer);
  }, [dirty, port, save, state.isLoading, state.isSaving, state.settings]);

  return (
    <section aria-label={english ? "General and account settings" : "常规与账号设置"} className="general-settings">
      <header className="general-settings__hero">
        <span aria-hidden="true">⚙</span>
        <div><p>SETTINGS</p><h2>{english ? "General & Account" : "常规与账号"}</h2><small>{english ? "Manage language, display, runtime behavior, and the current account." : "管理语言、显示、运行行为与当前账号。"}</small></div>
        {(state.isLoading || state.isSaving) && <div className="general-settings__progress"><span className="mini-spinner" />{state.isLoading ? (english ? "Loading…" : "正在读取…") : (english ? "Saving…" : "正在保存…")}</div>}
      </header>
      {state.error && <div className="general-settings__error" role="alert"><span>!</span><p>{state.error}</p><button onClick={clearError} type="button">×</button></div>}
      <SettingsCard icon="◎" title={english ? "Language" : "语言"} subtitle={english ? "Control the client interface and AI internal context independently." : "分别控制客户端界面和 AI 内部上下文。"}>
        <PreferenceRow detail={english ? "The client shell switches immediately." : "客户端外壳会立即切换。"} title={english ? "Interface language" : "界面语言"}>
          <LanguagePicker disabled={state.isLoading} value={state.settings.interfaceLanguage} onChange={(interfaceLanguage) => updateDraft({ interfaceLanguage })} />
        </PreferenceRow>
        <PreferenceRow detail={english ? "Used for subsequent AI prompts, built-in tools, and task progress." : "用于后续 AI system prompt、内置工具和任务过程。"} title={english ? "Internal context language" : "内部上下文语言"}>
          <LanguagePicker disabled={state.isLoading} value={state.settings.contextLanguage} onChange={(contextLanguage) => updateDraft({ contextLanguage })} />
        </PreferenceRow>
      </SettingsCard>
      <SettingsCard icon="A" title={english ? "Display" : "显示"} subtitle={english ? "Adjust the overall reading density of ChatOS." : "调整 ChatOS 界面的整体阅读密度。"}>
        <PreferenceRow detail={english ? "Titles, body text, and supporting text keep the same visual hierarchy." : "标题、正文和辅助信息保持相同的层级比例。"} title={english ? "Font size" : "字体大小"}>
          <div className="general-settings__font"><span>A</span><input aria-label={english ? "Font size" : "字体大小"} disabled={state.isLoading} max="18" min="12" onChange={(event) => updateDraft({ interfaceFontSize: Number(event.target.value) })} step="1" type="range" value={state.settings.interfaceFontSize} /><strong>A</strong><output>{state.settings.interfaceFontSize} pt</output><button disabled={state.settings.interfaceFontSize === 14} onClick={() => updateDraft({ interfaceFontSize: 14 })} type="button">{english ? "Reset" : "恢复默认"}</button></div>
        </PreferenceRow>
      </SettingsCard>
      <SettingsCard icon="ϟ" title={english ? "Runtime & Power" : "运行与电源"} subtitle={english ? "Control system behavior while long-running tasks are active." : "控制长时间任务运行时的系统行为。"}>
        <PreferenceRow detail={english ? "Prevents idle system sleep. The display may still turn off, and manual sleep still works." : "阻止空闲系统睡眠；屏幕仍可熄灭，也不会阻止用户主动睡眠。"} title={english ? "Keep Windows awake while ChatOS is running" : "程序运行时保持 Windows 唤醒"}>
          <label className="general-settings__switch"><input checked={state.settings.preventsIdleSystemSleep} disabled={state.isLoading} onChange={(event) => updateDraft({ preventsIdleSystemSleep: event.target.checked })} type="checkbox" /><span /></label>
        </PreferenceRow>
      </SettingsCard>
      <SettingsCard icon="●" title={english ? "Account" : "账号"} subtitle={english ? "The ChatOS platform identity currently signed in." : "当前登录的 ChatOS 平台身份。"}>
        <div className="general-settings__account"><span>{(displayName ?? username).slice(0, 1).toUpperCase()}</span><div><strong>{displayName ?? username}</strong>{displayName && displayName !== username && <small>{username}</small>}</div><em>{role}</em><button onClick={() => void onLogout()} type="button">{english ? "Sign out" : "退出登录"}</button></div>
      </SettingsCard>
    </section>
  );
}

function SettingsCard({ icon, title, subtitle, children }: { icon: string; title: string; subtitle: string; children: React.ReactNode }) {
  return <section className="general-settings-card"><header><span aria-hidden="true">{icon}</span><div><h3>{title}</h3><p>{subtitle}</p></div></header><div className="general-settings-card__body">{children}</div></section>;
}

function PreferenceRow({ title, detail, children }: { title: string; detail: string; children: React.ReactNode }) {
  return <div className="general-settings-row"><div><strong>{title}</strong><p>{detail}</p></div><div>{children}</div></div>;
}

function LanguagePicker({ value, disabled, onChange }: { value: "zh-CN" | "en-US"; disabled: boolean; onChange(value: "zh-CN" | "en-US"): void }) {
  return <select aria-label="Language" disabled={disabled} onChange={(event) => onChange(event.target.value as "zh-CN" | "en-US")} value={value}><option value="zh-CN">中文</option><option value="en-US">English</option></select>;
}
