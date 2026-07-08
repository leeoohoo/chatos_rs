// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeRemoteSftpTransferPayloadWrapper,
} from './types';

interface UseRemoteSftpTransferRealtimeOptions {
  connectionId?: string | null;
  transferId?: string | null;
  enabled?: boolean;
  onTransferUpdated: (payload: RealtimeRemoteSftpTransferPayloadWrapper) => void | Promise<void>;
}

const isRemoteSftpTransferPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeRemoteSftpTransferPayloadWrapper } => (
  envelope?.payload?.kind === 'remote_sftp_transfer'
);

export const useRemoteSftpTransferRealtime = ({
  connectionId,
  transferId,
  enabled = true,
  onTransferUpdated,
}: UseRemoteSftpTransferRealtimeOptions) => {
  const onTransferUpdatedRef = useRef(onTransferUpdated);

  useEffect(() => {
    onTransferUpdatedRef.current = onTransferUpdated;
  }, [onTransferUpdated]);

  useRealtimeTopic(
    connectionId ? { scope: 'remote_connection', id: connectionId } : null,
    enabled && Boolean(connectionId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !connectionId || !transferId || event.event !== 'remote.sftp.transfer.updated') {
      return;
    }
    if (!isRemoteSftpTransferPayload(event)) {
      return;
    }

    const payloadConnectionId = String(event.payload.connection_id || '').trim();
    const payloadTransferId = String(event.payload.id || '').trim();
    if (!payloadConnectionId || !payloadTransferId) {
      return;
    }
    if (payloadConnectionId !== connectionId || payloadTransferId !== transferId) {
      return;
    }
    void onTransferUpdatedRef.current(event.payload);
  });
};
