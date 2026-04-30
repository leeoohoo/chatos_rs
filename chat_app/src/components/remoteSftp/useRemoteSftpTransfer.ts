import { useCallback, useEffect, useRef, useState } from 'react';

import { resolveRemoteSftpErrorMessage } from '../../lib/api/remoteConnectionErrors';
import { useRealtimeConnectionState } from '../../lib/realtime/RealtimeProvider';
import { useRemoteSftpTransferRealtime } from '../../lib/realtime/useRemoteSftpTransferRealtime';

import { normalizeTransferStatus, type RemoteSftpClient } from './helpers';
import type { SftpTransferRequest, SftpTransferStatus } from './types';

const SECOND_FACTOR_REQUIRED_ERROR_PREFIX = '__CHATOS_SECOND_FACTOR_REQUIRED__:';
const REMOTE_SFTP_REFRESH_DELAY_MS = 500;

const extractSecondFactorPrompt = (rawError?: string | null): string => {
  if (typeof rawError !== 'string') {
    return '';
  }
  if (!rawError.startsWith(SECOND_FACTOR_REQUIRED_ERROR_PREFIX)) {
    return '';
  }
  return rawError.slice(SECOND_FACTOR_REQUIRED_ERROR_PREFIX.length).trim();
};

interface UseRemoteSftpTransferOptions {
  client: RemoteSftpClient;
  currentRemoteConnectionId: string | null;
  loadLocal: (path?: string | null) => Promise<void>;
  loadRemote: (path?: string) => Promise<void>;
  remotePathRef: React.MutableRefObject<string>;
  localPathRef: React.MutableRefObject<string | null>;
  setMessage: (message: string | null) => void;
  setError: (message: string | null) => void;
  getVerificationCode: () => string | null;
  onSecondFactorRequired: (
    error: unknown,
    retryWithCode: (code: string) => Promise<void>,
  ) => boolean;
}

interface ActiveTransferContext {
  direction: 'upload' | 'download';
  localSource: string;
  remoteSource: string;
  fallbackSuccess: string;
}

const isDocumentVisible = (): boolean => (
  typeof document === 'undefined' || document.visibilityState === 'visible'
);

export const useRemoteSftpTransfer = ({
  client,
  currentRemoteConnectionId,
  loadLocal,
  loadRemote,
  remotePathRef,
  localPathRef,
  setMessage,
  setError,
  getVerificationCode,
  onSecondFactorRequired,
}: UseRemoteSftpTransferOptions) => {
  const connectionState = useRealtimeConnectionState();
  const [transfering, setTransfering] = useState(false);
  const [transferStatus, setTransferStatus] = useState<SftpTransferStatus | null>(null);
  const [queuedTransfers, setQueuedTransfers] = useState<SftpTransferRequest[]>([]);

  const transferRefreshTimerRef = useRef<number | null>(null);
  const transferStatusRefreshBusyRef = useRef(false);
  const transferQueueSeqRef = useRef(0);
  const activeTransferContextRef = useRef<ActiveTransferContext | null>(null);
  const transferStatusRef = useRef<SftpTransferStatus | null>(null);

  useEffect(() => {
    transferStatusRef.current = transferStatus;
  }, [transferStatus]);

  const clearTransferRefreshTimer = useCallback(() => {
    if (transferRefreshTimerRef.current !== null) {
      window.clearTimeout(transferRefreshTimerRef.current);
      transferRefreshTimerRef.current = null;
    }
  }, []);

  const resetTransferState = useCallback(() => {
    clearTransferRefreshTimer();
    setTransferStatus(null);
    setTransfering(false);
    setQueuedTransfers([]);
    activeTransferContextRef.current = null;
    transferStatusRef.current = null;
  }, [clearTransferRefreshTimer]);

  const finalizeTransferSuccess = useCallback(async (
    latest: SftpTransferStatus,
    fallbackSuccess: string,
  ) => {
    clearTransferRefreshTimer();
    setTransfering(false);
    activeTransferContextRef.current = null;
    transferStatusRef.current = latest;
    setMessage(latest.message || fallbackSuccess);
    await loadRemote(remotePathRef.current);
    if (localPathRef.current !== null) {
      await loadLocal(localPathRef.current);
    } else {
      await loadLocal(null);
    }
  }, [clearTransferRefreshTimer, loadLocal, loadRemote, localPathRef, remotePathRef, setMessage]);

  const finalizeTransferCancelled = useCallback((latest: SftpTransferStatus) => {
    clearTransferRefreshTimer();
    setTransfering(false);
    activeTransferContextRef.current = null;
    transferStatusRef.current = latest;
    setMessage(latest.message || '传输已取消');
  }, [clearTransferRefreshTimer, setMessage]);

  const finalizeTransferError = useCallback((
    latest: SftpTransferStatus,
    context: ActiveTransferContext | null,
  ) => {
    clearTransferRefreshTimer();
    setTransfering(false);
    activeTransferContextRef.current = null;
    transferStatusRef.current = latest;
    const secondFactorPrompt = extractSecondFactorPrompt(latest.error);
    if (secondFactorPrompt && context) {
      const handled = onSecondFactorRequired(
        {
          code: 'second_factor_required',
          message: '需要二次验证',
          payload: { challenge_prompt: secondFactorPrompt },
        },
        async (code) => {
          await startTransfer(
            context.direction,
            context.localSource,
            context.remoteSource,
            context.fallbackSuccess,
            code,
          );
        },
      );
      if (handled) {
        return;
      }
    }
    setError(latest.error || '传输失败');
  }, [clearTransferRefreshTimer, onSecondFactorRequired, setError]);

  const refreshTransferStatusOnce = useCallback(async () => {
    const transferId = transferStatusRef.current?.id || '';
    const context = activeTransferContextRef.current;
    if (!currentRemoteConnectionId || !transferId) {
      return;
    }
    if (transferStatusRefreshBusyRef.current) {
      return;
    }
    transferStatusRefreshBusyRef.current = true;
    try {
      const latest = normalizeTransferStatus(
        await client.getRemoteSftpTransferStatus(currentRemoteConnectionId, transferId),
      );
      transferStatusRef.current = latest;
      setTransferStatus(latest);
      if (latest.state === 'success') {
        await finalizeTransferSuccess(
          latest,
          context?.fallbackSuccess || '传输完成',
        );
        return;
      }
      if (latest.state === 'cancelled') {
        finalizeTransferCancelled(latest);
        return;
      }
      if (latest.state === 'error') {
        finalizeTransferError(latest, context);
      }
    } catch (error) {
      console.error('Failed to refresh remote sftp transfer status:', error);
    } finally {
      transferStatusRefreshBusyRef.current = false;
    }
  }, [
    client,
    currentRemoteConnectionId,
    finalizeTransferCancelled,
    finalizeTransferError,
    finalizeTransferSuccess,
  ]);

  const scheduleTransferStatusRefresh = useCallback((delayMs = REMOTE_SFTP_REFRESH_DELAY_MS) => {
    if (connectionState === 'connected' || !transfering || !currentRemoteConnectionId || !transferStatusRef.current?.id) {
      return;
    }
    if (transferRefreshTimerRef.current !== null) {
      return;
    }
    transferRefreshTimerRef.current = window.setTimeout(() => {
      transferRefreshTimerRef.current = null;
      void refreshTransferStatusOnce();
    }, delayMs);
  }, [connectionState, currentRemoteConnectionId, refreshTransferStatusOnce, transfering]);

  const startTransfer = useCallback(async (
    direction: 'upload' | 'download',
    localSource: string,
    remoteSource: string,
    fallbackSuccess: string,
    verificationCodeOverride?: string,
  ) => {
    if (!currentRemoteConnectionId) return;

    clearTransferRefreshTimer();
    setTransfering(true);
    setTransferStatus(null);
    transferStatusRef.current = null;
    activeTransferContextRef.current = {
      direction,
      localSource,
      remoteSource,
      fallbackSuccess,
    };
    setError(null);
    setMessage(null);

    try {
      const verificationCode = (verificationCodeOverride ?? getVerificationCode() ?? '').trim();
      const started = normalizeTransferStatus(
        await client.startRemoteSftpTransfer(currentRemoteConnectionId, {
          direction,
          local_path: localSource,
          remote_path: remoteSource,
        }, verificationCode || undefined),
      );
      setTransferStatus(started);
      transferStatusRef.current = started;

      if (connectionState === 'connected') {
        return;
      }
      scheduleTransferStatusRefresh();
    } catch (error) {
      setTransfering(false);
      setTransferStatus(null);
      transferStatusRef.current = null;
      if ((verificationCodeOverride || '').trim()) {
        throw error;
      }
      if (onSecondFactorRequired(error, async (code) => {
        await startTransfer(direction, localSource, remoteSource, fallbackSuccess, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(error, '启动传输失败'));
    }
  }, [
    client,
    currentRemoteConnectionId,
    getVerificationCode,
    loadLocal,
    loadRemote,
    localPathRef,
    onSecondFactorRequired,
    remotePathRef,
    setError,
    setMessage,
    clearTransferRefreshTimer,
    connectionState,
    scheduleTransferStatusRefresh,
  ]);

  const enqueueTransfer = useCallback((request: Omit<SftpTransferRequest, 'id'>) => {
    const queuedRequest: SftpTransferRequest = {
      ...request,
      id: `transfer-${Date.now()}-${transferQueueSeqRef.current}`,
    };
    transferQueueSeqRef.current += 1;

    setQueuedTransfers((previous) => {
      const next = [...previous, queuedRequest];
      if (previous.length > 0 || transfering) {
        setMessage(`已加入队列：${queuedRequest.label}（当前队列 ${next.length}）`);
        setError(null);
      }
      return next;
    });
  }, [setError, setMessage, transfering]);

  useEffect(() => {
    if (!currentRemoteConnectionId || transfering || queuedTransfers.length === 0) return;
    const [next, ...rest] = queuedTransfers;
    setQueuedTransfers(rest);
    setMessage(`开始队列任务：${next.label}${rest.length > 0 ? `（剩余 ${rest.length}）` : ''}`);
    setError(null);
    void startTransfer(next.direction, next.localSource, next.remoteSource, next.fallbackSuccess);
  }, [currentRemoteConnectionId, queuedTransfers, setError, setMessage, startTransfer, transfering]);

  useEffect(() => () => clearTransferRefreshTimer(), [clearTransferRefreshTimer]);

  useRemoteSftpTransferRealtime({
    connectionId: currentRemoteConnectionId,
    transferId: transferStatus?.id || null,
    enabled: Boolean(currentRemoteConnectionId && transferStatus?.id && transfering),
    onTransferUpdated: async (payload) => {
      const latest = normalizeTransferStatus(payload);
      setTransferStatus(latest);
      if (latest.state === 'success') {
        await finalizeTransferSuccess(
          latest,
          activeTransferContextRef.current?.fallbackSuccess || '传输完成',
        );
        return;
      }
      if (latest.state === 'cancelled') {
        finalizeTransferCancelled(latest);
        return;
      }
      if (latest.state === 'error') {
        finalizeTransferError(latest, activeTransferContextRef.current);
      }
    },
  });

  useEffect(() => {
    if (connectionState !== 'connected' || !transfering) {
      return;
    }
    clearTransferRefreshTimer();
  }, [clearTransferRefreshTimer, connectionState, transfering]);

  useEffect(() => {
    if (!transfering || connectionState === 'connected') {
      return undefined;
    }
    if (typeof document === 'undefined') {
      return undefined;
    }
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'hidden') {
        clearTransferRefreshTimer();
        return;
      }
      scheduleTransferStatusRefresh(0);
    };
    const handleWindowFocus = () => {
      if (!isDocumentVisible()) {
        return;
      }
      scheduleTransferStatusRefresh(0);
    };
    const handleWindowOnline = () => {
      scheduleTransferStatusRefresh(0);
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('focus', handleWindowFocus);
    window.addEventListener('online', handleWindowOnline);
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('focus', handleWindowFocus);
      window.removeEventListener('online', handleWindowOnline);
    };
  }, [clearTransferRefreshTimer, connectionState, scheduleTransferStatusRefresh, transfering]);

  useEffect(() => {
    if (!transfering || connectionState === 'connected') {
      return;
    }
    scheduleTransferStatusRefresh();
  }, [connectionState, scheduleTransferStatusRefresh, transferStatus?.id, transfering]);

  const handleRemoveQueuedTransfer = useCallback((transferId: string) => {
    setQueuedTransfers((previous) => previous.filter((item) => item.id !== transferId));
  }, []);

  const handleClearQueuedTransfers = useCallback(() => {
    if (queuedTransfers.length === 0) return;
    setQueuedTransfers([]);
    setMessage('已清空传输队列');
    setError(null);
  }, [queuedTransfers.length, setError, setMessage]);

  const handleCancelTransfer = useCallback(async () => {
    if (!currentRemoteConnectionId || !transferStatus?.id || !transfering) return;
    try {
      const status = normalizeTransferStatus(
        await client.cancelRemoteSftpTransfer(currentRemoteConnectionId, transferStatus.id),
      );
      setTransferStatus(status);
      transferStatusRef.current = status;
      setMessage(null);
      setError(null);
      if (connectionState !== 'connected') {
        scheduleTransferStatusRefresh();
      }
    } catch (error) {
      setError(resolveRemoteSftpErrorMessage(error, '取消传输失败'));
    }
  }, [
    client,
    connectionState,
    currentRemoteConnectionId,
    scheduleTransferStatusRefresh,
    setError,
    setMessage,
    transferStatus?.id,
    transfering,
  ]);

  return {
    transfering,
    transferStatus,
    queuedTransfers,
    enqueueTransfer,
    handleRemoveQueuedTransfer,
    handleClearQueuedTransfers,
    handleCancelTransfer,
    resetTransferState,
  };
};
