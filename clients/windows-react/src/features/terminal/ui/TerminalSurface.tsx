import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";

export interface TerminalSurfaceHandle {
  clear(): void;
  clearSearch(): void;
  findNext(query: string): void;
  findPrevious(query: string): void;
  focus(): void;
}

type TerminalEvent =
  | { kind: "output"; cursor: number; data: Uint8Array }
  | { kind: "terminated"; cursor: number };

interface TerminalSurfaceProps {
  ariaLabel: string;
  className: string;
  events: TerminalEvent[];
  lifecycleKey: unknown;
  onInput(data: Uint8Array): void;
  onOsc7(value: string): void;
  onResize(cols: number, rows: number): void;
  onTitleChange(title: string): void;
  outputGeneration: number;
}

export const TerminalSurface = forwardRef<TerminalSurfaceHandle, TerminalSurfaceProps>(
  function TerminalSurface(props, ref) {
    const containerRef = useRef<HTMLDivElement>(null);
    const terminalRef = useRef<Terminal | null>(null);
    const searchRef = useRef<SearchAddon | null>(null);
    const lastWrittenCursor = useRef(0);
    const resizeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const callbacks = useRef(props);
    const [ready, setReady] = useState(false);
    callbacks.current = props;

    useImperativeHandle(ref, () => ({
      clear() { terminalRef.current?.clear(); },
      clearSearch() {
        searchRef.current?.clearDecorations();
        terminalRef.current?.clearSelection();
      },
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
        callbacks.current.onInput(new TextEncoder().encode(value));
      });
      const binaryDisposable = terminal.onBinary((value) => {
        callbacks.current.onInput(Uint8Array.from(
          value,
          (character) => character.charCodeAt(0) & 0xff,
        ));
      });
      const resizeDisposable = terminal.onResize(({ cols, rows }) => {
        if (resizeTimer.current) clearTimeout(resizeTimer.current);
        resizeTimer.current = setTimeout(() => callbacks.current.onResize(cols, rows), 80);
      });
      const titleDisposable = terminal.onTitleChange((title) => {
        callbacks.current.onTitleChange(title);
      });
      const oscDisposable = terminal.parser.registerOscHandler(7, (value) => {
        callbacks.current.onOsc7(value);
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
    }, [props.lifecycleKey]);

    useEffect(() => {
      lastWrittenCursor.current = 0;
      terminalRef.current?.reset();
    }, [props.outputGeneration]);

    useEffect(() => {
      const terminal = terminalRef.current;
      if (!terminal || !ready) return;
      for (const event of props.events) {
        if (event.cursor <= lastWrittenCursor.current) continue;
        if (event.kind === "output") terminal.write(event.data);
        lastWrittenCursor.current = event.cursor;
      }
    }, [props.events, props.outputGeneration, ready]);

    return (
      <div
        aria-label={props.ariaLabel}
        className={props.className}
        onClick={() => terminalRef.current?.focus()}
        ref={containerRef}
        role="application"
      />
    );
  },
);

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
