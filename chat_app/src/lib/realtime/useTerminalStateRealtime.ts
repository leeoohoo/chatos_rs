import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeTerminalStatePayloadWrapper,
} from './types';

interface UseTerminalStateRealtimeOptions {
  terminalId?: string | null;
  enabled?: boolean;
  onStateChanged: (payload: RealtimeTerminalStatePayloadWrapper) => void | Promise<void>;
}

const isTerminalStatePayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeTerminalStatePayloadWrapper } => (
  envelope?.payload?.kind === 'terminal_state'
);

export const useTerminalStateRealtime = ({
  terminalId,
  enabled = true,
  onStateChanged,
}: UseTerminalStateRealtimeOptions) => {
  const onStateChangedRef = useRef(onStateChanged);

  useEffect(() => {
    onStateChangedRef.current = onStateChanged;
  }, [onStateChanged]);

  useRealtimeTopic(
    terminalId ? { scope: 'terminal', id: terminalId } : null,
    enabled && Boolean(terminalId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !terminalId || event.event !== 'terminal.state_changed') {
      return;
    }
    if (!isTerminalStatePayload(event)) {
      return;
    }
    const payloadTerminalId = String(event.payload.terminal_id || '').trim();
    if (!payloadTerminalId || payloadTerminalId !== terminalId) {
      return;
    }
    void onStateChangedRef.current(event.payload);
  });
};
