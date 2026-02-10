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

interface CommandHistoryItem {
  id: string;
  command: string;
  createdAt: string;
}

interface CommandHistoryParseState {
  lineBuffer: string;
}

const MAX_COMMAND_HISTORY = 200;

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

const createInitialCommandHistoryParseState = (): CommandHistoryParseState => ({
  lineBuffer: '',
});

const stripTerminalControlSequences = (input: string): string => (
  input
    .replace(/\u001b\][^\u001b\u0007]*(?:\u0007|\u001b\\)/g, '')
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, '')
    .replace(/\u001b[@-Z\\-_]/g, '')
);

const collapseBackspaces = (input: string): string => {
  let out = '';
  for (const ch of input) {
    if (ch === '\u0008' || ch === '\u007f') {
      out = out.slice(0, -1);
      continue;
    }
    if (ch === '\u0000') {
      continue;
    }
    out += ch;
  }
  return out;
};

const looksLikePromptPrefix = (prefixWithMarker: string): boolean => {
  const marker = prefixWithMarker[prefixWithMarker.length - 1];
  if (!marker || !['$', '%', '#', '>'].includes(marker)) {
    return false;
  }

  const prefix = prefixWithMarker.slice(0, -1).trim();
  if (!prefix) {
    return false;
  }

  if (prefix.startsWith('(') || prefix.includes('@')) {
    return true;
  }

  if (prefix.includes(':') || prefix.includes('/') || prefix.includes('\\') || prefix.includes('~')) {
    return true;
  }

  return /^[\w.-]+(?:\s+[\w.-]+)*$/.test(prefix);
};

const extractCommandFromPromptLine = (line: string): string | null => {
  const visible = collapseBackspaces(line.split('\r').pop() ?? '').trimEnd();
  if (!visible) {
    return null;
  }

  const windowsPrompt = visible.match(/^([A-Za-z]:\\.*>)\s*(.+)$/);
  if (windowsPrompt?.[2]) {
    const command = windowsPrompt[2].trim();
    return command || null;
  }

  const unixPromptWithSpace = visible.match(/^(.*\s[#$%>])\s+(.+)$/);
  if (unixPromptWithSpace?.[2] && looksLikePromptPrefix(unixPromptWithSpace[1])) {
    const command = unixPromptWithSpace[2].trim();
    return command || null;
  }

  const unixUserHostPrompt = visible.match(/^([^\r\n]*@[^\r\n]*[#$%>])\s+(.+)$/);
  if (unixUserHostPrompt?.[2]) {
    const command = unixUserHostPrompt[2].trim();
    return command || null;
  }

  return null;
};

const parseOutputChunkForCommands = (
  chunk: string,
  state: CommandHistoryParseState,
): { commands: string[]; nextState: CommandHistoryParseState } => {
  const cleaned = stripTerminalControlSequences(chunk);
  const combined = `${state.lineBuffer}${cleaned}`;
  const lines = combined.split('\n');
  const nextLineBuffer = lines.pop() ?? '';
  const commands: string[] = [];

  for (const rawLine of lines) {
    const command = extractCommandFromPromptLine(rawLine);
    if (command) {
      commands.push(command);
    }
  }

  return {
    commands,
    nextState: {
      lineBuffer: nextLineBuffer,
    },
  };
};

const normalizeLogTimestamp = (value: Date | string | undefined): string => {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'string' && value.trim()) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed.toISOString();
    }
  }
  return new Date().toISOString();
};

const formatCommandTime = (value: string): string => {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return '--:--:--';
  }
  return parsed.toLocaleTimeString('zh-CN', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
};

const writeToTerminal = (term: XTerm, data: string): Promise<void> => (
  new Promise((resolve) => {
    if (!data) {
      resolve();
      return;
    }
    term.write(data, () => resolve());
  })
);

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
  const inputForwardEnabledRef = useRef(false);
  const outputParseStateRef = useRef<CommandHistoryParseState>(createInitialCommandHistoryParseState());
  const commandSeqRef = useRef(0);

  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [historyState, setHistoryState] = useState<HistoryState>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [connectSeq, setConnectSeq] = useState(0);
  const [commandHistory, setCommandHistory] = useState<CommandHistoryItem[]>([]);

  const client = apiClientFromContext ?? apiClient;
  const apiBaseUrl = client.getBaseUrl();

  const terminalTitle = currentTerminal?.name || '终端';
  const terminalCwd = currentTerminal?.cwd || '';
  const terminalStatus = currentTerminal?.status || 'unknown';

  const themeColors = useMemo(() => getThemeColors(actualTheme), [actualTheme]);
  const displayHistory = useMemo(() => [...commandHistory].reverse(), [commandHistory]);

  const appendCommands = (commands: string[], createdAt: string) => {
    if (commands.length === 0) {
      return;
    }

    setCommandHistory((prev) => {
      const next = [...prev];
      for (const command of commands) {
        next.push({
          id: `cmd-${commandSeqRef.current++}`,
          command,
          createdAt,
        });
      }
      return next.slice(-MAX_COMMAND_HISTORY);
    });
  };

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

    let cancelled = false;

    outputParseStateRef.current = createInitialCommandHistoryParseState();
    setCommandHistory([]);
    setHistoryState('loading');
    setConnectionState('disconnected');
    setErrorMessage(null);
    inputForwardEnabledRef.current = false;

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
      if (!inputForwardEnabledRef.current) {
        return;
      }

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
        if (cancelled) {
          return;
        }

        const normalized = Array.isArray(logs) ? logs.map(normalizeTerminalLog) : [];
        const outputLogs = normalized.filter((log: TerminalLog) => log.logType === 'output' || log.logType === 'system');

        const outputContent = outputLogs.map((log: TerminalLog) => log.content).join('');
        await writeToTerminal(term, outputContent);

        if (cancelled) {
          return;
        }

        const parsedCommands: CommandHistoryItem[] = [];
        let parseState = createInitialCommandHistoryParseState();

        for (const log of outputLogs) {
          const parsed = parseOutputChunkForCommands(log.content, parseState);
          parseState = parsed.nextState;
          if (parsed.commands.length === 0) {
            continue;
          }

          const createdAt = normalizeLogTimestamp(log.createdAt);
          for (const command of parsed.commands) {
            parsedCommands.push({
              id: `cmd-${commandSeqRef.current++}`,
              command,
              createdAt,
            });
          }
        }

        outputParseStateRef.current = parseState;
        setCommandHistory(parsedCommands.slice(-MAX_COMMAND_HISTORY));
        setHistoryState('ready');
      } catch (error) {
        if (cancelled) {
          return;
        }
        console.error('Failed to load terminal history:', error);
        setHistoryState('error');
        setErrorMessage(error instanceof Error ? error.message : '加载历史失败');
      } finally {
        if (!cancelled) {
          setConnectSeq((prev) => prev + 1);
        }
      }
    };

    loadHistory();

    return () => {
      cancelled = true;
      inputForwardEnabledRef.current = false;
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
    inputForwardEnabledRef.current = false;

    const ws = new WebSocket(wsUrl);
    socketRef.current = ws;

    ws.onopen = () => {
      if (socketRef.current !== ws) {
        return;
      }
      setConnectionState('connected');
      const term = terminalRef.current;
      if (term) {
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      }
      inputForwardEnabledRef.current = true;
    };

    ws.onmessage = (event) => {
      if (socketRef.current !== ws) {
        return;
      }

      try {
        const payload = JSON.parse(event.data);
        if (payload?.type === 'output') {
          const outputData = payload.data ?? '';
          terminalRef.current?.write(outputData);

          const parsed = parseOutputChunkForCommands(outputData, outputParseStateRef.current);
          outputParseStateRef.current = parsed.nextState;
          appendCommands(parsed.commands, new Date().toISOString());
        } else if (payload?.type === 'exit') {
          inputForwardEnabledRef.current = false;
          setConnectionState('disconnected');
          loadTerminals();
        } else if (payload?.type === 'state') {
          loadTerminals();
        } else if (payload?.type === 'error') {
          setErrorMessage(payload.error || '终端发生错误');
          inputForwardEnabledRef.current = false;
          setConnectionState('error');
        }
      } catch (err) {
        console.warn('terminal ws message parse failed', err);
      }
    };

    ws.onerror = () => {
      if (socketRef.current !== ws) {
        return;
      }
      inputForwardEnabledRef.current = false;
      setConnectionState('error');
    };

    ws.onclose = () => {
      if (socketRef.current !== ws) {
        return;
      }
      inputForwardEnabledRef.current = false;
      setConnectionState('disconnected');
      loadTerminals();
    };

    return () => {
      inputForwardEnabledRef.current = false;
      if (socketRef.current === ws) {
        socketRef.current = null;
      }
      ws.close();
    };
  }, [currentTerminal?.id, historyState, apiBaseUrl, connectSeq, loadTerminals]);

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

      <div className="flex flex-1 overflow-hidden bg-background">
        <div className="min-w-0 flex-1 overflow-hidden">
          <div ref={containerRef} className="h-full w-full" />
        </div>

        <div className="w-80 max-w-[45%] shrink-0 border-l border-border bg-card/40">
          <div className="border-b border-border px-3 py-2">
            <div className="text-sm font-medium text-foreground">历史命令</div>
            <div className="text-xs text-muted-foreground">{commandHistory.length} 条（仅当前终端）</div>
          </div>

          <div className="h-[calc(100%-53px)] overflow-y-auto p-2">
            {displayHistory.length === 0 ? (
              <div className="rounded border border-dashed border-border px-3 py-4 text-xs text-muted-foreground">
                暂无命令，执行后会显示在这里
              </div>
            ) : (
              <div className="space-y-2">
                {displayHistory.map((item) => (
                  <div key={item.id} className="rounded border border-border/60 bg-background/80 px-2 py-1.5">
                    <div className="text-[10px] text-muted-foreground">{formatCommandTime(item.createdAt)}</div>
                    <div className="mt-1 break-all font-mono text-xs text-foreground">{item.command}</div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default TerminalView;
