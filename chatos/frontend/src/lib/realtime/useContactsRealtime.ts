// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeContactsUpdatedPayloadWrapper,
  RealtimeEventEnvelope,
} from './types';

interface UseContactsRealtimeOptions {
  enabled?: boolean;
  onInvalidate: (payload: RealtimeContactsUpdatedPayloadWrapper) => void | Promise<void>;
}

const isContactsUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeContactsUpdatedPayloadWrapper } => {
  return envelope?.payload?.kind === 'contacts_updated';
};

export const useContactsRealtime = ({
  enabled = true,
  onInvalidate,
}: UseContactsRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeContactsUpdatedPayloadWrapper>({
    delayMs: 250,
    onExecute: (payload) => onInvalidateRef.current(payload),
  });

  useRealtimeTopic({ scope: 'contacts' }, enabled);

  useRealtimeEvent((event) => {
    if (!enabled || event.event !== 'contacts.updated') {
      return;
    }
    if (!isContactsUpdatedPayload(event)) {
      return;
    }
    queue.run(event.payload);
  });
};
