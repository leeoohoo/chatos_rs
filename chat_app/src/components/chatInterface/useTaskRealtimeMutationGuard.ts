import type { TaskRealtimeMutationGuardPayload } from './useSessionWorkbarPanels.types';
import { useRecentMutationGuard } from '../../hooks/useRecentMutationGuard';

const buildTaskRealtimeMutationGuardKey = (
  payload: TaskRealtimeMutationGuardPayload,
): string => {
  const action = String(payload.action || '').trim();
  const taskId = String(payload.taskId || '').trim();
  const turnId = String(payload.turnId || '').trim();
  if (!action || !taskId) {
    return '';
  }
  return `${action}:${taskId}:${turnId}`;
};

export const useTaskRealtimeMutationGuard = () => {
  const {
    markRecentMutation,
    consumeRecentMutation,
  } = useRecentMutationGuard<TaskRealtimeMutationGuardPayload>({
    buildKey: buildTaskRealtimeMutationGuardKey,
  });

  return {
    markTaskRealtimeMutationHandled: markRecentMutation,
    consumeRecentTaskRealtimeMutation: consumeRecentMutation,
  };
};
