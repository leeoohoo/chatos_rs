import { useEffect, useState } from "react";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { useWeChatCompanionStore } from "../application/weChatCompanionStore";
import type { WeChatClientSession } from "../domain/wechatCompanion";
import type { WeChatCompanionPort } from "../ports/WeChatCompanionPort";
import "./wechatCompanion.css";

export function WeChatCompanionView({ ownerUserId, port }: { ownerUserId: string; port: WeChatCompanionPort }) {
  const state = useWeChatCompanionStore();
  const language = useGeneralSettingsStore((value) => value.settings.interfaceLanguage);
  const english = language === "en-US";
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1_000));
  const [confirmsUnbind, setConfirmsUnbind] = useState(false);

  useEffect(() => {
    void state.activate(port, ownerUserId);
    return () => {
      state.reset();
      void port.discardTicket(ownerUserId);
    };
  }, [ownerUserId, port, state.activate, state.reset]);

  useEffect(() => {
    if (state.ticket?.status !== "issued" || state.isPolling) return undefined;
    const timer = window.setTimeout(() => void state.pollTicket(port), state.pollingDelayMs);
    return () => window.clearTimeout(timer);
  }, [port, state.pollTicket, state.pollingDelayMs, state.ticket?.previewUrl, state.ticket?.status, state.isPolling]);

  useEffect(() => {
    if (!state.ticket) return undefined;
    const timer = window.setInterval(() => setNow(Math.floor(Date.now() / 1_000)), 1_000);
    return () => window.clearInterval(timer);
  }, [state.ticket]);

  const binding = state.snapshot?.binding;
  const remaining = Math.max(0, (state.ticket?.expiresAtUnix ?? 0) - now);

  useEffect(() => {
    if (state.ticket?.status === "issued" && remaining === 0) {
      state.expireTicket(state.ticket.previewUrl);
    }
  }, [remaining, state.expireTicket, state.ticket?.previewUrl, state.ticket?.status]);

  return (
    <section className="wechat-companion" aria-label={english ? "WeChat Companion settings" : "微信小程序设置"}>
      <header className="wechat-companion__hero">
        <h2>{english ? "WeChat Mini Program" : "微信小程序"}</h2>
        {state.isLoading && <div className="wechat-companion__progress"><span className="mini-spinner" />{english ? "Loading…" : "正在读取…"}</div>}
      </header>

      <SettingsCard icon="⌗" title={english ? "WeChat Account Binding" : "微信账号绑定"} subtitle={english ? "Scanning still requires confirmation on this signed-in Windows device." : "扫码后仍需在这台已登录的 Windows 设备上确认。"}>
        {state.isLoading && !binding ? <Loading english={english} /> : binding?.bound ? (
          <div className="wechat-binding-state">
            <span className="wechat-binding-state__icon">✓</span>
            <div><strong>{english ? "WeChat Mini Program is bound" : "微信小程序已绑定"}</strong><p>{english ? "WeChat can view devices, continue conversations, and answer requests." : "可从微信查看设备、继续会话和处理交互请求。"}</p></div>
            <button className="wechat-danger-button" disabled={state.isMutating} onClick={() => setConfirmsUnbind(true)} type="button">{english ? "Unbind" : "解除绑定"}</button>
          </div>
        ) : state.ticket ? (
          <div className="wechat-ticket">
            <div className="wechat-ticket__image"><img alt={english ? "WeChat Mini Program code" : "微信小程序码"} src={state.ticket.previewUrl} /></div>
            <div className="wechat-ticket__copy">
              <strong>{state.ticket.status === "claimed" ? (english ? "A WeChat device requested binding" : "微信设备请求绑定") : (english ? "Scan with WeChat" : "用微信扫描小程序码")}</strong>
              <p>{state.ticket.status === "claimed" ? (english ? "Confirm that you scanned the code. The WeChat account will receive Companion access." : "请确认刚才是你本人扫码。确认后，该微信账号将能访问当前账号的 Companion 功能。") : (english ? "A second confirmation appears here. Never approve a forwarded screenshot or unknown device." : "扫码后此处会出现二次确认。不要确认来自截图转发或陌生设备的请求。")}</p>
              <small className={remaining === 0 || state.ticket.status === "expired" ? "is-expired" : ""}>{remaining > 0 && state.ticket.status !== "expired" ? (english ? `Code expires in ${remaining}s` : `小程序码将在 ${remaining} 秒后过期`) : (english ? "The code has expired. Generate a new one." : "小程序码已过期，请重新生成。")}</small>
              <div>
                {state.ticket.status === "claimed" && <button className="primary-button" disabled={state.isMutating} onClick={() => void state.confirmTicket(port)} type="button">{english ? "Confirm Binding" : "确认绑定"}</button>}
                <button className="secondary-button" disabled={state.isMutating} onClick={() => void state.issueTicket(port)} type="button">{english ? "Regenerate" : "重新生成"}</button>
              </div>
            </div>
          </div>
        ) : (
          <div className="wechat-binding-state">
            <span className="wechat-binding-state__icon wechat-binding-state__icon--idle">⌗</span>
            <div><strong>{english ? "WeChat is not bound" : "尚未绑定微信"}</strong><p>{english ? "The Mini Program code is valid for two minutes and can only be used once." : "小程序码两分钟内有效，并且只能使用一次。"}</p></div>
            <button className="primary-button" disabled={state.isMutating} onClick={() => void state.issueTicket(port)} type="button">{english ? "Generate Code" : "生成小程序码"}</button>
          </div>
        )}
        {state.notice && <div className="wechat-companion__message wechat-companion__message--success" role="status"><span>✓</span><p>{state.notice}</p></div>}
        {state.error && <div className="wechat-companion__message wechat-companion__message--error" role="alert"><span>!</span><p>{state.error}</p><button aria-label={english ? "Dismiss error" : "关闭错误"} onClick={state.clearError} type="button">×</button></div>}
      </SettingsCard>

      <SettingsCard icon="▯" title={english ? "Mini Program Sessions" : "小程序登录设备"} subtitle={english ? "Revoke access for lost devices or sessions you no longer use." : "撤销丢失设备或不再使用的登录。"}>
        {!state.snapshot || state.snapshot.sessions.length === 0 ? <p className="wechat-empty">{english ? "No Mini Program sessions." : "暂无小程序登录记录。"}</p> : (
          <div className="wechat-sessions">{state.snapshot.sessions.map((session) => <SessionRow english={english} key={session.id} session={session} disabled={state.isMutating} onRevoke={() => void state.revokeSession(port, session.id)} />)}</div>
        )}
      </SettingsCard>

      {confirmsUnbind && <div className="wechat-confirm" role="presentation"><section aria-label={english ? "Unbind WeChat" : "解除微信绑定"} role="dialog" aria-modal="true"><h3>{english ? "Unbind WeChat?" : "解除微信绑定？"}</h3><p>{english ? "All Mini Program sessions will be revoked. Desktop devices, projects, and conversations will not be deleted." : "所有微信小程序登录会立即失效。桌面设备、项目和会话不会被删除。"}</p><div><button className="secondary-button" onClick={() => setConfirmsUnbind(false)} type="button">{english ? "Cancel" : "取消"}</button><button className="wechat-danger-button" onClick={() => { setConfirmsUnbind(false); void state.unbind(port); }} type="button">{english ? "Unbind" : "解除绑定"}</button></div></section></div>}
    </section>
  );
}

function SettingsCard({ icon, title, subtitle, children }: { icon: string; title: string; subtitle: string; children: React.ReactNode }) {
  return <section className="wechat-card"><header><span aria-hidden="true">{icon}</span><h3>{title}</h3></header><div className="wechat-card__body">{children}</div><p className="wechat-card__footnote">{subtitle}</p></section>;
}

function SessionRow({ session, english, disabled, onRevoke }: { session: WeChatClientSession; english: boolean; disabled: boolean; onRevoke(): void }) {
  const date = new Date(session.lastSeenAt);
  const lastSeen = Number.isNaN(date.valueOf()) ? session.lastSeenAt : new Intl.DateTimeFormat(english ? "en-US" : "zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(date);
  return <div className="wechat-session"><span aria-hidden="true">{session.revokedAt ? "⊘" : "▯"}</span><div><strong>{english ? "WeChat Mini Program" : "微信小程序"}</strong><small>{english ? `Last active: ${lastSeen}` : `最近活动：${lastSeen}`}</small></div>{session.revokedAt ? <em>{english ? "Revoked" : "已撤销"}</em> : <button className="wechat-danger-button" disabled={disabled} onClick={onRevoke} type="button">{english ? "Revoke" : "撤销"}</button>}</div>;
}

function Loading({ english }: { english: boolean }) {
  return <div className="wechat-loading"><span className="mini-spinner" />{english ? "Loading binding status…" : "正在读取绑定状态…"}</div>;
}
