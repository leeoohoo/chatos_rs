import { useLayoutEffect, useRef, useState } from "react";
import { useRemoteTerminalStore } from "../application/remoteTerminalStore";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";
import { RemoteTerminalSurface, type RemoteTerminalSurfaceHandle } from "./RemoteTerminalSurface";
import "./remoteTerminal.css";

export function RemoteTerminalWorkspace({
  connectionId,
  connectionName,
  initialWorkingDirectory,
  ownerUserId,
  port,
}: {
  connectionId: string;
  connectionName: string;
  initialWorkingDirectory: string;
  ownerUserId: string;
  port: RemoteTerminalPort;
}) {
  const state = useRemoteTerminalStore();
  const surfaceRef = useRef<RemoteTerminalSurfaceHandle>(null);
  const [searchText, setSearchText] = useState("");

  useLayoutEffect(() => {
    void useRemoteTerminalStore.getState().activate(port, ownerUserId, connectionId, connectionName, initialWorkingDirectory);
    return () => useRemoteTerminalStore.getState().deactivate(port);
  }, [connectionId, connectionName, initialWorkingDirectory, ownerUserId, port]);

  const search = (direction: "next" | "previous") => {
    const query = searchText.trim();
    if (!query) return;
    if (direction === "next") surfaceRef.current?.findNext(query);
    else surfaceRef.current?.findPrevious(query);
  };
  const clear = () => {
    surfaceRef.current?.clear();
    state.clearOutput();
  };

  return (
    <section className="remote-terminal" aria-label="远程终端工作区">
      <header className="remote-terminal__toolbar">
        <div className="remote-terminal__path"><span aria-hidden="true">⌁</span><code>{state.workingDirectory}</code></div>
        <span className={`remote-terminal-status remote-terminal-status--${state.phase}`}>{statusLabel(state.phase, state.exitCode)}</span>
        <label className="remote-terminal-search"><span aria-hidden="true">⌕</span><input aria-label="搜索终端输出" onChange={(event) => setSearchText(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") search("next"); }} placeholder="搜索终端输出" value={searchText} /></label>
        <button aria-label="上一个匹配项" disabled={!searchText.trim()} onClick={() => search("previous")} type="button">↑</button>
        <button aria-label="下一个匹配项" disabled={!searchText.trim()} onClick={() => search("next")} type="button">↓</button>
        <button onClick={clear} type="button">清屏</button>
        <button disabled={state.phase !== "running" || state.activeAction != null} onClick={() => void state.interrupt(port)} type="button">中断</button>
        {state.phase === "running" || state.phase === "connecting" ? (
          <button className="danger-link" disabled={state.activeAction != null} onClick={() => void state.disconnect(port)} type="button">断开</button>
        ) : (
          <button className="primary-button" disabled={state.activeAction != null} onClick={() => void state.reconnect(port)} type="button">重新连接</button>
        )}
      </header>
      <div className="remote-terminal__title"><strong>{state.title || connectionName}</strong><span>{connectionName}</span></div>
      <RemoteTerminalSurface port={port} ref={surfaceRef} />
      {(state.errorMessage || state.warningMessage) && (
        <div className={`remote-terminal__feedback ${state.errorMessage ? "is-error" : ""}`} role={state.errorMessage ? "alert" : "status"}>
          <span>{state.errorMessage ?? state.warningMessage}</span>
          {state.phase === "failed" && <button onClick={() => void state.reconnect(port)} type="button">重新连接</button>}
          <button aria-label="关闭提示" onClick={state.dismissFeedback} type="button">×</button>
        </div>
      )}
      {state.verificationPrompt && (
        <div className="remote-terminal-dialog-backdrop" role="presentation">
          <section aria-labelledby="remote-terminal-verification-title" aria-modal="true" className="remote-terminal-dialog" role="dialog">
            <span aria-hidden="true">⌁</span>
            <h3 id="remote-terminal-verification-title">SSH 二次验证</h3>
            <p>{state.verificationPrompt}</p>
            <label><span>验证码</span><input autoFocus disabled={state.phase === "connecting"} maxLength={1_024} onChange={(event) => state.setVerificationCode(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && state.verificationCode.trim()) void state.submitVerification(port); }} type="password" value={state.verificationCode} /></label>
            {state.errorMessage && <p className="remote-terminal-dialog__error" role="alert">{state.errorMessage}</p>}
            <footer><button disabled={state.activeAction != null} onClick={() => void state.cancelVerification(port)} type="button">取消</button><button className="primary-button" disabled={state.phase === "connecting" || !state.verificationCode.trim()} onClick={() => void state.submitVerification(port)} type="button">验证并连接</button></footer>
          </section>
        </div>
      )}
    </section>
  );
}

function statusLabel(phase: ReturnType<typeof useRemoteTerminalStore.getState>["phase"], exitCode: number | null) {
  switch (phase) {
    case "idle": return "准备连接";
    case "connecting": return "连接中";
    case "running": return "已连接";
    case "exited": return exitCode == null ? "连接已退出" : `已退出 (${exitCode})`;
    case "failed": return "连接失败";
    case "closed": return "已断开";
  }
}
