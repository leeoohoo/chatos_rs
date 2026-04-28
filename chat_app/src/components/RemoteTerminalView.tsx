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
import { RemoteTerminalHeader } from './remoteTerminal/RemoteTerminalHeader';
import {
  getThemeColors,
  toXtermTheme,
} from './remoteTerminal/terminalTheme';
import type { ConnectionState } from './remoteTerminal/types';
import {
  buildWsUrl,
  closeWebSocketSafely,
} from './remoteTerminal/websocket';

interface RemoteTerminalViewProps {
  className?: string;
}

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
      <RemoteTerminalHeader
        connection={currentRemoteConnection}
        connectionState={connectionState}
        busy={busy}
        disconnecting={disconnecting}
        onReconnect={() => setConnectSeq((prev) => prev + 1)}
        onDisconnect={() => void handleDisconnect()}
        onOpenSftp={() => void openRemoteSftp(currentRemoteConnection.id)}
      />

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
