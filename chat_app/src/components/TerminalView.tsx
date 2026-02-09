import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { useChatStoreFromContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { apiClient } from '../lib/api/client';
import { normalizeTerminalLog } from '../lib/store/helpers/terminals';
import { useTheme } from '../hooks/useTheme';
import { cn } from '../lib/utils';
import type { TerminalLog } from '../types';

interface TerminalViewProps {
  className?: string;
}

type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
type HistoryState = 'idle' | 'loading' | 'ready' | 'error';

const buildWsUrl = (baseUrl: string, path: string) => {
  const cleanedBase = baseUrl.endsWith('/') ? baseUrl.slice(0, -1) : baseUrl;
  const cleanedPath = path.startsWith('/') ? path : `/${path}`;
  if (cleanedBase.startsWith('http://') || cleanedBase.startsWith('https://')) {
    return cleanedBase.replace(/^http/, 'ws') + cleanedPath;
  }
  const { protocol, host } = window.location;
  const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:';
  return `${wsProtocol}//${host}${cleanedBase}${cleanedPath}`;
};

const getThemeColors = (theme: 'light' | 'dark') => {
  if (theme === 'dark') {
    return {
      background: '#0f172a',
      foreground: '#e2e8f0',
      cursor: '#f8fafc',
      selection: 'rgba(148, 163, 184, 0.35)',
      black: '#0f172a',
      red: '#f87171',
      green: '#34d399',
      yellow: '#fbbf24',
      blue: '#60a5fa',
      magenta: '#c084fc',
      cyan: '#22d3ee',
      white: '#e2e8f0',
      brightBlack: '#334155',
      brightRed: '#fca5a5',
      brightGreen: '#6ee7b7',
      brightYellow: '#fde68a',
      brightBlue: '#93c5fd',
      brightMagenta: '#d8b4fe',
      brightCyan: '#67e8f9',
      brightWhite: '#f8fafc',
    };
  }
  return {
    background: '#ffffff',
    foreground: '#0f172a',
    cursor: '#0f172a',
    selection: 'rgba(59, 130, 246, 0.25)',
    black: '#0f172a',
    red: '#dc2626',
    green: '#16a34a',
    yellow: '#d97706',
    blue: '#2563eb',
    magenta: '#7c3aed',
    cyan: '#0891b2',
    white: '#e2e8f0',
    brightBlack: '#475569',
    brightRed: '#ef4444',
    brightGreen: '#22c55e',
    brightYellow: '#f59e0b',
    brightBlue: '#3b82f6',
    brightMagenta: '#8b5cf6',
    brightCyan: '#06b6d4',
    brightWhite: '#f8fafc',
  };
};

export const TerminalView: React.FC<TerminalViewProps> = ({ className }) => {
  const {
    currentTerminal,
    loadTerminals,
  } = useChatStoreFromContext();
  const apiClientFromContext = useChatApiClientFromContext();
  const { actualTheme } = useTheme();
  const terminalRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const dataHandlerRef = useRef<ReturnType<XTerm['onData']> | null>(null);

  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [historyState, setHistoryState] = useState<HistoryState>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [connectSeq, setConnectSeq] = useState(0);

  const client = apiClientFromContext ?? apiClient;
  const apiBaseUrl = client.getBaseUrl();

  const terminalTitle = currentTerminal?.name || '终端';
  const terminalCwd = currentTerminal?.cwd || '';
  const terminalStatus = currentTerminal?.status || 'unknown';

  const themeColors = useMemo(() => getThemeColors(actualTheme), [actualTheme]);

  useEffect(() => {
    const term = terminalRef.current;
    if (term) {
      term.options.theme = themeColors;
    }
  }, [themeColors]);

  useEffect(() => {
    if (!currentTerminal || !containerRef.current) {
      return;
    }

    setHistoryState('loading');
    setConnectionState('disconnected');
    setErrorMessage(null);

    const term = new XTerm({
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 3000,
      theme: themeColors,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    term.open(containerRef.current);
    fitAddon.fit();
    term.focus();

    terminalRef.current = term;
    fitRef.current = fitAddon;

    dataHandlerRef.current = term.onData((data) => {
      const ws = socketRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'input', data }));
      }
    });

    const resizeObserver = new ResizeObserver(() => {
      const fit = fitRef.current;
      if (!fit) return;
      fit.fit();
      const active = socketRef.current;
      if (active && active.readyState === WebSocket.OPEN && terminalRef.current) {
        active.send(JSON.stringify({ type: 'resize', cols: terminalRef.current.cols, rows: terminalRef.current.rows }));
      }
    });
    resizeObserver.observe(containerRef.current);
    resizeObserverRef.current = resizeObserver;

    const loadHistory = async () => {
      try {
        const logs = await client.listTerminalLogs(currentTerminal.id);
        const normalized = Array.isArray(logs) ? logs.map(normalizeTerminalLog) : [];
        const outputLogs = normalized.filter((log: TerminalLog) => log.logType === 'output' || log.logType === 'system');
        for (const log of outputLogs) {
          term.write(log.content);
        }
        setHistoryState('ready');
      } catch (error) {
        console.error('Failed to load terminal history:', error);
        setHistoryState('error');
        setErrorMessage(error instanceof Error ? error.message : '加载历史失败');
      } finally {
        setConnectSeq((prev) => prev + 1);
      }
    };

    loadHistory();

    return () => {
      socketRef.current?.close();
      socketRef.current = null;
      dataHandlerRef.current?.dispose();
      dataHandlerRef.current = null;
      resizeObserver.disconnect();
      resizeObserverRef.current = null;
      term.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      setHistoryState('idle');
      setConnectionState('disconnected');
    };
  }, [currentTerminal?.id, client]);

  useEffect(() => {
    if (!currentTerminal) return;
    if (historyState === 'loading') return;

    const wsUrl = buildWsUrl(apiBaseUrl, `/terminals/${currentTerminal.id}/ws`);
    setConnectionState('connecting');
    const ws = new WebSocket(wsUrl);
    socketRef.current = ws;

    ws.onopen = () => {
      setConnectionState('connected');
      const term = terminalRef.current;
      if (term) {
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      }
    };

    ws.onmessage = (event) => {
      try {
        const payload = JSON.parse(event.data);
        if (payload?.type === 'output') {
          terminalRef.current?.write(payload.data ?? '');
        } else if (payload?.type === 'exit') {
          setConnectionState('disconnected');
          loadTerminals();
        } else if (payload?.type === 'state') {
          loadTerminals();
        } else if (payload?.type === 'error') {
          setErrorMessage(payload.error || '终端发生错误');
          setConnectionState('error');
        }
      } catch (err) {
        console.warn('terminal ws message parse failed', err);
      }
    };

    ws.onerror = () => {
      setConnectionState('error');
    };

    ws.onclose = () => {
      setConnectionState('disconnected');
      loadTerminals();
    };

    return () => {
      ws.close();
    };
  }, [currentTerminal?.id, historyState, apiBaseUrl, connectSeq]);

  useEffect(() => {
    loadTerminals();
  }, [loadTerminals]);

  if (!currentTerminal) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        请选择一个终端
      </div>
    );
  }

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground truncate">{terminalTitle}</div>
          <div className="text-xs text-muted-foreground truncate">{terminalCwd}</div>
        </div>
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span className={cn(
            'inline-flex items-center gap-1',
            connectionState === 'connected' ? 'text-emerald-500' : connectionState === 'error' ? 'text-destructive' : 'text-muted-foreground'
          )}>
            <span className={cn(
              'inline-block h-2 w-2 rounded-full',
              connectionState === 'connected' ? 'bg-emerald-500' : connectionState === 'error' ? 'bg-destructive' : 'bg-muted-foreground/50'
            )} />
            {connectionState === 'connected' ? '已连接' : connectionState === 'connecting' ? '连接中' : connectionState === 'error' ? '连接错误' : '未连接'}
          </span>
          <span>状态: {terminalStatus}</span>
          <button
            type="button"
            onClick={() => setConnectSeq((prev) => prev + 1)}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
          >
            重连
          </button>
        </div>
      </div>

      {historyState === 'loading' && (
        <div className="px-4 py-2 text-xs text-muted-foreground">加载历史记录中...</div>
      )}
      {errorMessage && (
        <div className="px-4 py-2 text-xs text-destructive">{errorMessage}</div>
      )}

      <div className="flex-1 overflow-hidden bg-background">
        <div ref={containerRef} className="h-full w-full" />
      </div>
    </div>
  );
};

export default TerminalView;
