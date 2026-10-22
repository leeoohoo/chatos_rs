import { useEffect, useMemo } from "react";
import { selectFilteredEntries, useClipboardHistoryStore } from "../application/clipboardHistoryStore";
import type { ClipboardEntry } from "../domain/clipboardHistory";
import type { ClipboardHistoryPort } from "../ports/ClipboardHistoryPort";
import "./clipboardHistory.css";

export function ClipboardHistorySurface({ port }: { port: ClipboardHistoryPort }) {
  const state = useClipboardHistoryStore();
  const english = state.language === "en-US";
  const entries = useMemo(() => selectFilteredEntries(state), [state.entries, state.query]);
  const selected = entries[state.selectedIndex];

  useEffect(() => {
    void state.load(port);
    const timer = window.setInterval(() => void useClipboardHistoryStore.getState().refresh(port), 1_500);
    return () => window.clearInterval(timer);
  }, [port]);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); void port.close(); }
      else if (event.key === "ArrowDown") { event.preventDefault(); state.moveSelection(1); }
      else if (event.key === "ArrowUp") { event.preventDefault(); state.moveSelection(-1); }
      else if (event.key === "Enter" && selected) { event.preventDefault(); void state.restore(port, selected); }
      else if (event.key === "Delete" && selected) { event.preventDefault(); void state.delete(port, selected); }
      else if (event.key.toLocaleLowerCase() === "p" && event.ctrlKey && selected) { event.preventDefault(); void state.setPinned(port, selected); }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [port, selected, state]);

  return <main className="clipboard-surface">
    <header data-tauri-drag-region>
      <span aria-hidden="true">▤</span>
      <input autoFocus aria-label={english ? "Search clipboard history" : "搜索剪贴板历史"} onChange={(event) => state.setQuery(event.target.value)} placeholder={english ? "Search clipboard history" : "搜索剪贴板历史"} value={state.query} />
      {state.isLoading && <span className="mini-spinner" />}
      <button aria-label={english ? "Clear history" : "清空历史"} disabled={state.entries.length === 0 || state.isMutating} onClick={() => void state.clear(port)} type="button">⌫</button>
      <button aria-label={english ? "Close" : "关闭"} onClick={() => void port.close()} type="button">×</button>
    </header>
    {state.error && <div className="clipboard-surface__error" role="alert"><span>!</span><p>{state.error}</p><button onClick={state.clearError} type="button">×</button></div>}
    <section className="clipboard-surface__content">
      {state.isLoading && state.entries.length === 0 ? <div className="clipboard-surface__empty"><span className="mini-spinner" /><strong>{english ? "Loading clipboard history…" : "正在读取剪贴板历史…"}</strong></div>
        : entries.length === 0 ? <div className="clipboard-surface__empty"><span>▤</span><strong>{state.entries.length ? (english ? "No Matching Clipboard Items" : "没有匹配的剪贴板内容") : (english ? "Clipboard History Is Empty" : "剪贴板历史还是空的")}</strong><p>{state.entries.length ? (english ? "Try another keyword." : "换一个关键词再试试。") : (english ? "Copy text, images, files, or links and they will appear here. Sensitive clipboard types are skipped." : "复制文字、图片、文件或链接后会显示在这里；密码等敏感剪贴板内容不会记录。")}</p></div>
        : <div className="clipboard-surface__list">{entries.map((entry, index) => <ClipboardRow english={english} entry={entry} key={entry.id} selected={index === state.selectedIndex} onDelete={() => void state.delete(port, entry)} onPin={() => void state.setPinned(port, entry)} onRestore={() => { state.select(index); void state.restore(port, entry); }} />)}</div>}
    </section>
    <footer><span>{english ? "Stored only on this PC" : "仅保存在这台电脑上"}</span><div><kbd>Ctrl P</kbd>{english ? "Pin" : "固定"}<kbd>Del</kbd>{english ? "Delete" : "删除"}<kbd>Enter</kbd>{english ? "Copy" : "复制"}</div></footer>
  </main>;
}

function ClipboardRow({ english, entry, onDelete, onPin, onRestore, selected }: { english: boolean; entry: ClipboardEntry; onDelete(): void; onPin(): void; onRestore(): void; selected: boolean }) {
  const title = entry.preview || ({ text: english ? "Text" : "文本", url: english ? "Link" : "链接", files: english ? "Files" : "文件", image: english ? "Image" : "图片" } as const)[entry.kind];
  return <article aria-current={selected ? "true" : undefined} className={selected ? "is-selected" : ""} onClick={onRestore}>
    <div className="clipboard-surface__preview">{entry.previewUrl ? <img alt="" src={entry.previewUrl} /> : <span>{({ text: "≡", url: "↗", files: "▧", image: "▨" } as const)[entry.kind]}</span>}</div>
    <div><strong>{title}</strong><small>{relativeTime(entry.updatedAtUnixMs, english)} · {formatBytes(entry.byteCount)}</small></div>
    <div className="clipboard-surface__actions">{entry.isPinned && <span title={english ? "Pinned" : "已固定"}>●</span>}<button aria-label={english ? "Toggle pin" : "切换固定"} onClick={(event) => { event.stopPropagation(); onPin(); }} type="button">⌖</button><button aria-label={english ? "Delete item" : "删除记录"} onClick={(event) => { event.stopPropagation(); onDelete(); }} type="button">×</button></div>
  </article>;
}

function relativeTime(value: number, english: boolean) {
  const minutes = Math.max(0, Math.floor((Date.now() - value) / 60_000));
  if (minutes < 1) return english ? "Now" : "刚刚";
  if (minutes < 60) return english ? `${minutes}m ago` : `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return english ? `${hours}h ago` : `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  return english ? `${days}d ago` : `${days} 天前`;
}

function formatBytes(value: number) { return value < 1024 ? `${value} B` : value < 1024 * 1024 ? `${(value / 1024).toFixed(1)} KB` : `${(value / 1024 / 1024).toFixed(1)} MB`; }
