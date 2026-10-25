import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  useLocalTerminalStore,
  type LocalTerminalPhase,
} from "../application/localTerminalStore";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";
import { localTerminalDisplayPath } from "../domain/localTerminal";
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
  const actionsRef = useRef<HTMLDetailsElement>(null);
  const [searchText, setSearchText] = useState("");
  const selected = state.tabs.find((tab) => tab.id === state.selectedTabId) ?? state.tabs[0];

  useLayoutEffect(() => {
    void useLocalTerminalStore.getState().activate(port, ownerUserId, hostReady);
    return () => useLocalTerminalStore.getState().deactivate(port);
  }, [hostReady, ownerUserId, port]);

  useEffect(() => {
    setSearchText("");
    actionsRef.current?.removeAttribute("open");
  }, [selected?.id]);

  useEffect(() => {
    const query = searchText.trim();
    if (query) surfaceRef.current?.findNext(query);
    else surfaceRef.current?.clearSearch();
  }, [searchText, selected?.id]);

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
    actionsRef.current?.removeAttribute("open");
  };
  const interrupt = () => {
    if (!selected) return;
    actionsRef.current?.removeAttribute("open");
    void state.interrupt(port, selected.id);
  };
  const closeSelected = () => {
    if (!selected) return;
    actionsRef.current?.removeAttribute("open");
    state.closeTerminal(port, selected.id);
  };

  return (
    <section className="local-terminal" aria-label="本机终端工作区">
      <header className="local-terminal-commandbar">
        <strong title={selected?.title ?? "终端"}>{selected?.title ?? "终端"}</strong>
        {selected && (
          <>
            <label className="local-terminal-search">
              <span aria-hidden="true">⌕</span>
              <input
                aria-label="搜索终端输出"
                onChange={(event) => setSearchText(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") search(event.shiftKey ? "previous" : "next");
                }}
                placeholder="搜索终端输出"
                value={searchText}
              />
            </label>
            <div className="local-terminal-commandbar__actions">
              <button aria-label="上一个匹配项" disabled={!searchText.trim()} onClick={() => search("previous")} type="button">⌃</button>
              <button aria-label="下一个匹配项" disabled={!searchText.trim()} onClick={() => search("next")} type="button">⌄</button>
              <details className="local-terminal-actions" ref={actionsRef}>
                <summary aria-label="终端操作" role="button" title="终端操作">•••</summary>
                <div role="menu">
                  <button onClick={clear} role="menuitem" type="button"><span aria-hidden="true">⌫</span>清屏</button>
                  <button disabled={selected.phase !== "running" || selected.activeAction != null} onClick={interrupt} role="menuitem" type="button"><span aria-hidden="true">■</span>中断</button>
                  <hr />
                  <button onClick={closeSelected} role="menuitem" type="button"><span aria-hidden="true">×</span>关闭终端</button>
                </div>
              </details>
            </div>
          </>
        )}
      </header>
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
            onClick={() => state.createTerminal(port)}
            title="新建终端"
            type="button"
          >＋</button>
        </div>
      </header>

      {selected ? (
        <>
          <header className="local-terminal__toolbar">
            <div className="local-terminal__path">
              <span aria-hidden="true">▱</span>
              <code>{localTerminalDisplayPath(selected.workingDirectory)}</code>
            </div>
            <span className={`local-terminal-status local-terminal-status--${selected.phase}`}>
              {selected.shellName}
            </span>
            <span className="local-terminal-shell">{statusLabel(selected.phase, selected.exitCode)}</span>
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
