import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import {
  resolveRemoteConnectionErrorMessage,
  resolveRemoteTerminalWsErrorMessage,
} from '../lib/api/remoteConnectionErrors';
import RemoteVerificationModal from './remote/RemoteVerificationModal';
import { useChatStoreSelector, useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useAuthStore } from '../lib/auth/authStore';
import { useTheme } from '../hooks/useTheme';
import { cn } from '../lib/utils';

interface RemoteTerminalViewProps {
  className?: string;
}

type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';

const closeWebSocketSafely = (socket: WebSocket | null | undefined) => {
  if (!socket) {
    return;
  }
  if (socket.readyState === WebSocket.OPEN) {
    socket.close();
    return;
  }
  if (socket.readyState === WebSocket.CONNECTING) {
    const closeOnOpen = () => {
      try {
        socket.close();
      } catch {
        // ignore
      }
    };
    socket.addEventListener('open', closeOnOpen, { once: true });
  }
};

const buildWsUrl = (
  baseUrl: string,
  path: string,
  accessToken?: string | null,
  verificationCode?: string | null,
) => {
  const cleanedBase = baseUrl.endsWith('/') ? baseUrl.slice(0, -1) : baseUrl;
  const cleanedPath = path.startsWith('/') ? path : `/${path}`;
  const rawUrl = (cleanedBase.startsWith('http://') || cleanedBase.startsWith('https://'))
    ? cleanedBase.replace(/^http/, 'ws') + cleanedPath
    : (() => {
        const { protocol, host } = window.location;
        const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:';
        return `${wsProtocol}//${host}${cleanedBase}${cleanedPath}`;
      })();
  const wsUrl = new URL(rawUrl);
  const token = (accessToken || '').trim();
  if (token) {
    wsUrl.searchParams.set('access_token', token);
  }
  const code = (verificationCode || '').trim();
  if (code) {
    wsUrl.searchParams.set('verification_code', code);
  }
  return wsUrl.toString();
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

const toXtermTheme = (palette: ReturnType<typeof getThemeColors>) => ({
  background: palette.background,
  foreground: palette.foreground,
  cursor: palette.cursor,
  selectionBackground: palette.selection,
  black: palette.black,
  red: palette.red,
  green: palette.green,
  yellow: palette.yellow,
  blue: palette.blue,
  magenta: palette.magenta,
  cyan: palette.cyan,
  white: palette.white,
  brightBlack: palette.brightBlack,
  brightRed: palette.brightRed,
  brightGreen: palette.brightGreen,
  brightYellow: palette.brightYellow,
  brightBlue: palette.brightBlue,
  brightMagenta: palette.brightMagenta,
  brightCyan: palette.brightCyan,
  brightWhite: palette.brightWhite,
});

const RemoteTerminalView: React.FC<RemoteTerminalViewProps> = ({ className }) => {
  const currentRemoteConnection = useChatStoreSelector((state) => state.currentRemoteConnection);
  const openRemoteSftp = useChatStoreSelector((state) => state.openRemoteSftp);
  const apiClientFromContext = useChatApiClientFromContext();
  const client = apiClientFromContext || globalApiClient;
  const { accessToken } = useAuthStore();
  const { actualTheme } = useTheme();

  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const lastSnapshotRef = useRef('');
  const lastConnectionIdRef = useRef<string | null>(null);
  const paletteRef = useRef(getThemeColors(actualTheme));
  const skippedStrictModeProbeRef = useRef(false);
  const pendingVerificationCodeRef = useRef<string | null>(null);

  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [connectSeq, setConnectSeq] = useState(0);
  const [busy, setBusy] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [verificationOpen, setVerificationOpen] = useState(false);
  const [verificationPrompt, setVerificationPrompt] = useState<string>('');
  const [verificationCode, setVerificationCode] = useState('');

  const apiBaseUrl = client.getBaseUrl();
  const palette = useMemo(() => getThemeColors(actualTheme), [actualTheme]);

  const handleDisconnect = useCallback(async () => {
    if (!currentRemoteConnection?.id || disconnecting) {
      return;
    }
    setDisconnecting(true);
    setErrorMessage(null);
    try {
      await client.disconnectRemoteTerminal(currentRemoteConnection.id);
      const active = socketRef.current;
      if (active) {
        socketRef.current = null;
        closeWebSocketSafely(active);
      }
      setConnectionState('disconnected');
      setBusy(false);
    } catch (error) {
      setErrorMessage(resolveRemoteConnectionErrorMessage(error, '断开连接失败'));
    } finally {
      setDisconnecting(false);
    }
  }, [client, currentRemoteConnection?.id, disconnecting]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const term = new XTerm({
      cursorBlink: true,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
      fontSize: 13,
      lineHeight: 1.25,
      scrollback: 5000,
      convertEol: false,
      theme: toXtermTheme(paletteRef.current),
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);
    fitAddon.fit();

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    const resizeObserver = new ResizeObserver(() => {
      if (!fitAddonRef.current || !terminalRef.current) return;
      fitAddonRef.current.fit();
      const active = socketRef.current;
      if (active && active.readyState === WebSocket.OPEN) {
        active.send(JSON.stringify({
          type: 'resize',
          cols: terminalRef.current.cols,
          rows: terminalRef.current.rows,
        }));
      }
    });
    resizeObserver.observe(container);

    const disposableInput = term.onData((data) => {
      const active = socketRef.current;
      if (active && active.readyState === WebSocket.OPEN) {
        active.send(JSON.stringify({ type: 'input', data }));
      }
    });

    return () => {
      disposableInput.dispose();
      resizeObserver.disconnect();
      terminalRef.current = null;
      fitAddonRef.current = null;
      term.dispose();
    };
  }, []);

  useEffect(() => {
    paletteRef.current = palette;
    const term = terminalRef.current;
    if (term) {
      term.options.theme = toXtermTheme(palette);
    }
  }, [palette]);

  useEffect(() => {
    if (!currentRemoteConnection?.id) {
      setConnectionState('disconnected');
      setErrorMessage(null);
      setBusy(false);
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCode('');
      pendingVerificationCodeRef.current = null;
      lastConnectionIdRef.current = null;
      lastSnapshotRef.current = '';
      return;
    }

    const term = terminalRef.current;
    if (!term) return;

    if (
      import.meta.env.DEV
      && !skippedStrictModeProbeRef.current
      && !pendingVerificationCodeRef.current
      && connectSeq === 0
    ) {
      skippedStrictModeProbeRef.current = true;
      return;
    }

    const connectionChanged = lastConnectionIdRef.current !== currentRemoteConnection.id;
    if (connectionChanged) {
      term.reset();
      lastSnapshotRef.current = '';
      lastConnectionIdRef.current = currentRemoteConnection.id;
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCode('');
      pendingVerificationCodeRef.current = null;
    }

    const verificationCodeForAttempt = pendingVerificationCodeRef.current;
    const wsUrl = buildWsUrl(
      apiBaseUrl,
      `/remote-connections/${currentRemoteConnection.id}/ws`,
      accessToken,
      verificationCodeForAttempt,
    );
    const ws = new WebSocket(wsUrl);
    socketRef.current = ws;
    setConnectionState('connecting');
    setErrorMessage(null);

    const clearVerificationCodeForAttempt = () => {
      if (
        verificationCodeForAttempt
        && pendingVerificationCodeRef.current === verificationCodeForAttempt
      ) {
        pendingVerificationCodeRef.current = null;
        setVerificationCode('');
      }
    };

    ws.onopen = () => {
      if (socketRef.current !== ws) return;
      setConnectionState('connected');
      if (terminalRef.current) {
        ws.send(JSON.stringify({
          type: 'resize',
          cols: terminalRef.current.cols,
          rows: terminalRef.current.rows,
        }));
      }
      term.focus();
    };

    ws.onmessage = (event) => {
      if (socketRef.current !== ws) return;
      try {
        const payload = JSON.parse(event.data as string);
        if (payload?.type === 'output' && typeof payload.data === 'string') {
          clearVerificationCodeForAttempt();
          terminalRef.current?.write(payload.data);
          return;
        }
        if (payload?.type === 'snapshot' && typeof payload.data === 'string') {
          clearVerificationCodeForAttempt();
          if (payload.data === lastSnapshotRef.current) {
            return;
          }
          lastSnapshotRef.current = payload.data;
          terminalRef.current?.reset();
          terminalRef.current?.write(payload.data);
          return;
        }
        if (payload?.type === 'state') {
          clearVerificationCodeForAttempt();
          setBusy(Boolean(payload.busy));
          return;
        }
        if (payload?.type === 'error') {
          if (payload?.code === 'second_factor_required') {
            pendingVerificationCodeRef.current = null;
            setVerificationPrompt(
              typeof payload?.challenge_prompt === 'string' ? payload.challenge_prompt : '',
            );
            setVerificationOpen(true);
            setConnectionState('error');
            setBusy(false);
            setErrorMessage(
              verificationCodeForAttempt
                ? '验证码未通过或已过期，请重新输入后继续连接'
                : '需要验证码，请输入后继续连接',
            );
            return;
          }
          setErrorMessage(resolveRemoteTerminalWsErrorMessage(payload, '远端终端错误'));
          return;
        }
      } catch {
        // ignore parse errors to keep terminal usable
      }
    };

    ws.onerror = () => {
      if (socketRef.current !== ws) return;
      setConnectionState('error');
      setBusy(false);
      setErrorMessage('远端终端连接异常，请重试');
    };

    ws.onclose = () => {
      if (socketRef.current !== ws) return;
      setConnectionState('disconnected');
      setBusy(false);
    };

    return () => {
      if (socketRef.current === ws) {
        socketRef.current = null;
      }
      closeWebSocketSafely(ws);
    };
  }, [currentRemoteConnection?.id, apiBaseUrl, accessToken, connectSeq]);

  if (!currentRemoteConnection) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        请选择一个远端连接
      </div>
    );
  }

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <div className="flex items-center justify-between border-b border-border px-4 py-2 gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground truncate">{currentRemoteConnection.name}</div>
          <div className="text-xs text-muted-foreground truncate">
            {currentRemoteConnection.username}@{currentRemoteConnection.host}:{currentRemoteConnection.port}
          </div>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground shrink-0">
          <span className={cn(
            'inline-flex items-center gap-1',
            connectionState === 'connected' ? 'text-emerald-500' : connectionState === 'error' ? 'text-destructive' : 'text-muted-foreground',
          )}>
            <span className={cn(
              'inline-block h-2 w-2 rounded-full',
              connectionState === 'connected' ? 'bg-emerald-500' : connectionState === 'error' ? 'bg-destructive' : 'bg-muted-foreground/50',
            )} />
            {connectionState === 'connected' ? '已连接' : connectionState === 'connecting' ? '连接中' : connectionState === 'error' ? '连接错误' : '未连接'}
          </span>
          <span>{busy ? '忙碌' : '空闲'}</span>
          <button
            type="button"
            onClick={() => setConnectSeq((prev) => prev + 1)}
            disabled={disconnecting}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
          >
            重连
          </button>
          <button
            type="button"
            onClick={() => void handleDisconnect()}
            disabled={disconnecting || connectionState === 'disconnected'}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50"
          >
            {disconnecting ? '断开中...' : '断开'}
          </button>
          <button
            type="button"
            onClick={() => void openRemoteSftp(currentRemoteConnection.id)}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
          >
            SFTP
          </button>
        </div>
      </div>

      {errorMessage && (
        <div className="px-4 py-2 text-xs text-destructive">{errorMessage}</div>
      )}

      <div className="flex-1 overflow-hidden bg-background">
        <div ref={containerRef} className="h-full w-full" />
      </div>

      <RemoteVerificationModal
        isOpen={verificationOpen}
        prompt={verificationPrompt}
        code={verificationCode}
        submitting={connectionState === 'connecting'}
        onCodeChange={setVerificationCode}
        onClose={() => {
          if (connectionState === 'connecting') {
            return;
          }
          setVerificationOpen(false);
        }}
        onSubmit={() => {
          const trimmed = verificationCode.trim();
          if (!trimmed) {
            return;
          }
          setVerificationOpen(false);
          const active = socketRef.current;
          if (active && active.readyState === WebSocket.OPEN) {
            active.send(JSON.stringify({ type: 'verification', code: trimmed }));
            setVerificationCode('');
            setConnectionState('connecting');
            setErrorMessage(null);
            return;
          }
          pendingVerificationCodeRef.current = trimmed;
          setConnectSeq((prev) => prev + 1);
        }}
      />
    </div>
  );
};

export default RemoteTerminalView;
