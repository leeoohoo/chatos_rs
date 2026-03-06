import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { useChatStoreFromContext, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { apiClient } from '../lib/api/client';
import { useAuthStore } from '../lib/auth/authStore';
import { normalizeTerminalLog } from '../lib/store/helpers/terminals';
import { useTheme } from '../hooks/useTheme';
import { cn } from '../lib/utils';
import type { TerminalLog } from '../types';
import {
  MAX_COMMAND_HISTORY,
  canCommandBeUsed,
  canOutputCommandCorrectInput,
  createInitialCommandHistoryParseState,
  createInitialInputCommandParseState,
  extractCommandFromTerminalBuffer,
  formatCommandTime,
  mergeCommandHistory,
  normalizeCommandForCompare,
  normalizeLogTimestamp,
  parseInputChunkForCommands,
  parseOutputChunkForCommands,
  writeToTerminal,
  writeToTerminalInChunks,
  type CommandHistoryItem,
  type CommandHistoryParseState,
  type InputCommandParseState,
} from './terminal/commandHistory';
import { buildWsUrl, getThemeColors } from './terminal/themeTransport';

interface TerminalViewProps {
  className?: string;
}

type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
type HistoryState = 'idle' | 'loading' | 'ready' | 'error';
const TERMINAL_HISTORY_PAGE_SIZE = 800;
const TERMINAL_HISTORY_MAX_LIMIT = 5000;

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
  const inputParseStateRef = useRef<InputCommandParseState>(createInitialInputCommandParseState());
  const outputParseStateRef = useRef<CommandHistoryParseState>(createInitialCommandHistoryParseState());
  const commandSeqRef = useRef(0);
  const historyLoadSeqRef = useRef(0);
  const replayingHistoryRef = useRef(false);
  const pendingOutputChunksRef = useRef<string[]>([]);
  const loadHistoryRef = useRef<((limit: number, mode: 'initial' | 'more') => Promise<void>) | null>(null);
  const themeColorsRef = useRef(getThemeColors(actualTheme));
  const commandHistoryCacheRef = useRef<Record<string, CommandHistoryItem[]>>({});

  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [historyState, setHistoryState] = useState<HistoryState>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [connectSeq, setConnectSeq] = useState(0);
  const [commandHistory, setCommandHistory] = useState<CommandHistoryItem[]>([]);
  const [historyLogLimit, setHistoryLogLimit] = useState(TERMINAL_HISTORY_PAGE_SIZE);
  const [canLoadMoreHistory, setCanLoadMoreHistory] = useState(false);
  const [historyBusy, setHistoryBusy] = useState(false);

  const client = apiClientFromContext ?? apiClient;
  const apiBaseUrl = client.getBaseUrl();
  const accessToken = useAuthStore((state) => state.accessToken);

  const terminalTitle = currentTerminal?.name || '终端';
  const terminalCwd = currentTerminal?.cwd || '';
  const terminalStatus = currentTerminal?.status || 'unknown';

  const themeColors = useMemo(() => getThemeColors(actualTheme), [actualTheme]);
  const displayHistory = useMemo(() => [...commandHistory].reverse(), [commandHistory]);

  const appendCommands = (
    commands: string[],
    createdAt: string,
    mode: 'append' | 'correct' = 'append',
  ) => {
    if (commands.length === 0) {
      return;
    }

    setCommandHistory((prev) => {
      const activeTerminalId = currentTerminal?.id;
      const next = [...prev];
      const normalizedCommands = commands
        .map((command) => normalizeCommandForCompare(command))
        .filter((command) => canCommandBeUsed(command));

      if (normalizedCommands.length === 0) {
        return next;
      }

      if (mode === 'append') {
        for (const command of normalizedCommands) {
          const last = next[next.length - 1];
          if (last && normalizeCommandForCompare(last.command) === command) {
            continue;
          }

          next.push({
            id: `cmd-${commandSeqRef.current++}`,
            command,
            createdAt,
          });
        }

        const finalHistory = next.slice(-MAX_COMMAND_HISTORY);
        if (activeTerminalId) {
          commandHistoryCacheRef.current[activeTerminalId] = finalHistory;
        }
        return finalHistory;
      }

      if (next.length === 0) {
        return next;
      }

      const windowStart = Math.max(0, next.length - 6);
      for (const outputCommand of normalizedCommands) {
        for (let i = next.length - 1; i >= windowStart; i -= 1) {
          const existing = next[i];
          const existingNormalized = normalizeCommandForCompare(existing.command);

          if (existingNormalized === outputCommand) {
            break;
          }

          if (canOutputCommandCorrectInput(existingNormalized, outputCommand) && outputCommand.length > existingNormalized.length) {
            next[i] = {
              ...existing,
              command: outputCommand,
              createdAt,
            };
            break;
          }
        }
      }

      const finalHistory = next.slice(-MAX_COMMAND_HISTORY);
      if (activeTerminalId) {
        commandHistoryCacheRef.current[activeTerminalId] = finalHistory;
      }
      return finalHistory;
    });
  };

  useEffect(() => {
    themeColorsRef.current = themeColors;
    const term = terminalRef.current;
    if (term) {
      term.options.theme = themeColors;
    }
  }, [themeColors]);

  useEffect(() => {
    if (!currentTerminal || !containerRef.current) {
      loadHistoryRef.current = null;
      return;
    }

    let cancelled = false;

    inputParseStateRef.current = createInitialInputCommandParseState();
    outputParseStateRef.current = createInitialCommandHistoryParseState();
    const cachedHistory = commandHistoryCacheRef.current[currentTerminal.id] ?? [];
    setCommandHistory(cachedHistory);
    pendingOutputChunksRef.current = [];
    replayingHistoryRef.current = false;
    setHistoryLogLimit(TERMINAL_HISTORY_PAGE_SIZE);
    setCanLoadMoreHistory(false);
    setHistoryBusy(false);
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
      theme: themeColorsRef.current,
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

      const submittedCommand = (data.includes('\r') || data.includes('\n'))
        ? extractCommandFromTerminalBuffer(term)
        : null;

      const parsedInput = parseInputChunkForCommands(data, inputParseStateRef.current);
      inputParseStateRef.current = parsedInput.nextState;
      appendCommands(parsedInput.commands, new Date().toISOString(), 'append');

      const normalizedSubmittedCommand = submittedCommand
        ? normalizeCommandForCompare(submittedCommand)
        : '';
      if (canCommandBeUsed(normalizedSubmittedCommand)) {
        appendCommands([normalizedSubmittedCommand], new Date().toISOString(), 'correct');
      }

      const ws = socketRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        if (canCommandBeUsed(normalizedSubmittedCommand)) {
          ws.send(JSON.stringify({ type: 'command', command: normalizedSubmittedCommand }));
        }
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

    const loadHistory = async (
      limit: number,
      mode: 'initial' | 'more',
    ) => {
      const requestSeq = historyLoadSeqRef.current + 1;
      historyLoadSeqRef.current = requestSeq;
      const isCurrentRequest = () => requestSeq === historyLoadSeqRef.current;

      if (mode === 'more') {
        setHistoryBusy(true);
      } else {
        setHistoryState('loading');
      }
      setErrorMessage(null);
      inputForwardEnabledRef.current = false;

      try {
        const logs = await client.listTerminalLogs(currentTerminal.id, { limit, offset: 0 });
        if (cancelled || !isCurrentRequest() || terminalRef.current !== term) {
          return;
        }

        const normalized = Array.isArray(logs) ? logs.map(normalizeTerminalLog) : [];
        setCanLoadMoreHistory(
          normalized.length >= limit && limit < TERMINAL_HISTORY_MAX_LIMIT
        );

        const outputLogs: TerminalLog[] = [];
        const commandLogs: TerminalLog[] = [];
        const inputLogs: TerminalLog[] = [];
        for (const log of normalized) {
          if (log.logType === 'command') {
            commandLogs.push(log);
          } else if (log.logType === 'input') {
            inputLogs.push(log);
          } else if (log.logType === 'output' || log.logType === 'system') {
            outputLogs.push(log);
          }
        }

        const outputContent = outputLogs.map((log: TerminalLog) => log.content).join('');
        replayingHistoryRef.current = true;
        pendingOutputChunksRef.current = [];
        term.reset();
        await writeToTerminalInChunks(term, outputContent);

        if (cancelled || !isCurrentRequest() || terminalRef.current !== term) {
          return;
        }

        const parsedCommands: CommandHistoryItem[] = [];

        if (commandLogs.length > 0) {
          for (const log of commandLogs) {
            const normalizedCommand = normalizeCommandForCompare(log.content);
            if (!canCommandBeUsed(normalizedCommand)) {
              continue;
            }
            parsedCommands.push({
              id: `cmd-${commandSeqRef.current++}`,
              command: normalizedCommand,
              createdAt: normalizeLogTimestamp(log.createdAt),
            });
          }
        }

        let inputState = createInitialInputCommandParseState();

        if (parsedCommands.length === 0) {
          for (const log of inputLogs) {
            const parsed = parseInputChunkForCommands(log.content, inputState);
            inputState = parsed.nextState;
            if (parsed.commands.length === 0) {
              continue;
            }

            const createdAt = normalizeLogTimestamp(log.createdAt);
            for (const command of parsed.commands) {
              const normalizedCommand = normalizeCommandForCompare(command);
              if (!canCommandBeUsed(normalizedCommand)) {
                continue;
              }

              parsedCommands.push({
                id: `cmd-${commandSeqRef.current++}`,
                command: normalizedCommand,
                createdAt,
              });
            }
          }
        }

        const outputDerivedCommands: CommandHistoryItem[] = [];
        let parseState = createInitialCommandHistoryParseState();

        for (const log of outputLogs) {
          const parsed = parseOutputChunkForCommands(log.content, parseState);
          parseState = parsed.nextState;

          if (commandLogs.length > 0 || parsed.commands.length === 0) {
            continue;
          }

          const createdAt = normalizeLogTimestamp(log.createdAt);
          for (const command of parsed.commands) {
            const normalizedOutputCommand = normalizeCommandForCompare(command);
            if (!canCommandBeUsed(normalizedOutputCommand)) {
              continue;
            }

            outputDerivedCommands.push({
              id: `cmd-${commandSeqRef.current++}`,
              command: normalizedOutputCommand,
              createdAt,
            });

            if (parsedCommands.length === 0) {
              continue;
            }

            const searchStart = Math.max(0, parsedCommands.length - 10);
            for (let i = parsedCommands.length - 1; i >= searchStart; i -= 1) {
              const baseCommand = parsedCommands[i].command;
              if (canOutputCommandCorrectInput(baseCommand, normalizedOutputCommand)
                && normalizedOutputCommand.length > normalizeCommandForCompare(baseCommand).length
              ) {
                parsedCommands[i] = {
                  ...parsedCommands[i],
                  command: normalizedOutputCommand,
                  createdAt,
                };
                break;
              }
            }
          }
        }

        if (commandLogs.length === 0 && parsedCommands.length === 0 && outputDerivedCommands.length > 0) {
          parsedCommands.push(...outputDerivedCommands);
        }

        inputParseStateRef.current = createInitialInputCommandParseState();
        outputParseStateRef.current = parseState;
        const cachedHistory = commandHistoryCacheRef.current[currentTerminal.id] ?? [];
        const mergedHistory = mergeCommandHistory(parsedCommands, cachedHistory);
        setCommandHistory(mergedHistory);
        commandHistoryCacheRef.current[currentTerminal.id] = mergedHistory;

        const pendingChunks = pendingOutputChunksRef.current;
        pendingOutputChunksRef.current = [];
        if (pendingChunks.length > 0) {
          for (const chunk of pendingChunks) {
            await writeToTerminal(term, chunk);
            const parsed = parseOutputChunkForCommands(chunk, outputParseStateRef.current);
            outputParseStateRef.current = parsed.nextState;
            appendCommands(parsed.commands, new Date().toISOString(), 'correct');
          }
        }

        replayingHistoryRef.current = false;
        setHistoryLogLimit(limit);
        setHistoryState('ready');
      } catch (error) {
        if (cancelled || !isCurrentRequest()) {
          return;
        }
        console.error('Failed to load terminal history:', error);
        if (mode === 'initial') {
          setHistoryState('error');
          setCanLoadMoreHistory(false);
        }
        setErrorMessage(error instanceof Error ? error.message : '加载历史失败');
      } finally {
        if (cancelled || !isCurrentRequest()) {
          return;
        }
        replayingHistoryRef.current = false;
        pendingOutputChunksRef.current = [];
        setHistoryBusy(false);
        inputForwardEnabledRef.current = socketRef.current?.readyState === WebSocket.OPEN;
      }
    };

    loadHistoryRef.current = loadHistory;
    void loadHistory(TERMINAL_HISTORY_PAGE_SIZE, 'initial');

    return () => {
      cancelled = true;
      historyLoadSeqRef.current += 1;
      inputForwardEnabledRef.current = false;
      loadHistoryRef.current = null;
      replayingHistoryRef.current = false;
      pendingOutputChunksRef.current = [];
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
    if (historyState !== 'ready' && historyState !== 'error') return;

    const wsUrl = buildWsUrl(apiBaseUrl, `/terminals/${currentTerminal.id}/ws`, accessToken);
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
          if (replayingHistoryRef.current) {
            pendingOutputChunksRef.current.push(outputData);
            return;
          }
          terminalRef.current?.write(outputData);

          const parsed = parseOutputChunkForCommands(outputData, outputParseStateRef.current);
          outputParseStateRef.current = parsed.nextState;
          appendCommands(parsed.commands, new Date().toISOString(), 'correct');
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
  }, [currentTerminal?.id, historyState, apiBaseUrl, accessToken, connectSeq, loadTerminals]);

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
            disabled={historyState === 'loading' || historyBusy || !canLoadMoreHistory}
            onClick={() => {
              if (!currentTerminal?.id) {
                return;
              }
              const nextLimit = Math.min(historyLogLimit + TERMINAL_HISTORY_PAGE_SIZE, TERMINAL_HISTORY_MAX_LIMIT);
              if (nextLimit <= historyLogLimit) {
                return;
              }
              void loadHistoryRef.current?.(nextLimit, 'more');
            }}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {historyBusy ? '加载中...' : 'Load More History'}
          </button>
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
