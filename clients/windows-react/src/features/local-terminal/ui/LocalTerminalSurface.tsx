import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useLocalTerminalStore } from "../application/localTerminalStore";
import { localWorkingDirectoryFromOsc } from "../domain/localTerminal";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";
import "@xterm/xterm/css/xterm.css";

export interface LocalTerminalSurfaceHandle {
  clear(): void;
  clearSearch(): void;
  findNext(query: string): void;
  findPrevious(query: string): void;
  focus(): void;
}

export const LocalTerminalSurface = forwardRef<
  LocalTerminalSurfaceHandle,
  { port: LocalTerminalPort; tabId: string }
>(function LocalTerminalSurface({ port, tabId }, ref) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const lastWrittenCursor = useRef(0);
  const resizeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [ready, setReady] = useState(false);
  const tab = useLocalTerminalStore((state) => state.tabs.find((candidate) => candidate.id === tabId));

  useImperativeHandle(ref, () => ({
    clear() { terminalRef.current?.clear(); },
    clearSearch() { searchRef.current?.clearDecorations(); },
    findNext(query) { searchRef.current?.findNext(query, { incremental: true }); },
    findPrevious(query) { searchRef.current?.findPrevious(query, { incremental: true }); },
    focus() { terminalRef.current?.focus(); },
  }), []);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: true,
      fontFamily: '"SFMono-Regular", "Cascadia Mono", Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.18,
      scrollback: 10_000,
      theme: terminalTheme(),
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.open(container);
    terminalRef.current = terminal;
    searchRef.current = search;
    setReady(true);
    const dataDisposable = terminal.onData((value) => {
      useLocalTerminalStore.getState().sendInput(port, tabId, new TextEncoder().encode(value));
    });
    const binaryDisposable = terminal.onBinary((value) => {
      useLocalTerminalStore.getState().sendInput(
        port,
        tabId,
        Uint8Array.from(value, (character) => character.charCodeAt(0) & 0xff),
      );
    });
    const resizeDisposable = terminal.onResize(({ cols, rows }) => {
      if (resizeTimer.current) clearTimeout(resizeTimer.current);
      resizeTimer.current = setTimeout(() => {
        void useLocalTerminalStore.getState().resize(port, tabId, cols, rows);
      }, 80);
    });
    const titleDisposable = terminal.onTitleChange((title) => {
      useLocalTerminalStore.getState().updateTitle(tabId, title);
    });
    const oscDisposable = terminal.parser.registerOscHandler(7, (value) => {
      const directory = localWorkingDirectoryFromOsc(value);
      if (directory) useLocalTerminalStore.getState().updateWorkingDirectory(tabId, directory);
      return true;
    });
    const fitTerminal = () => {
      try { fit.fit(); } catch { /* xterm can be between layout passes */ }
    };
    fitTerminal();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fitTerminal);
    observer?.observe(container);
    queueMicrotask(() => { fitTerminal(); terminal.focus(); });
    return () => {
      if (resizeTimer.current) clearTimeout(resizeTimer.current);
      observer?.disconnect();
      dataDisposable.dispose();
      binaryDisposable.dispose();
      resizeDisposable.dispose();
      titleDisposable.dispose();
      oscDisposable.dispose();
      terminal.dispose();
      terminalRef.current = null;
      searchRef.current = null;
      setReady(false);
    };
  }, [port, tabId]);

  useEffect(() => {
    lastWrittenCursor.current = 0;
    terminalRef.current?.reset();
  }, [tab?.outputGeneration]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal || !ready || !tab) return;
    for (const event of tab.events) {
      if (event.cursor <= lastWrittenCursor.current) continue;
      if (event.kind === "output") terminal.write(event.data);
      lastWrittenCursor.current = event.cursor;
    }
  }, [ready, tab]);

  return (
    <div
      aria-label="本机终端输出"
      className="local-terminal-surface"
      onClick={() => terminalRef.current?.focus()}
      ref={containerRef}
      role="application"
    />
  );
});

function terminalTheme() {
  const dark = typeof matchMedia !== "undefined"
    && matchMedia("(prefers-color-scheme: dark)").matches;
  return dark
    ? {
        background: "#1c1c1e",
        foreground: "#f2f2f7",
        cursor: "#0a84ff",
        selectionBackground: "#0a84ff66",
      }
    : {
        background: "#ffffff",
        foreground: "#1d1d1f",
        cursor: "#0a84ff",
        selectionBackground: "#0a84ff4d",
      };
}
