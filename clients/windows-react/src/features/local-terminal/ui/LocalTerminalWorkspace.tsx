import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  useLocalTerminalStore,
  type LocalTerminalPhase,
} from "../application/localTerminalStore";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";
import { LocalTerminalSurface, type LocalTerminalSurfaceHandle } from "./LocalTerminalSurface";
import "./localTerminal.css";

export function LocalTerminalWorkspace({
  hostReady,
  ownerUserId,
  port,
}: {
  hostReady: boolean;
  ownerUserId: string;
  port: LocalTerminalPort;
}) {
  const state = useLocalTerminalStore();
  const surfaceRef = useRef<LocalTerminalSurfaceHandle>(null);
  const [searchText, setSearchText] = useState("");
  const selected = state.tabs.find((tab) => tab.id === state.selectedTabId) ?? state.tabs[0];

  useLayoutEffect(() => {
    void useLocalTerminalStore.getState().activate(port, ownerUserId, hostReady);
    return () => useLocalTerminalStore.getState().deactivate(port);
  }, [hostReady, ownerUserId, port]);

  useEffect(() => setSearchText(""), [selected?.id]);

  if (!hostReady) {
    return (
      <section className="local-terminal local-terminal--unavailable" aria-label="本机终端工作区">
        <span aria-hidden="true">⌘</span>
        <h2>本机终端暂不可用</h2>
        <p>Local Agent Host 就绪后，终端会在受限的 Windows PowerShell 会话中启动。</p>
      </section>
    );
  }

  const search = (direction: "next" | "previous") => {
    const query = searchText.trim();
    if (!query) return;
    if (direction === "next") surfaceRef.current?.findNext(query);
    else surfaceRef.current?.findPrevious(query);
  };
  const clear = () => {
    if (!selected) return;
    surfaceRef.current?.clear();
    state.clearOutput(selected.id);
  };

  return (
    <section className="local-terminal" aria-label="本机终端工作区">
      <header className="local-terminal-tabs" aria-label="终端标签">
        <div className="local-terminal-tabs__items">
          {state.tabs.map((tab) => (
            <div
              className={`local-terminal-tab ${tab.id === selected?.id ? "local-terminal-tab--selected" : ""}`}
              key={tab.id}
            >
              <button onClick={() => state.selectTerminal(tab.id)} type="button">
                <span className={`local-terminal-tab__status local-terminal-tab__status--${tab.phase}`} />
                <span>{tab.title}</span>
              </button>
              <button
                aria-label={`关闭 ${tab.title}`}
                className="local-terminal-tab__close"
                onClick={() => state.closeTerminal(port, tab.id)}
                type="button"
              >×</button>
            </div>
          ))}
          <button
            aria-label="新建终端"
            className="local-terminal-tabs__add"
            disabled={state.tabs.length >= 4}
            onClick={() => state.createTerminal(port)}
            title={state.tabs.length >= 4 ? "最多同时打开 4 个终端" : "新建终端"}
            type="button"
          >＋</button>
        </div>
      </header>

      {selected ? (
        <>
          <header className="local-terminal__toolbar">
            <div className="local-terminal__path">
              <span aria-hidden="true">▱</span>
              <code>{selected.workingDirectory}</code>
            </div>
            <span className={`local-terminal-status local-terminal-status--${selected.phase}`}>
              {statusLabel(selected.phase, selected.exitCode)}
            </span>
            <span className="local-terminal-shell">{selected.shellName}</span>
            <label className="local-terminal-search">
              <span aria-hidden="true">⌕</span>
              <input
                aria-label="搜索终端输出"
                onChange={(event) => setSearchText(event.target.value)}
                onKeyDown={(event) => { if (event.key === "Enter") search("next"); }}
                placeholder="搜索终端输出"
                value={searchText}
              />
            </label>
            <button
              aria-label="上一个匹配项"
              disabled={!searchText.trim()}
              onClick={() => search("previous")}
              type="button"
            >↑</button>
            <button
              aria-label="下一个匹配项"
              disabled={!searchText.trim()}
              onClick={() => search("next")}
              type="button"
            >↓</button>
            <button onClick={clear} type="button">清屏</button>
            <button
              className="danger-link"
              disabled={selected.phase !== "running" || selected.activeAction != null}
              onClick={() => void state.interrupt(port, selected.id)}
              type="button"
            >中断</button>
          </header>
          <LocalTerminalSurface key={selected.id} port={port} ref={surfaceRef} tabId={selected.id} />
          {(selected.errorMessage || selected.warningMessage) && (
            <div
              className={`local-terminal__feedback ${selected.errorMessage ? "is-error" : ""}`}
              role={selected.errorMessage ? "alert" : "status"}
            >
              <span>{selected.errorMessage ?? selected.warningMessage}</span>
              <button aria-label="关闭提示" onClick={() => state.dismissFeedback(selected.id)} type="button">×</button>
            </div>
          )}
        </>
      ) : (
        <div className="local-terminal__empty"><span className="mini-spinner" />正在准备终端…</div>
      )}
    </section>
  );
}

function statusLabel(phase: LocalTerminalPhase, exitCode: number | null) {
  switch (phase) {
    case "starting": return "准备中";
    case "running": return "已连接";
    case "exited": return exitCode == null ? "已退出" : `已退出 (${exitCode})`;
    case "failed": return "启动失败";
    case "closed": return "已关闭";
  }
}
