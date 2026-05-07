import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type { RealtimeEventEnvelope, RealtimeProjectChangeSummaryPayloadWrapper } from './types';

interface UseProjectChangeSummaryRealtimeOptions {
  projectId?: string | null;
  enabled?: boolean;
  onInvalidate: () => void | Promise<void>;
}

const isProjectChangeSummaryPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeProjectChangeSummaryPayloadWrapper } => {
  return envelope?.payload?.kind === 'project_change_summary';
};

export const useProjectChangeSummaryRealtime = ({
  projectId,
  enabled = true,
  onInvalidate,
}: UseProjectChangeSummaryRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeProjectChangeSummaryPayloadWrapper>({
    delayMs: 300,
    onExecute: () => onInvalidateRef.current(),
  });

  useRealtimeTopic(
    projectId ? { scope: 'project', id: projectId } : null,
    enabled && Boolean(projectId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !projectId || event.event !== 'project.change_summary.updated') {
      return;
    }
    if (!isProjectChangeSummaryPayload(event)) {
      return;
    }
    const payloadProjectId = String(
      event.project_id
      || event.payload.project_id
      || '',
    ).trim();
    if (!payloadProjectId || payloadProjectId !== projectId) {
      return;
    }
    queue.run(event.payload);
  });
};
