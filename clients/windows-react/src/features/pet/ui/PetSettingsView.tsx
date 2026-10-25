import { useEffect } from "react";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import {
  isPetPreferencesDirty,
  usePetPreferencesStore,
} from "../application/petPreferencesStore";
import type { PetPreferencesPort } from "../ports/PetPreferencesPort";
import "./pet.css";

interface PetSettingsViewProps {
  ownerUserId: string;
  port: PetPreferencesPort;
}

export function PetSettingsView({ ownerUserId, port }: PetSettingsViewProps) {
  const state = usePetPreferencesStore();
  const { clearError, resetPosition, save, updateDraft } = state;
  const language = useGeneralSettingsStore((settings) => settings.settings.interfaceLanguage);
  const english = language === "en-US";
  const dirty = isPetPreferencesDirty(state);

  useEffect(() => {
    if (!dirty || state.isLoading || state.isSaving) return undefined;
    const timer = window.setTimeout(() => void save(port), 180);
    return () => window.clearTimeout(timer);
  }, [dirty, port, save, state.isLoading, state.isSaving, state.preferences]);

  return (
    <section aria-label={english ? "Pet settings" : "宠物设置"} className="pet-settings">
      <header className="pet-settings__hero">
        <h2>{english ? "Pet" : "宠物"}</h2>
        {(state.isLoading || state.isSaving) && <div className="pet-settings__progress"><span className="mini-spinner" />{state.isLoading ? (english ? "Loading…" : "正在读取…") : (english ? "Saving…" : "正在保存…")}</div>}
      </header>

      {state.error && <div className="pet-settings__error" role="alert"><span>!</span><p>{state.error}</p><button aria-label={english ? "Dismiss error" : "关闭错误"} onClick={clearError} type="button">×</button></div>}

      <SettingsCard icon="🐾" title={english ? "Global Presence" : "全局展示"} subtitle={english ? "Keep the pet visible outside the main window to show task status across apps and desktops." : "让宠物脱离主窗口，在其他桌面与应用上持续展示任务状态。"}>
        <PreferenceRow detail={english ? "Hides the pet window immediately while preserving notification history." : "关闭后宠物窗口会立即隐藏，但提醒记录仍会保留。"} title={english ? "Enable global pet" : "启用全局宠物"}>
          <Switch checked={state.preferences.isEnabled} disabled={state.isLoading} label={english ? "Enable global pet" : "启用全局宠物"} onChange={(isEnabled) => updateDraft({ isEnabled })} />
        </PreferenceRow>
        <PreferenceRow detail={english ? "Keeps the pet visible when switching Spaces or entering full-screen apps." : "切换 Space 或进入全屏应用时，宠物仍保持可见。"} title={english ? "Show across desktops and full-screen apps" : "跨桌面与全屏应用显示"}>
          <Switch checked={state.preferences.showAcrossSpaces} disabled={state.isLoading} label={english ? "Show across desktops" : "跨桌面显示"} onChange={(showAcrossSpaces) => updateDraft({ showAcrossSpaces })} />
        </PreferenceRow>
      </SettingsCard>

      <SettingsCard icon="🔔" title={english ? "Event Notifications" : "事件提醒"} subtitle={english ? "Choose which task events appear proactively beside the pet." : "决定哪些任务事件会主动出现在宠物旁边。"}>
        <PreferenceRow detail={english ? "Shows active tasks, important steps, and cancellation state." : "显示正在执行的任务、关键步骤和可取消状态。"} title={english ? "Show AI execution progress" : "展示 AI 执行过程"}>
          <Switch checked={state.preferences.showProcess} disabled={state.isLoading} label={english ? "Show AI execution progress" : "展示 AI 执行过程"} onChange={(showProcess) => updateDraft({ showProcess })} />
        </PreferenceRow>
        <PreferenceRow detail={english ? "Keeps a clear completion result and lets you open the details." : "任务完成后保留明确反馈，并允许继续查看详情。"} title={english ? "Show task completion feedback" : "展示任务完成反馈"}>
          <Switch checked={state.preferences.showCompletions} disabled={state.isLoading} label={english ? "Show completion feedback" : "展示任务完成反馈"} onChange={(showCompletions) => updateDraft({ showCompletions })} />
        </PreferenceRow>
      </SettingsCard>

      <SettingsCard icon="☷" title={english ? "Appearance & Position" : "外观与位置"} subtitle={english ? "Adjust the pet size or restore its default screen position." : "调整宠物尺寸，或把它恢复到默认屏幕位置。"}>
        <PreferenceRow detail={english ? "Changes only the pet itself; message cards remain readable." : "只改变宠物本体尺寸，消息卡片会保持易读。"} title={english ? "Pet size" : "宠物大小"}>
          <div className="pet-settings__size"><span>🐾</span><input aria-label={english ? "Pet size" : "宠物大小"} disabled={state.isLoading} max="180" min="72" onChange={(event) => updateDraft({ size: Number(event.target.value) })} step="4" type="range" value={state.preferences.size} /><strong>🐾</strong><output>{state.preferences.size}</output></div>
        </PreferenceRow>
        <div className="pet-settings__asset">
          <span aria-hidden="true">◆</span>
          <div><strong>{english ? "Current pet asset" : "当前宠物资源"}</strong><small>{english ? "Fengtuan · Autumn Fox" : "枫团 · 秋日小狐狸"}</small></div>
          <button disabled={state.isLoading || state.isSaving || dirty} onClick={() => void resetPosition(port)} type="button">{english ? "Reset Position" : "恢复默认位置"}</button>
        </div>
      </SettingsCard>
    </section>
  );
}

function SettingsCard({ icon, title, subtitle, children }: { icon: string; title: string; subtitle: string; children: React.ReactNode }) {
  return <section className="pet-settings-card"><header><span aria-hidden="true">{icon}</span><h3>{title}</h3></header><div className="pet-settings-card__body">{children}</div><p className="pet-settings-card__footnote">{subtitle}</p></section>;
}

function PreferenceRow({ title, detail, children }: { title: string; detail: string; children: React.ReactNode }) {
  return <div className="pet-settings-row"><div><strong>{title}</strong><p>{detail}</p></div><div>{children}</div></div>;
}

function Switch({ checked, disabled, label, onChange }: { checked: boolean; disabled: boolean; label: string; onChange(value: boolean): void }) {
  return <label className="pet-settings__switch"><input aria-label={label} checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} type="checkbox" /><span /></label>;
}
