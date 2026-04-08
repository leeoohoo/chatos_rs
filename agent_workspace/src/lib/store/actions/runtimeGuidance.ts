import type ApiClient from '../../api/client';
import {
  queueOptimisticRuntimeGuidance,
  reconcileSubmittedRuntimeGuidance,
  rollbackRuntimeGuidanceSubmission,
} from './sendMessage/runtimeGuidanceState';
import type {
  ChatActions,
  ChatStoreSet,
} from '../types';

interface Deps {
  set: ChatStoreSet;
  client: ApiClient;
}

type SubmitRuntimeGuidanceAction = Pick<ChatActions, 'submitRuntimeGuidance'>;

export function createRuntimeGuidanceActions({
  set,
  client,
}: Deps): SubmitRuntimeGuidanceAction {
  return {
    submitRuntimeGuidance: async (
      content: string,
      options: { sessionId: string; turnId: string; projectId?: string | null },
    ) => {
      const sessionId = String(options?.sessionId || '').trim();
      const turnId = String(options?.turnId || '').trim();
      const trimmedContent = String(content || '').trim();
      const projectId = typeof options?.projectId === 'string'
        ? options.projectId.trim()
        : '';

      if (!sessionId || !turnId || !trimmedContent) {
        throw new Error('缺少运行时引导参数');
      }

      const guidanceAt = new Date().toISOString();
      const optimisticGuidanceId = `local_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

      set((state) => {
        queueOptimisticRuntimeGuidance(state, {
          sessionId,
          turnId,
          guidanceId: optimisticGuidanceId,
          content: trimmedContent,
          guidanceAt,
        });
      });

      try {
        const response = await client.submitRuntimeGuidance({
          sessionId,
          turnId,
          content: trimmedContent,
          projectId: projectId || undefined,
        });

        set((state) => {
          const pendingFromResponse = Number.isFinite(Number(response?.pending_count))
            ? Math.max(0, Number(response?.pending_count))
            : null;
          const nextGuidanceId = String(response?.guidance_id || '').trim() || optimisticGuidanceId;

          reconcileSubmittedRuntimeGuidance(state, {
            sessionId,
            turnId,
            optimisticGuidanceId,
            responseGuidanceId: nextGuidanceId,
            content: trimmedContent,
            guidanceAt,
            status: typeof response?.status === 'string' ? response.status : null,
            pendingCount: pendingFromResponse,
          });

          if (state.currentSessionId === sessionId && Array.isArray(state.messages)) {
            const guidanceMessageIndex = state.messages.findIndex((message) => (
              message?.id === optimisticGuidanceId
              || message?.id === nextGuidanceId
            ));
            if (guidanceMessageIndex >= 0) {
              state.messages[guidanceMessageIndex] = {
                ...state.messages[guidanceMessageIndex],
                id: nextGuidanceId,
                status: 'completed',
              };
            }
          }
        });

        return {
          success: response?.success === true,
          guidanceId: response?.guidance_id,
          status: response?.status,
          pendingCount: response?.pending_count,
          turnId: response?.turn_id,
        };
      } catch (error) {
        set((state) => {
          rollbackRuntimeGuidanceSubmission(state, {
            sessionId,
            guidanceId: optimisticGuidanceId,
          });
        });
        throw error;
      }
    },
  };
}
