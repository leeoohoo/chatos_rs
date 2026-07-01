// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { useI18n } from '../i18n/I18nProvider';
import {
  resolveRemoteConnectionErrorMessage,
  resolveRemoteTerminalWsErrorMessage,
} from '../lib/api/remoteConnectionErrors';
import { useApiClient } from '../lib/api/ApiClientContext';
import RemoteVerificationModal from './remote/RemoteVerificationModal';
import { useChatStoreSelector } from '../lib/store/ChatStoreContext';
import { useAuthStoreSelector } from '../lib/auth/authStore';
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
  const { t } = useI18n();
  const currentRemoteConnection = useChatStoreSelector((state) => state.currentRemoteConnection);
  const openRemoteSftp = useChatStoreSelector((state) => state.openRemoteSftp);
  const client = useApiClient();
  const accessToken = useAuthStoreSelector((state) => state.accessToken);
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
  const terminalErrorHandledRef = useRef(false);

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
      setErrorMessage(resolveRemoteConnectionErrorMessage(error, t('remote.terminal.disconnectFailed')));
    } finally {
      setDisconnecting(false);
    }
  }, [client, currentRemoteConnection?.id, disconnecting, t]);

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
      terminalErrorHandledRef.current = false;
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
      terminalErrorHandledRef.current = false;
    }

    const verificationCodeForAttempt = pendingVerificationCodeRef.current;
    terminalErrorHandledRef.current = false;
    setConnectionState('connecting');
    setErrorMessage(null);
    setBusy(false);

    let disposed = false;
    let ws: WebSocket | null = null;

    const clearVerificationCodeForAttempt = () => {
      if (
        verificationCodeForAttempt
        && pendingVerificationCodeRef.current === verificationCodeForAttempt
      ) {
        pendingVerificationCodeRef.current = null;
        setVerificationCode('');
      }
    };

    void (async () => {
      try {
        const webSocketTicket = await client.issueWebSocketTicket();
        if (disposed) {
          return;
        }
        const wsUrl = buildWsUrl(
          apiBaseUrl,
          `/remote-connections/${currentRemoteConnection.id}/ws`,
          webSocketTicket,
        );
        const socket = new WebSocket(wsUrl);
        ws = socket;
        socketRef.current = socket;

        socket.onopen = () => {
          if (socketRef.current !== socket) return;
          setConnectionState('connected');
          if (verificationCodeForAttempt) {
            socket.send(JSON.stringify({ type: 'verification', code: verificationCodeForAttempt }));
          }
          if (terminalRef.current) {
            socket.send(JSON.stringify({
              type: 'resize',
              cols: terminalRef.current.cols,
              rows: terminalRef.current.rows,
            }));
          }
          term.focus();
        };

        socket.onmessage = (event) => {
          if (socketRef.current !== socket) return;
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
              terminalErrorHandledRef.current = true;
              setConnectionState('error');
              setBusy(false);
              if (payload?.code === 'second_factor_required') {
                pendingVerificationCodeRef.current = null;
                setVerificationPrompt(
                  typeof payload?.challenge_prompt === 'string' ? payload.challenge_prompt : '',
                );
                setVerificationOpen(true);
                setErrorMessage(
                  verificationCodeForAttempt
                    ? t('remote.common.needsVerificationRetry')
                    : t('remote.common.needsVerification'),
                );
                return;
              }
              setErrorMessage(resolveRemoteTerminalWsErrorMessage(payload, t('remote.terminal.wsError')));
              return;
            }
          } catch {
            // ignore parse errors to keep terminal usable
          }
        };

        socket.onerror = () => {
          if (socketRef.current !== socket) return;
          if (terminalErrorHandledRef.current) {
            return;
          }
          setConnectionState('error');
          setBusy(false);
          setErrorMessage(t('remote.terminal.connectionError'));
        };

        socket.onclose = () => {
          if (socketRef.current !== socket) return;
          setConnectionState((prev) => (terminalErrorHandledRef.current ? prev : 'disconnected'));
          setBusy(false);
        };
      } catch (error) {
        if (disposed) {
          return;
        }
        terminalErrorHandledRef.current = true;
        setConnectionState('error');
        setBusy(false);
        setErrorMessage(resolveRemoteConnectionErrorMessage(error, t('remote.terminal.connectionError')));
      }
    })();

    return () => {
      disposed = true;
      if (socketRef.current === ws) {
        socketRef.current = null;
      }
      closeWebSocketSafely(ws);
    };
  }, [currentRemoteConnection?.id, apiBaseUrl, accessToken, connectSeq, t]);

  if (!currentRemoteConnection) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        {t('remote.common.chooseConnection')}
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
