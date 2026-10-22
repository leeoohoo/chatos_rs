import { useEffect, useMemo, useRef } from "react";
import { useQuickSearchStore } from "../application/quickSearchStore";
import { quickSearchAllowsFileLookup, type QuickSearchResult } from "../domain/quickSearch";
import type { QuickSearchPort } from "../ports/QuickSearchPort";
import "./quickSearch.css";

const FILE_SEARCH_DEBOUNCE_MS = 150;

export function QuickSearchSurface({ port }: { port: QuickSearchPort }) {
  const state = useQuickSearchStore();
  const inputRef = useRef<HTMLInputElement>(null);
  const english = state.language === "en-US";
  const selected = state.results[state.selectedIndex];

  useEffect(() => {
    void state.open(port);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void port.subscribePresented(() => {
      void useQuickSearchStore.getState().open(port);
      window.setTimeout(() => inputRef.current?.focus(), 0);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [port]);

  useEffect(() => {
    if (!state.presentationId) return undefined;
    void state.search(port, false);
    if (!quickSearchAllowsFileLookup(state.query)) return undefined;
    const timer = window.setTimeout(() => {
      void useQuickSearchStore.getState().search(port, true);
    }, FILE_SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [port, state.presentationId, state.query]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void state.close(port);
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        state.moveSelection(1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        state.moveSelection(-1);
      } else if (event.key === "Enter" && selected) {
        event.preventDefault();
        void state.execute(port, selected);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [port, selected, state]);

  const grouped = useMemo(() => groupResults(state.results), [state.results]);

  return <main className="quick-search-surface">
    <header data-tauri-drag-region>
      <span aria-hidden="true" className="quick-search-surface__magnifier">⌕</span>
      <input
        aria-label={english ? "Quick Search" : "快捷搜索"}
        autoFocus
        disabled={!state.presentationId}
        maxLength={256}
        onChange={(event) => state.setQuery(event.target.value)}
        placeholder={english ? "Search apps, files, ChatOS, and actions" : "搜索应用、文件、ChatOS 和操作"}
        ref={inputRef}
        value={state.query}
      />
      {(state.isLoading || state.isSearchingFiles) && <span aria-label={english ? "Searching" : "正在搜索"} className="mini-spinner" />}
      <kbd>Ctrl Space</kbd>
      <button aria-label={english ? "Close" : "关闭"} onClick={() => void state.close(port)} type="button">×</button>
    </header>

    {state.error && <div className="quick-search-surface__error" role="alert">
      <span>!</span><p>{state.error}</p><button onClick={state.clearError} type="button">×</button>
    </div>}

    <section className="quick-search-surface__content">
      {state.results.length === 0 ? <div className="quick-search-surface__empty">
        <span aria-hidden="true">✦</span>
        <strong>{state.isLoading ? (english ? "Preparing Quick Search…" : "正在准备快捷搜索…") : (english ? "Type to search this PC" : "输入内容搜索这台电脑")}</strong>
        <p>{state.diagnostic ?? (english ? "Use > for actions, @ for ChatOS, and / for files." : "使用 > 搜索操作、@ 搜索 ChatOS、/ 搜索文件。")}</p>
      </div> : <div className="quick-search-surface__groups">
        {grouped.map((group) => <section key={group.kind}>
          <h2>{groupTitle(group.kind, english)}</h2>
          <div>{group.results.map(({ result, index }) => <ResultRow
            english={english}
            key={result.id}
            result={result}
            selected={index === state.selectedIndex}
            onExecute={() => void state.execute(port, result)}
            onSelect={() => state.select(index)}
          />)}</div>
        </section>)}
      </div>}
    </section>

    <footer>
      <span>{state.diagnostic ?? (english ? "Actions stay on this PC" : "操作仅在这台电脑上执行")}</span>
      <div><kbd>↑↓</kbd>{english ? "Navigate" : "选择"}<kbd>Enter</kbd>{english ? "Open" : "打开"}<kbd>Esc</kbd>{english ? "Close" : "关闭"}</div>
    </footer>
  </main>;
}

function ResultRow({ english, onExecute, onSelect, result, selected }: {
  english: boolean;
  onExecute(): void;
  onSelect(): void;
  result: QuickSearchResult;
  selected: boolean;
}) {
  return <article
    aria-current={selected ? "true" : undefined}
    className={selected ? "is-selected" : ""}
    onClick={onSelect}
    onDoubleClick={onExecute}
  >
    <span className="quick-search-result__icon" aria-hidden="true">{iconFor(result.icon)}</span>
    <div><strong>{result.title}</strong>{result.subtitle && <small>{result.subtitle}</small>}</div>
    {selected && <span aria-label={english ? "Press Enter to open" : "按回车打开"}>↵</span>}
  </article>;
}

function groupResults(results: QuickSearchResult[]) {
  const order: QuickSearchResult["kind"][] = ["chatOs", "application", "file", "action"];
  return order.flatMap((kind) => {
    const grouped = results
      .map((result, index) => ({ result, index }))
      .filter((value) => value.result.kind === kind);
    return grouped.length ? [{ kind, results: grouped }] : [];
  });
}

function groupTitle(kind: QuickSearchResult["kind"], english: boolean) {
  const labels = {
    chatOs: "ChatOS",
    application: english ? "Applications" : "应用程序",
    file: english ? "Files & Folders" : "文件与文件夹",
    action: english ? "Actions" : "操作",
  } as const;
  return labels[kind];
}

function iconFor(icon: QuickSearchResult["icon"]) {
  return ({
    project: "▰",
    contact: "●",
    application: "◆",
    file: "▤",
    folder: "▱",
    screenshot: "⌗",
    screenRecording: "◉",
    clipboard: "▣",
    settings: "⚙",
    permissions: "◇",
  } as const)[icon];
}
