// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, type Dispatch, type SetStateAction } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { FormInstance } from 'antd';

import { api } from '../../api/client';
import type {
  TaskMcpResolutionResponse,
  TaskRecord,
  TaskStatus,
} from '../../types';
import {
  buildEditTaskFormValues,
  completeEnabledBuiltinKindDependencies,
  type TaskFormValues,
} from './taskPageUtils';

type UseTasksPageEffectsParams = {
  visibleTasks?: TaskRecord[];
  routeTaskId: string | null;
  statusFilter: 'all' | TaskStatus;
  keywordFilter: string;
  tagFilter?: string;
  routeModelConfigId?: string;
  routeProjectId?: string;
  scheduledOnly: boolean;
  drawerOpen: boolean;
  editingTask: TaskRecord | null;
  form: FormInstance<TaskFormValues>;
  taskEditorMcpResolution?: TaskMcpResolutionResponse;
  subtasksParentTask: TaskRecord | null;
  setSelectedTaskIds: Dispatch<SetStateAction<string[]>>;
  setTaskPage: Dispatch<SetStateAction<number>>;
  setDetailTaskId: Dispatch<SetStateAction<string | null>>;
  setDetailTaskPreview: Dispatch<SetStateAction<TaskRecord | null>>;
};

export function useTasksPageEffects({
  visibleTasks,
  routeTaskId,
  statusFilter,
  keywordFilter,
  tagFilter,
  routeModelConfigId,
  routeProjectId,
  scheduledOnly,
  drawerOpen,
  editingTask,
  form,
  taskEditorMcpResolution,
  subtasksParentTask,
  setSelectedTaskIds,
  setTaskPage,
  setDetailTaskId,
  setDetailTaskPreview,
}: UseTasksPageEffectsParams) {
  const taskSubtasksQuery = useQuery({
    queryKey: ['task-subtasks', subtasksParentTask?.id],
    queryFn: () =>
      api.listTasks({
        parent_task_id: subtasksParentTask!.id,
        limit: 100,
      }),
    enabled: Boolean(subtasksParentTask),
  });

  useEffect(() => {
    if (!visibleTasks) {
      return;
    }
    const visibleIds = new Set(visibleTasks.map((task) => task.id));
    setSelectedTaskIds((current) => current.filter((taskId) => visibleIds.has(taskId)));
  }, [setSelectedTaskIds, visibleTasks]);

  useEffect(() => {
    setTaskPage(1);
  }, [
    keywordFilter,
    routeModelConfigId,
    routeProjectId,
    scheduledOnly,
    setTaskPage,
    statusFilter,
    tagFilter,
  ]);

  useEffect(() => {
    if (routeTaskId) {
      setDetailTaskId(routeTaskId);
      setDetailTaskPreview((current) => {
        if (current?.id === routeTaskId) {
          return current;
        }
        return visibleTasks?.find((task) => task.id === routeTaskId) || null;
      });
      return;
    }
    setDetailTaskId(null);
    setDetailTaskPreview(null);
  }, [routeTaskId, setDetailTaskId, setDetailTaskPreview, visibleTasks]);

  useEffect(() => {
    const resolution = taskEditorMcpResolution;
    if (!drawerOpen || !editingTask || !resolution) {
      return;
    }
    const current = form.getFieldValue('enabledBuiltinKinds') || [];
    const stored = buildEditTaskFormValues(editingTask).enabledBuiltinKinds || [];
    if (!sameStringArray(current, stored)) {
      return;
    }
    const requested = completeEnabledBuiltinKindDependencies(
      resolution.requested_builtin_kinds,
    );
    if (!sameStringArray(current, requested)) {
      form.setFieldsValue({ enabledBuiltinKinds: requested });
    }
  }, [drawerOpen, editingTask, form, taskEditorMcpResolution]);

  return { taskSubtasksQuery };
}

function sameStringArray(left: string[], right: string[]) {
  if (left.length !== right.length) {
    return false;
  }
  return left.every((value, index) => value === right[index]);
}
