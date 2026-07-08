// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeTerminalListInvalidatedPayloadWrapper,
} from './types';

interface UseTerminalListRealtimeOptions {
  enabled?: boolean;
  projectId?: string | null;
  onInvalidate: (payload: RealtimeTerminalListInvalidatedPayloadWrapper) => void | Promise<void>;
}

const isTerminalListInvalidatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeTerminalListInvalidatedPayloadWrapper } => (
  envelope?.payload?.kind === 'terminal_list_invalidated'
);

export const useTerminalListRealtime = ({
  enabled = true,
  projectId,
  onInvalidate,
}: UseTerminalListRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeTerminalListInvalidatedPayloadWrapper>({
    delayMs: 250,
    onExecute: (payload) => onInvalidateRef.current(payload),
  });

  useRealtimeTopic(
    projectId ? { scope: 'project', id: projectId } : null,
    enabled && Boolean(projectId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || event.event !== 'terminal.list.invalidated') {
      return;
    }
    if (!isTerminalListInvalidatedPayload(event)) {
      return;
    }
    if (projectId) {
      const payloadProjectId = String(
        event.project_id
        || event.payload.project_id
        || '',
      ).trim();
      if (!payloadProjectId || payloadProjectId !== projectId) {
        return;
      }
    }
    queue.run(event.payload);
  });
};
