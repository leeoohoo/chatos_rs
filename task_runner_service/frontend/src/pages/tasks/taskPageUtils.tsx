// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import dayjs from 'dayjs';
import { Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  CreateTaskPayload,
  RemoteServerRecord,
  TaskRecord,
  TaskRunEventRecord,
  TaskRunRecord,
  TaskProfile,
  TaskScheduleConfig,
  TaskScheduleMode,
  TaskStatus,
  AskUserPromptStatus,
} from '../../types';
import {
  isRemoteToolName,
  payloadAsOptionalNumber,
  payloadAsOptionalString,
  payloadAsRecord,
  summarizeRemoteOperationStats,
  type RemoteOperationStats,
} from '../shared/remoteOperationUtils';

export { formatRemoteEndpoint as formatTaskRemoteEndpoint } from '../shared/remoteOperationUtils';

export type TaskFormValues = {
  title: string;
  objective: string;
  description?: string;
  priority?: number;
  status: TaskStatus;
  projectId: string;
  default_model_config_id?: string;
  requiresExecution: boolean;
  prerequisite_task_ids?: string[];
  tagsText?: string;
  pluginDeviceId?: string;
  pluginWorkspaceId?: string;
  selectedPluginIds?: string[];
  pluginCommandSelections?: Record<string, boolean>;
  pluginCommandArguments?: Record<string, string>;
  pluginAgentSelection?: string;
  scheduleMode: TaskScheduleMode;
  scheduleRunAt?: string;
  scheduleIntervalSeconds?: number;
  taskProfile: TaskProfile;
};

export type RunTaskFormValues = {
  model_config_id?: string;
  prompt_override?: string;
};

export function buildCreateTaskFormValues(
  routeProjectId?: string,
): TaskFormValues {
  return {
    title: '',
    objective: '',
    description: '',
    priority: 0,
    status: 'draft',
    taskProfile: 'default',
    projectId: normalizeTaskProjectId(routeProjectId),
    default_model_config_id: undefined,
    requiresExecution: true,
    prerequisite_task_ids: [],
    tagsText: '',
    pluginDeviceId: undefined,
    pluginWorkspaceId: undefined,
    selectedPluginIds: [],
    pluginCommandSelections: {},
    pluginCommandArguments: {},
    pluginAgentSelection: undefined,
    scheduleMode: 'manual',
    scheduleRunAt: undefined,
    scheduleIntervalSeconds: undefined,
  };
}

export function buildEditTaskFormValues(task: TaskRecord): TaskFormValues {
  return {
    title: task.title,
    objective: task.objective,
    description: task.description || '',
    priority: task.priority,
    status: task.status,
    taskProfile: task.task_profile || 'default',
    projectId: normalizeTaskProjectId(task.project_id),
    default_model_config_id: task.default_model_config_id || undefined,
    requiresExecution: task.mcp_config.requires_execution ?? true,
    prerequisite_task_ids: task.prerequisite_task_ids || [],
    tagsText: task.tags.join(', '),
    pluginDeviceId: task.plugin_config?.device_id || undefined,
    pluginWorkspaceId: task.plugin_config?.workspace_id || undefined,
    selectedPluginIds:
      task.plugin_config?.selected_plugins?.map((plugin) => plugin.plugin_id) || [],
    pluginCommandSelections: Object.fromEntries(
      (task.plugin_config?.selected_plugins || []).flatMap((plugin) =>
        (plugin.selected_command_ids || []).map((commandId) => [
          taskPluginCommandKey(plugin.plugin_id, commandId),
          true,
        ]),
      ),
    ),
    pluginCommandArguments: Object.fromEntries(
      (task.plugin_config?.command_invocations || [])
        .filter((invocation) => invocation.arguments?.trim())
        .map((invocation) => [
          taskPluginCommandKey(invocation.plugin_id, invocation.command_id),
          invocation.arguments!.trim(),
        ]),
    ),
    pluginAgentSelection: (task.plugin_config?.selected_plugins || [])
      .flatMap((plugin) =>
        (plugin.selected_agent_ids || []).map((agentId) =>
          taskPluginAgentKey(plugin.plugin_id, agentId),
        ),
      )[0],
    scheduleMode: task.schedule.mode,
    scheduleRunAt: formatScheduleInput(task.schedule.run_at ?? task.schedule.next_run_at),
    scheduleIntervalSeconds: task.schedule.interval_seconds || undefined,
  };
}

export function buildTaskPayload(
  values: TaskFormValues,
  options: {
    routeProjectId?: string;
  },
): CreateTaskPayload | null {
  const schedule = buildSchedulePayload(values);
  if (!schedule) {
    return null;
  }

  const selectedPluginIds = values.selectedPluginIds || [];
  const selectedPluginIdSet = new Set(selectedPluginIds);
  const selectedCommandsByPlugin = new Map<string, string[]>();
  Object.entries(values.pluginCommandSelections || {}).forEach(([key, selected]) => {
    if (!selected) {
      return;
    }
    const parsed = parseTaskPluginCommandKey(key);
    if (!parsed || !selectedPluginIdSet.has(parsed.pluginId)) {
      return;
    }
    const commands = selectedCommandsByPlugin.get(parsed.pluginId) || [];
    if (!commands.includes(parsed.commandId)) {
      commands.push(parsed.commandId);
    }
    selectedCommandsByPlugin.set(parsed.pluginId, commands);
  });
  const commandInvocations = Array.from(selectedCommandsByPlugin.entries()).flatMap(
    ([pluginId, commandIds]) =>
      commandIds.flatMap((commandId) => {
        const argumentsValue = values.pluginCommandArguments?.[
          taskPluginCommandKey(pluginId, commandId)
        ]?.trim();
        return argumentsValue
          ? [{ plugin_id: pluginId, command_id: commandId, arguments: argumentsValue }]
          : [];
      }),
  );
  const selectedAgent = values.pluginAgentSelection
    ? parseTaskPluginAgentKey(values.pluginAgentSelection)
    : null;

  return {
    title: values.title,
    objective: values.objective,
    description: values.description?.trim() || undefined,
    priority: values.priority,
    status: values.status,
    task_profile: values.taskProfile,
    default_model_config_id: values.default_model_config_id,
    project_id: normalizeTaskProjectId(values.projectId || options.routeProjectId),
    prerequisite_task_ids: values.prerequisite_task_ids || [],
    tags: values.tagsText
      ?.split(',')
      .map((item) => item.trim())
      .filter(Boolean),
    schedule,
    plugin_config: {
      device_id: values.pluginDeviceId?.trim() || undefined,
      workspace_id: values.pluginWorkspaceId?.trim() || undefined,
      selected_plugins: selectedPluginIds.map((pluginId) => ({
        plugin_id: pluginId,
        selected_skill_ids: [],
        selected_command_ids: selectedCommandsByPlugin.get(pluginId) || [],
        selected_agent_ids:
          selectedAgent?.pluginId === pluginId && selectedPluginIdSet.has(pluginId)
            ? [selectedAgent.agentId]
            : [],
      })),
      command_invocations: commandInvocations,
    },
    mcp_config: {
      requires_execution: values.requiresExecution,
    },
  };
}

export function taskPluginCommandKey(pluginId: string, commandId: string): string {
  return JSON.stringify([pluginId, commandId]);
}

export function taskPluginAgentKey(pluginId: string, agentId: string): string {
  return JSON.stringify([pluginId, agentId]);
}

function parseTaskPluginAgentKey(
  value: string,
): { pluginId: string; agentId: string } | null {
  try {
    const parsed = JSON.parse(value);
    if (
      !Array.isArray(parsed) ||
      parsed.length !== 2 ||
      typeof parsed[0] !== 'string' ||
      typeof parsed[1] !== 'string'
    ) {
      return null;
    }
    const pluginId = parsed[0].trim();
    const agentId = parsed[1].trim();
    return pluginId && agentId ? { pluginId, agentId } : null;
  } catch {
    return null;
  }
}

function parseTaskPluginCommandKey(
  value: string,
): { pluginId: string; commandId: string } | null {
  try {
    const parsed = JSON.parse(value);
    if (
      !Array.isArray(parsed) ||
      parsed.length !== 2 ||
      typeof parsed[0] !== 'string' ||
      typeof parsed[1] !== 'string' ||
      !parsed[0].trim() ||
      !parsed[1].trim()
    ) {
      return null;
    }
    return { pluginId: parsed[0], commandId: parsed[1] };
  } catch {
    return null;
  }
}

function normalizeTaskProjectId(value?: string | null): string {
  const trimmed = value?.trim();
  return trimmed && trimmed !== '0' ? trimmed : '-1';
}

export const taskProfileValues: TaskProfile[] = ['default', 'chatos_plan'];

export const taskProfileColorMap: Record<TaskProfile, string> = {
  default: 'default',
  chatos_plan: 'geekblue',
};

export function taskProfileLabel(
  profile: string | undefined,
  t: TranslateFn,
  requiresExecution?: boolean,
): string {
  if (profile === 'chatos_plan' && requiresExecution !== true) {
    return t('tasks.profile.chatosPlan');
  }
  return t('tasks.profile.default');
}

export const statusColorMap: Record<TaskStatus, string> = {
  draft: 'default',
  ready: 'blue',
  queued: 'gold',
  running: 'processing',
  succeeded: 'success',
  failed: 'error',
  blocked: 'warning',
  cancelled: 'default',
  archived: 'default',
};

export const taskStatusValues: TaskStatus[] = [
  'draft',
  'ready',
  'running',
  'succeeded',
  'failed',
  'blocked',
  'cancelled',
  'archived',
];

export const statusFilterValues: Array<'all' | TaskStatus> = [
  'all',
  'draft',
  'ready',
  'queued',
  'running',
  'succeeded',
  'failed',
];

export const runStatusColorMap: Record<TaskRunRecord['status'], string> = {
  queued: 'default',
  running: 'processing',
  succeeded: 'success',
  failed: 'error',
  cancelled: 'default',
  blocked: 'warning',
};

export const scheduleModeLabelKeys: Record<TaskScheduleMode, string> = {
  manual: 'tasks.schedule.manual',
  once: 'tasks.schedule.once',
  interval: 'tasks.schedule.interval',
  contact_async: 'tasks.schedule.contactAsync',
};

export const scheduleModeDescriptionKeys: Record<TaskScheduleMode, string> = {
  manual: 'tasks.schedule.manualDescription',
  once: 'tasks.schedule.onceDescription',
  interval: 'tasks.schedule.intervalDescription',
  contact_async: 'tasks.schedule.contactAsyncDescription',
};

export const promptStatusColorMap: Record<AskUserPromptStatus, string> = {
  pending: 'processing',
  submitted: 'success',
  cancelled: 'default',
  timed_out: 'warning',
  failed: 'error',
};

export function taskCreatorLabel(task: TaskRecord): string {
  const displayName = task.creator_display_name?.trim();
  const username = task.creator_username?.trim();
  if (displayName && username && displayName !== username) {
    return `${displayName} (${username})`;
  }
  return displayName || username || '-';
}

export function taskOwnerLabel(task: TaskRecord): string {
  const displayName = task.owner_display_name?.trim();
  const username = task.owner_username?.trim();
  if (displayName && username && displayName !== username) {
    return `${displayName} (${username})`;
  }
  return displayName || username || task.owner_user_id || taskCreatorLabel(task);
}

export function taskRunReportContent(run?: TaskRunRecord | null): string | null {
  const report = run?.report;
  if (!report || typeof report !== 'object' || Array.isArray(report)) {
    return null;
  }
  const content = (report as { content?: unknown }).content;
  if (typeof content !== 'string') {
    return null;
  }
  const trimmed = content.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function isSchedulerOnlyTask(task: Pick<TaskRecord, 'schedule'>): boolean {
  return task.schedule.mode === 'contact_async';
}

export function isTaskRunActionDisabled(
  task: Pick<TaskRecord, 'status' | 'schedule'>,
): boolean {
  return task.status === 'queued'
    || task.status === 'running'
    || task.status === 'cancelled'
    || isSchedulerOnlyTask(task);
}

export function taskModelOptionLabel(
  model: {
    name: string;
    model: string;
    usage_scenario?: string | null;
    enabled?: boolean;
  },
  t: TranslateFn,
): string {
  const parts = [`${model.name} (${model.model})`];
  const usageScenario = model.usage_scenario?.trim();
  if (usageScenario) {
    parts.push(usageScenario);
  }
  let label = parts.join(' - ');
  if (model.enabled === false) {
    label = `${label} / ${t('common.disabled')}`;
  }
  return label;
}

export function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      <Typography.Paragraph
        style={{
          background: '#fafafa',
          padding: 12,
          borderRadius: 6,
          marginBottom: 0,
          whiteSpace: 'pre-wrap',
          fontFamily: 'monospace',
        }}
      >
        {JSON.stringify(value, null, 2)}
      </Typography.Paragraph>
    </div>
  );
}

export type TaskRemoteOperationView = {
  name: string;
  success: boolean;
  connectionId?: string;
  connectionName?: string;
  username?: string;
  host?: string;
  port?: number;
  command?: string;
  path?: string;
  remoteHost?: string;
  content?: string;
  summary?: string;
};

export type TaskRemoteOperationStats = RemoteOperationStats;

export function collectTaskRemoteOperations(
  events: TaskRunEventRecord[],
  remoteServerMap: Map<string, RemoteServerRecord>,
): TaskRemoteOperationView[] {
  return events
    .filter((event) => event.event_type === 'tool_stream')
    .map((event) => payloadAsRecord(event.payload))
    .filter((payload): payload is Record<string, unknown> => Boolean(payload))
    .filter((payload) => isRemoteToolName(payloadAsOptionalString(payload.name) || ''))
    .map((payload) => {
      const result = payloadAsRecord(payload.result);
      const nestedResult = payloadAsRecord(result?.result);
      const connectionId = payloadAsOptionalString(result?.connection_id);
      const remoteServer = connectionId ? remoteServerMap.get(connectionId) : undefined;
      const command = payloadAsOptionalString(result?.command);
      const path = payloadAsOptionalString(result?.path);
      const connectionName = payloadAsOptionalString(result?.name) || remoteServer?.name;

      return {
        name: payloadAsOptionalString(payload.name) || 'unknown_tool',
        success: Boolean(payload.success) && !Boolean(payload.is_error),
        connectionId,
        connectionName,
        username: payloadAsOptionalString(result?.username) || remoteServer?.username,
        host: payloadAsOptionalString(result?.host) || remoteServer?.host,
        port: payloadAsOptionalNumber(result?.port) || remoteServer?.port,
        command,
        path,
        remoteHost: payloadAsOptionalString(nestedResult?.remote_host),
        content: payloadAsOptionalString(payload.content),
        summary: command || path || payloadAsOptionalString(payload.content),
      };
    });
}

export function summarizeTaskRemoteOperations(
  items: TaskRemoteOperationView[],
): TaskRemoteOperationStats {
  return summarizeRemoteOperationStats(items);
}

export function buildSchedulePayload(values: TaskFormValues): TaskScheduleConfig | null {
  if (values.scheduleMode === 'manual') {
    return {
      mode: 'manual',
    };
  }

  const runAtInput = values.scheduleRunAt?.trim();
  if (!runAtInput) {
    return null;
  }
  const runAt = dayjs(runAtInput);
  if (!runAt.isValid()) {
    return null;
  }

  if (values.scheduleMode === 'once') {
    return {
      mode: 'once',
      run_at: runAt.toISOString(),
    };
  }

  if (values.scheduleMode === 'contact_async') {
    return {
      mode: 'contact_async',
      run_at: runAt.toISOString(),
    };
  }

  if (!values.scheduleIntervalSeconds || values.scheduleIntervalSeconds < 60) {
    return null;
  }

  return {
    mode: 'interval',
    run_at: runAt.toISOString(),
    interval_seconds: values.scheduleIntervalSeconds,
  };
}

export function formatScheduleInput(value?: string | null): string | undefined {
  if (!value) {
    return undefined;
  }
  const parsed = dayjs(value);
  if (!parsed.isValid()) {
    return undefined;
  }
  return parsed.format('YYYY-MM-DDTHH:mm:ss');
}

export function describeTaskSchedule(schedule: TaskScheduleConfig, t: TranslateFn): string {
  if (schedule.mode === 'manual') {
    return t(scheduleModeLabelKeys.manual);
  }

  const parts: string[] = [t(scheduleModeLabelKeys[schedule.mode])];
  if (schedule.next_run_at) {
    parts.push(t('tasks.schedule.nextAt', {
      time: dayjs(schedule.next_run_at).format('YYYY-MM-DD HH:mm:ss'),
    }));
  } else if (schedule.run_at) {
    parts.push(dayjs(schedule.run_at).format('YYYY-MM-DD HH:mm:ss'));
  }
  if (schedule.interval_seconds) {
    parts.push(t('tasks.schedule.everySeconds', { seconds: schedule.interval_seconds }));
  }
  return parts.join(' / ');
}

export function memoryRoleColor(role: string): string {
  switch (role) {
    case 'assistant':
      return 'blue';
    case 'tool':
      return 'purple';
    case 'system':
      return 'gold';
    case 'user':
      return 'green';
    default:
      return 'default';
  }
}

export function memorySummaryColor(status: string): string {
  switch (status) {
    case 'done':
      return 'success';
    case 'pending':
      return 'warning';
    case 'running':
      return 'processing';
    case 'failed':
      return 'error';
    default:
      return 'default';
  }
}
