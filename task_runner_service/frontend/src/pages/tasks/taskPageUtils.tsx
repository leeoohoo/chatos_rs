import dayjs from 'dayjs';
import { Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  RemoteServerRecord,
  TaskBuiltinPromptMode,
  TaskMcpInitMode,
  TaskRecord,
  TaskRunEventRecord,
  TaskRunRecord,
  TaskScheduleConfig,
  TaskScheduleMode,
  TaskStatus,
  UiPromptStatus,
} from '../../types';

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
  mcpInitMode: TaskMcpInitMode;
  builtinPromptMode: TaskBuiltinPromptMode;
  builtinPromptLocale: string;
  enabledBuiltinKinds: string[];
  workspaceDir?: string;
  defaultRemoteServerId?: string;
  externalMcpConfigIds?: string[];
  scheduleMode: TaskScheduleMode;
  scheduleRunAt?: string;
  scheduleIntervalSeconds?: number;
};

export type RunTaskFormValues = {
  model_config_id?: string;
  prompt_override?: string;
};

export const statusColorMap: Record<TaskStatus, string> = {
  draft: 'default',
  ready: 'blue',
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

export const promptStatusColorMap: Record<UiPromptStatus, string> = {
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

export type TaskRemoteOperationStats = {
  total: number;
  serverCount: number;
  successCount: number;
  failedCount: number;
};

export function collectTaskRemoteOperations(
  events: TaskRunEventRecord[],
  remoteServerMap: Map<string, RemoteServerRecord>,
): TaskRemoteOperationView[] {
  return events
    .filter((event) => event.event_type === 'tool_stream')
    .map((event) => taskPayloadAsRecord(event.payload))
    .filter((payload): payload is Record<string, unknown> => Boolean(payload))
    .filter((payload) => isTaskRemoteToolName(taskPayloadAsOptionalString(payload.name) || ''))
    .map((payload) => {
      const result = taskPayloadAsRecord(payload.result);
      const nestedResult = taskPayloadAsRecord(result?.result);
      const connectionId = taskPayloadAsOptionalString(result?.connection_id);
      const remoteServer = connectionId ? remoteServerMap.get(connectionId) : undefined;
      const command = taskPayloadAsOptionalString(result?.command);
      const path = taskPayloadAsOptionalString(result?.path);
      const connectionName =
        taskPayloadAsOptionalString(result?.name) || remoteServer?.name;

      return {
        name: taskPayloadAsOptionalString(payload.name) || 'unknown_tool',
        success: Boolean(payload.success) && !Boolean(payload.is_error),
        connectionId,
        connectionName,
        username:
          taskPayloadAsOptionalString(result?.username) || remoteServer?.username,
        host: taskPayloadAsOptionalString(result?.host) || remoteServer?.host,
        port: taskPayloadAsOptionalNumber(result?.port) || remoteServer?.port,
        command,
        path,
        remoteHost: taskPayloadAsOptionalString(nestedResult?.remote_host),
        content: taskPayloadAsOptionalString(payload.content),
        summary: command || path || taskPayloadAsOptionalString(payload.content),
      };
    });
}

export function summarizeTaskRemoteOperations(
  items: TaskRemoteOperationView[],
): TaskRemoteOperationStats {
  const serverIds = new Set(items.map((item) => item.connectionId).filter(Boolean));
  const successCount = items.filter((item) => item.success).length;
  return {
    total: items.length,
    serverCount: serverIds.size,
    successCount,
    failedCount: items.length - successCount,
  };
}

function isTaskRemoteToolName(name: string): boolean {
  return (
    name === 'list_connections' ||
    name === 'test_connection' ||
    name === 'run_command' ||
    name === 'list_directory' ||
    name === 'read_file'
  );
}

function taskPayloadAsRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function taskPayloadAsOptionalString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const text = value.trim();
  return text ? text : undefined;
}

function taskPayloadAsOptionalNumber(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  return undefined;
}

export function formatTaskRemoteEndpoint(
  username?: string,
  host?: string,
  port?: number,
): string | undefined {
  if (!host) {
    return undefined;
  }
  const userPrefix = username ? `${username}@` : '';
  const portSuffix = port ? `:${port}` : '';
  return `${userPrefix}${host}${portSuffix}`;
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
