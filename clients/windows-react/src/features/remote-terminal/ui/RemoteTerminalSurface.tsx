import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import { useRemoteTerminalStore } from "../application/remoteTerminalStore";
import { workingDirectoryFromOsc } from "../domain/remoteTerminal";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";
import "@xterm/xterm/css/xterm.css";

export interface RemoteTerminalSurfaceHandle {
  clear(): void;
  findNext(query: string): void;
  findPrevious(query: string): void;
  focus(): void;
}

export const RemoteTerminalSurface = forwardRef<RemoteTerminalSurfaceHandle, { port: RemoteTerminalPort }>(
  function RemoteTerminalSurface({ port }, ref) {
    const containerRef = useRef<HTMLDivElement>(null);
    const terminalRef = useRef<Terminal | null>(null);
    const searchRef = useRef<SearchAddon | null>(null);
    const lastWrittenCursor = useRef(0);
    const resizeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const [ready, setReady] = useState(false);
    const events = useRemoteTerminalStore((state) => state.events);
    const outputGeneration = useRemoteTerminalStore((state) => state.outputGeneration);

    useImperativeHandle(ref, () => ({
      clear() { terminalRef.current?.clear(); },
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
      const sendText = (value: string) => useRemoteTerminalStore.getState().sendInput(port, new TextEncoder().encode(value));
      const dataDisposable = terminal.onData(sendText);
      const binaryDisposable = terminal.onBinary((value) => {
        useRemoteTerminalStore.getState().sendInput(
          port,
          Uint8Array.from(value, (character) => character.charCodeAt(0) & 0xff),
        );
      });
      const resizeDisposable = terminal.onResize(({ cols, rows }) => {
        if (resizeTimer.current) clearTimeout(resizeTimer.current);
        resizeTimer.current = setTimeout(() => {
          void useRemoteTerminalStore.getState().resize(port, cols, rows);
        }, 80);
      });
      const titleDisposable = terminal.onTitleChange((title) => useRemoteTerminalStore.getState().updateTitle(title));
      const oscDisposable = terminal.parser.registerOscHandler(7, (value) => {
        const directory = workingDirectoryFromOsc(value);
        if (directory) useRemoteTerminalStore.getState().updateWorkingDirectory(directory);
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
    }, [port]);

    useEffect(() => {
      lastWrittenCursor.current = 0;
      terminalRef.current?.reset();
    }, [outputGeneration]);

    useEffect(() => {
      const terminal = terminalRef.current;
      if (!terminal || !ready) return;
      for (const event of events) {
        if (event.cursor <= lastWrittenCursor.current) continue;
        if (event.kind === "output") terminal.write(event.data);
        lastWrittenCursor.current = event.cursor;
      }
    }, [events, outputGeneration, ready]);

    return <div aria-label="远程终端输出" className="remote-terminal-surface" onClick={() => terminalRef.current?.focus()} ref={containerRef} role="application" />;
  },
);

function terminalTheme() {
  const dark = typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;
  return dark
    ? { background: "#1c1c1e", foreground: "#f2f2f7", cursor: "#0a84ff", selectionBackground: "#0a84ff66" }
    : { background: "#ffffff", foreground: "#1d1d1f", cursor: "#0a84ff", selectionBackground: "#0a84ff4d" };
}
