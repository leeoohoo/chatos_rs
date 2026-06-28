import dayjs from 'dayjs';
import { Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  CreateTaskPayload,
  RemoteServerRecord,
  TaskBuiltinPromptMode,
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
  default_model_config_id?: string;
  prerequisite_task_ids?: string[];
  tagsText?: string;
  mcpEnabled: boolean;
  builtinPromptMode: TaskBuiltinPromptMode;
  builtinPromptLocale: string;
  enabledBuiltinKinds: string[];
  workspaceDir?: string;
  defaultRemoteServerId?: string;
  externalMcpConfigIds?: string[];
  scheduleMode: TaskScheduleMode;
  scheduleRunAt?: string;
  scheduleIntervalSeconds?: number;
  taskProfile: TaskProfile;
};

export type RunTaskFormValues = {
  model_config_id?: string;
  prompt_override?: string;
};

export function buildCreateTaskFormValues(locale: string): TaskFormValues {
  return {
    title: '',
    objective: '',
    description: '',
    priority: 0,
    status: 'draft',
    taskProfile: 'default',
    default_model_config_id: undefined,
    prerequisite_task_ids: [],
    tagsText: '',
    mcpEnabled: true,
    builtinPromptMode: 'effective',
    builtinPromptLocale: locale,
    enabledBuiltinKinds: [],
    workspaceDir: '',
    defaultRemoteServerId: undefined,
    externalMcpConfigIds: [],
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
    default_model_config_id: task.default_model_config_id || undefined,
    prerequisite_task_ids: task.prerequisite_task_ids || [],
    tagsText: task.tags.join(', '),
    mcpEnabled: task.mcp_config.enabled,
    builtinPromptMode: task.mcp_config.builtin_prompt_mode,
    builtinPromptLocale: task.mcp_config.builtin_prompt_locale,
    enabledBuiltinKinds: task.mcp_config.enabled_builtin_kinds,
    workspaceDir: task.mcp_config.workspace_dir || '',
    defaultRemoteServerId: task.mcp_config.default_remote_server_id || undefined,
    externalMcpConfigIds: task.mcp_config.external_mcp_config_ids || [],
    scheduleMode: task.schedule.mode,
    scheduleRunAt: formatScheduleInput(task.schedule.run_at ?? task.schedule.next_run_at),
    scheduleIntervalSeconds: task.schedule.interval_seconds || undefined,
  };
}

export function buildTaskPayload(
  values: TaskFormValues,
  options: {
    editingTask?: TaskRecord | null;
    routeProjectId?: string;
  },
): CreateTaskPayload | null {
  const schedule = buildSchedulePayload(values);
  if (!schedule) {
    return null;
  }

  const enabledBuiltinKinds = completeEnabledBuiltinKindDependencies(
    values.enabledBuiltinKinds,
  );

  return {
    title: values.title,
    objective: values.objective,
    description: values.description?.trim() || undefined,
    priority: values.priority,
    status: values.status,
    task_profile: values.taskProfile,
    default_model_config_id: values.default_model_config_id,
    project_id: options.editingTask ? undefined : options.routeProjectId,
    prerequisite_task_ids: values.prerequisite_task_ids || [],
    tags: values.tagsText
      ?.split(',')
      .map((item) => item.trim())
      .filter(Boolean),
    schedule,
    mcp_config: {
      enabled: values.mcpEnabled,
      init_mode: 'full',
      builtin_prompt_mode: values.builtinPromptMode,
      builtin_prompt_locale: values.builtinPromptLocale,
      enabled_builtin_kinds: enabledBuiltinKinds,
      workspace_dir: values.workspaceDir?.trim() || undefined,
      default_remote_server_id: values.defaultRemoteServerId,
      external_mcp_config_ids: values.externalMcpConfigIds || [],
    },
  };
}

export const CODE_MAINTAINER_READ_KIND = 'CodeMaintainerRead';
export const CODE_MAINTAINER_WRITE_KIND = 'CodeMaintainerWrite';
export const PROJECT_MANAGEMENT_KIND = 'ProjectManagement';
export const PROJECT_MANAGEMENT_MCP_SERVER_NAME = 'project_management_service';
export const taskProfileValues: TaskProfile[] = ['default', 'chatos_plan'];

export const taskProfileColorMap: Record<TaskProfile, string> = {
  default: 'default',
  chatos_plan: 'geekblue',
};

export function taskProfileLabel(profile: string | undefined, t: TranslateFn): string {
  if (profile === 'chatos_plan') {
    return t('tasks.profile.chatosPlan');
  }
  return t('tasks.profile.default');
}

export function systemInjectedMcpServerNames(
  taskOrProfile: Pick<TaskRecord, 'task_profile'> | TaskProfile | string | undefined,
): string[] {
  const profile =
    typeof taskOrProfile === 'string' || taskOrProfile === undefined
      ? taskOrProfile
      : taskOrProfile.task_profile;
  return profile === 'chatos_plan' ? [PROJECT_MANAGEMENT_MCP_SERVER_NAME] : [];
}

export function completeEnabledBuiltinKindDependencies(values?: string[]): string[] {
  const out: string[] = [];
  (values || []).forEach((value) => {
    const trimmed = value.trim();
    if (trimmed && trimmed !== PROJECT_MANAGEMENT_KIND && !out.includes(trimmed)) {
      out.push(trimmed);
    }
  });

  if (
    out.includes(CODE_MAINTAINER_WRITE_KIND) &&
    !out.includes(CODE_MAINTAINER_READ_KIND)
  ) {
    const writeIndex = out.indexOf(CODE_MAINTAINER_WRITE_KIND);
    out.splice(writeIndex >= 0 ? writeIndex : out.length, 0, CODE_MAINTAINER_READ_KIND);
  }

  return out;
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
