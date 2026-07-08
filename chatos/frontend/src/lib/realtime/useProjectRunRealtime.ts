// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeProjectMembersUpdatedPayloadWrapper,
  RealtimeProjectRunCatalogPayloadWrapper,
  RealtimeProjectRunInstancePayloadWrapper,
  RealtimeProjectRunStatePayloadWrapper,
} from './types';

interface UseProjectRunRealtimeOptions {
  projectId?: string | null;
  enabled?: boolean;
  onRunStateChanged?: (payload: RealtimeProjectRunStatePayloadWrapper) => void | Promise<void>;
  onRunInstanceChanged?: (payload: RealtimeProjectRunInstancePayloadWrapper) => void | Promise<void>;
  onCatalogUpdated?: (payload: RealtimeProjectRunCatalogPayloadWrapper) => void | Promise<void>;
  onMembersUpdated?: (payload: RealtimeProjectMembersUpdatedPayloadWrapper) => void | Promise<void>;
}

const isProjectRunStatePayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeProjectRunStatePayloadWrapper } => (
  envelope?.payload?.kind === 'project_run_state'
);

const isProjectRunCatalogPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeProjectRunCatalogPayloadWrapper } => (
  envelope?.payload?.kind === 'project_run_catalog'
);

const isProjectRunInstancePayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeProjectRunInstancePayloadWrapper } => (
  envelope?.payload?.kind === 'project_run_instance'
);

const isProjectMembersUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeProjectMembersUpdatedPayloadWrapper } => (
  envelope?.payload?.kind === 'project_members_updated'
);

export const useProjectRunRealtime = ({
  projectId,
  enabled = true,
  onRunStateChanged,
  onRunInstanceChanged,
  onCatalogUpdated,
  onMembersUpdated,
}: UseProjectRunRealtimeOptions) => {
  const onRunStateChangedRef = useRef(onRunStateChanged);
  const onRunInstanceChangedRef = useRef(onRunInstanceChanged);
  const onCatalogUpdatedRef = useRef(onCatalogUpdated);
  const onMembersUpdatedRef = useRef(onMembersUpdated);

  useEffect(() => {
    onRunStateChangedRef.current = onRunStateChanged;
  }, [onRunStateChanged]);

  useEffect(() => {
    onRunInstanceChangedRef.current = onRunInstanceChanged;
  }, [onRunInstanceChanged]);

  useEffect(() => {
    onCatalogUpdatedRef.current = onCatalogUpdated;
  }, [onCatalogUpdated]);

  useEffect(() => {
    onMembersUpdatedRef.current = onMembersUpdated;
  }, [onMembersUpdated]);

  const runStateQueue = useRealtimeInvalidationQueue<RealtimeProjectRunStatePayloadWrapper>({
    delayMs: 150,
    onExecute: (payload) => onRunStateChangedRef.current?.(payload),
  });

  const catalogQueue = useRealtimeInvalidationQueue<RealtimeProjectRunCatalogPayloadWrapper>({
    delayMs: 150,
    onExecute: (payload) => onCatalogUpdatedRef.current?.(payload),
  });

  const membersQueue = useRealtimeInvalidationQueue<RealtimeProjectMembersUpdatedPayloadWrapper>({
    delayMs: 150,
    onExecute: (payload) => onMembersUpdatedRef.current?.(payload),
  });

  useRealtimeTopic(
    projectId ? { scope: 'project', id: projectId } : null,
    enabled && Boolean(projectId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !projectId) {
      return;
    }

    if (event.event === 'project.run.state_changed' && isProjectRunStatePayload(event)) {
      const payloadProjectId = String(
        event.project_id
        || event.payload.project_id
        || '',
      ).trim();
      if (payloadProjectId === projectId) {
        runStateQueue.run(event.payload);
      }
      return;
    }

    if (event.event === 'project.run.instance_changed' && isProjectRunInstancePayload(event)) {
      const payloadProjectId = String(
        event.project_id
        || event.payload.project_id
        || '',
      ).trim();
      if (payloadProjectId === projectId) {
        void onRunInstanceChangedRef.current?.(event.payload);
      }
      return;
    }

    if (event.event === 'project.run.catalog.updated' && isProjectRunCatalogPayload(event)) {
      const payloadProjectId = String(
        event.project_id
        || event.payload.project_id
        || '',
      ).trim();
      if (payloadProjectId === projectId) {
        catalogQueue.run(event.payload);
      }
      return;
    }

    if (event.event === 'project.members.updated' && isProjectMembersUpdatedPayload(event)) {
      const payloadProjectId = String(
        event.project_id
        || event.payload.project_id
        || '',
      ).trim();
      if (payloadProjectId === projectId) {
        membersQueue.run(event.payload);
      }
    }
  });
};
