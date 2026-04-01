import { useCallback, useEffect, useRef, useState } from 'react';

import { resolveRemoteSftpErrorMessage } from '../../lib/api/remoteConnectionErrors';

import { normalizeTransferStatus, type RemoteSftpClient } from './helpers';
import type { SftpTransferRequest, SftpTransferStatus } from './types';

interface UseRemoteSftpTransferOptions {
  client: RemoteSftpClient;
  currentRemoteConnectionId: string | null;
  loadLocal: (path?: string | null) => Promise<void>;
  loadRemote: (path?: string) => Promise<void>;
  remotePathRef: React.MutableRefObject<string>;
  localPathRef: React.MutableRefObject<string | null>;
  setMessage: (message: string | null) => void;
  setError: (message: string | null) => void;
}

export const useRemoteSftpTransfer = ({
  client,
  currentRemoteConnectionId,
  loadLocal,
  loadRemote,
  remotePathRef,
  localPathRef,
  setMessage,
  setError,
}: UseRemoteSftpTransferOptions) => {
  const [transfering, setTransfering] = useState(false);
  const [transferStatus, setTransferStatus] = useState<SftpTransferStatus | null>(null);
  const [queuedTransfers, setQueuedTransfers] = useState<SftpTransferRequest[]>([]);

  const transferPollTimerRef = useRef<number | null>(null);
  const transferPollingBusyRef = useRef(false);
  const transferQueueSeqRef = useRef(0);

  const stopTransferPolling = useCallback(() => {
    if (transferPollTimerRef.current !== null) {
      window.clearInterval(transferPollTimerRef.current);
      transferPollTimerRef.current = null;
    }
    transferPollingBusyRef.current = false;
  }, []);

  const resetTransferState = useCallback(() => {
    stopTransferPolling();
    setTransferStatus(null);
    setTransfering(false);
    setQueuedTransfers([]);
  }, [stopTransferPolling]);

  const startTransfer = useCallback(async (
    direction: 'upload' | 'download',
    localSource: string,
    remoteSource: string,
    fallbackSuccess: string,
  ) => {
    if (!currentRemoteConnectionId) return;

    stopTransferPolling();
    setTransfering(true);
    setTransferStatus(null);
    setError(null);
    setMessage(null);

    try {
      const started = normalizeTransferStatus(
        await client.startRemoteSftpTransfer(currentRemoteConnectionId, {
          direction,
          local_path: localSource,
          remote_path: remoteSource,
        }),
      );
      setTransferStatus(started);

      transferPollTimerRef.current = window.setInterval(async () => {
        if (transferPollingBusyRef.current) return;
        transferPollingBusyRef.current = true;
        try {
          const latest = normalizeTransferStatus(
            await client.getRemoteSftpTransferStatus(currentRemoteConnectionId, started.id),
          );
          setTransferStatus(latest);
          if (latest.state === 'success') {
            stopTransferPolling();
            setTransfering(false);
            setMessage(latest.message || fallbackSuccess);
            await loadRemote(remotePathRef.current);
            if (localPathRef.current !== null) {
              await loadLocal(localPathRef.current);
            } else {
              await loadLocal(null);
            }
          } else if (latest.state === 'cancelled') {
            stopTransferPolling();
            setTransfering(false);
            setMessage(latest.message || '传输已取消');
          } else if (latest.state === 'error') {
            stopTransferPolling();
            setTransfering(false);
            setError(latest.error || '传输失败');
          }
        } catch (error) {
          stopTransferPolling();
          setTransfering(false);
          setError(resolveRemoteSftpErrorMessage(error, '查询传输进度失败'));
        } finally {
          transferPollingBusyRef.current = false;
        }
      }, 350);
    } catch (error) {
      setTransfering(false);
      setTransferStatus(null);
      setError(resolveRemoteSftpErrorMessage(error, '启动传输失败'));
    }
  }, [
    client,
    currentRemoteConnectionId,
    loadLocal,
    loadRemote,
    localPathRef,
    remotePathRef,
    setError,
    setMessage,
    stopTransferPolling,
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

  useEffect(() => () => stopTransferPolling(), [stopTransferPolling]);

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
      setMessage(null);
      setError(null);
    } catch (error) {
      setError(resolveRemoteSftpErrorMessage(error, '取消传输失败'));
    }
  }, [client, currentRemoteConnectionId, setError, setMessage, transferStatus?.id, transfering]);

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
