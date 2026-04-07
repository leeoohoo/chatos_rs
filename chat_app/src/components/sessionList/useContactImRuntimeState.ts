import { useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { readSessionImConversationId } from '../../lib/store/helpers/sessionRuntime';
import type { ImConversationRuntimeState } from '../../lib/store/types';

type ContactTaskRuntimeState = {
  busy: boolean;
  status: 'pending_execute' | 'running' | null;
};

export type ImRuntimeState = ImConversationRuntimeState & {
  busySource?: 'idle' | 'im_run' | 'task_execution';
  taskBusyStatus?: ContactTaskRuntimeState['status'];
};

interface SessionLike {
  id: string;
  metadata?: unknown;
}

interface UseContactImRuntimeStateOptions {
  apiClient: Pick<ApiClient, 'getTaskManagerTasks'>;
  sessions: SessionLike[];
  displaySessions: SessionLike[];
  displayBackingSessionIdMap?: Record<string, string>;
  isCollapsed: boolean;
  imConversationRuntimeByConversationId: Record<string, ImConversationRuntimeState | undefined>;
}

const normalizeDisplaySessionRefs = (
  displaySessions: SessionLike[],
  sessions: SessionLike[],
  displayBackingSessionIdMap?: Record<string, string>,
): Array<{ displaySessionId: string; runtimeSessionId: string; conversationId: string }> => {
  return (displaySessions || [])
    .map((displaySession) => {
      const displaySessionId = typeof displaySession?.id === 'string'
        ? displaySession.id.trim()
        : '';
      if (!displaySessionId) {
        return null;
      }
      const runtimeSessionId = typeof displayBackingSessionIdMap?.[displaySessionId] === 'string'
        ? displayBackingSessionIdMap[displaySessionId].trim()
        : '';
      const runtimeSession = runtimeSessionId
        ? sessions.find((session) => session.id === runtimeSessionId) || null
        : null;
      const effectiveRuntimeSessionId = runtimeSessionId || displaySessionId;
      const conversationId = readSessionImConversationId(displaySession?.metadata)
        || readSessionImConversationId(runtimeSession?.metadata);
      if (!conversationId || !effectiveRuntimeSessionId) {
        return null;
      }
      return {
        displaySessionId,
        runtimeSessionId: effectiveRuntimeSessionId,
        conversationId,
      };
    })
    .filter(Boolean) as Array<{ displaySessionId: string; runtimeSessionId: string; conversationId: string }>;
};

export const useContactImRuntimeState = ({
  apiClient,
  sessions,
  displaySessions,
  displayBackingSessionIdMap,
  isCollapsed,
  imConversationRuntimeByConversationId,
}: UseContactImRuntimeStateOptions) => {
  const contactRuntimeSessionRefs = useMemo(
    () => normalizeDisplaySessionRefs(displaySessions, sessions, displayBackingSessionIdMap),
    [displayBackingSessionIdMap, displaySessions, sessions],
  );
  const [taskRuntimeByDisplaySessionId, setTaskRuntimeByDisplaySessionId] = useState<
    Record<string, ContactTaskRuntimeState>
  >({});
  const pollSeqRef = useRef(0);

  useEffect(() => {
    if (isCollapsed || contactRuntimeSessionRefs.length === 0) {
      setTaskRuntimeByDisplaySessionId({});
      return;
    }

    let cancelled = false;

    const loadTaskRuntime = async () => {
      const requestSeq = pollSeqRef.current + 1;
      pollSeqRef.current = requestSeq;

      const entries = await Promise.all(
        contactRuntimeSessionRefs.map(async (ref) => {
          try {
            const tasks = await apiClient.getTaskManagerTasks(ref.runtimeSessionId, {
              includeDone: false,
              limit: 50,
            });
            const activeTask = (Array.isArray(tasks) ? tasks : []).find((task) => {
              const status = typeof task?.status === 'string'
                ? task.status.trim().toLowerCase()
                : '';
              return status === 'pending_execute' || status === 'running';
            });
            const taskStatus = typeof activeTask?.status === 'string'
              ? activeTask.status.trim().toLowerCase()
              : '';
            return [
              ref.displaySessionId,
              {
                busy: Boolean(activeTask),
                status: taskStatus === 'pending_execute' || taskStatus === 'running'
                  ? taskStatus
                  : null,
              } satisfies ContactTaskRuntimeState,
            ] as const;
          } catch {
            return [
              ref.displaySessionId,
              {
                busy: false,
                status: null,
              } satisfies ContactTaskRuntimeState,
            ] as const;
          }
        }),
      );

      if (cancelled || pollSeqRef.current !== requestSeq) {
        return;
      }

      setTaskRuntimeByDisplaySessionId(Object.fromEntries(entries));
    };

    void loadTaskRuntime();
    const timer = window.setInterval(() => {
      if (typeof document !== 'undefined' && document.hidden) {
        return;
      }
      void loadTaskRuntime();
    }, 5000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [apiClient, contactRuntimeSessionRefs, isCollapsed]);

  const imRuntimeStateByRuntimeSessionId = useMemo(() => {
    if (isCollapsed || contactRuntimeSessionRefs.length === 0) {
      return {};
    }

    return Object.fromEntries(
      contactRuntimeSessionRefs.map((ref) => {
        const imRuntimeState = imConversationRuntimeByConversationId[ref.conversationId] || {
          busy: false,
          unreadCount: 0,
          latestRunStatus: null,
          lastMessagePreview: null,
          lastMessageAt: null,
        };
        const taskRuntimeState = taskRuntimeByDisplaySessionId[ref.displaySessionId] || {
          busy: false,
          status: null,
        };
        const busySource = imRuntimeState.busy
          ? 'im_run'
          : (taskRuntimeState.busy ? 'task_execution' : 'idle');
        return [
          ref.displaySessionId,
          {
            ...imRuntimeState,
            busy: imRuntimeState.busy || taskRuntimeState.busy,
            busySource,
            taskBusyStatus: taskRuntimeState.status,
          } satisfies ImRuntimeState,
        ];
      }),
    );
  }, [
    contactRuntimeSessionRefs,
    imConversationRuntimeByConversationId,
    isCollapsed,
    taskRuntimeByDisplaySessionId,
  ]);

  return {
    imRuntimeStateByRuntimeSessionId,
  };
};
