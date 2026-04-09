import type { ContactTask, TaskPlanOperationResult } from './types';

const BUILTIN_MCP_LABELS: Record<string, string> = {
  builtin_code_maintainer_read: '查看',
  builtin_code_maintainer_write: '读写',
  builtin_task_planner: '任务',
  builtin_terminal_controller: '终端',
  builtin_remote_connection_controller: '远程连接',
  builtin_notepad: 'Notepad',
  builtin_agent_builder: 'Agent Builder',
  builtin_ui_prompter: 'UI Prompter',
  builtin_task_executor: '任务执行',
};

const ASSET_TYPE_LABELS: Record<string, string> = {
  skill: '技能',
  plugin: '插件',
  common: 'Commons',
};

const TASK_STATUS_COLORS: Record<string, string> = {
  pending_confirm: 'orange',
  pending_execute: 'blue',
  running: 'gold',
  paused: 'purple',
  blocked: 'volcano',
  completed: 'green',
  failed: 'red',
  cancelled: 'default',
  skipped: 'default',
};

const BLOCKED_REASON_LABELS: Record<string, string> = {
  waiting_for_dependencies: '等待前置任务完成',
  dependency_missing: '前置任务缺失',
  upstream_terminal_failure: '前置任务已失败、取消或跳过',
};

export const EXECUTION_PAGE_SIZE = 8;

export function formatBuiltinMcpLabel(id: string): string {
  return BUILTIN_MCP_LABELS[id] || id;
}

export function formatAssetTypeLabel(assetType?: string | null): string {
  if (!assetType) {
    return '资产';
  }
  return ASSET_TYPE_LABELS[assetType] || assetType;
}

export function describeSkipOperation(result?: TaskPlanOperationResult): string {
  if (!result) {
    return '节点及其后继已按计划操作跳过';
  }
  if (result.affected_count <= 0) {
    return '没有可跳过的节点，可能它们已经处于运行中或终态';
  }
  return `已跳过 ${result.affected_count} 个节点`;
}

export function describeRewireOperation(result?: TaskPlanOperationResult): string {
  if (!result) {
    return '已更新直接后继的前置依赖';
  }
  if (result.affected_count <= 0) {
    return '没有可调整的直接后继，可能它们已经处于运行中或终态';
  }
  return result.replacement_task_id
    ? `已重挂 ${result.affected_count} 个直接后继到新前置`
    : `已移除 ${result.affected_count} 个直接后继对当前节点的前置依赖`;
}

export function stringifyPretty(value: unknown): string {
  if (value == null) {
    return '';
  }
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function truncateText(value?: string | null, maxLength = 140): string {
  const normalized = (value || '').trim();
  if (!normalized) {
    return '-';
  }
  if (normalized.length <= maxLength) {
    return normalized;
  }
  return `${normalized.slice(0, maxLength)}...`;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

export function extractToolEntries(value: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(value)) {
    return value.filter((item): item is Record<string, unknown> => isObjectRecord(item));
  }
  if (isObjectRecord(value)) {
    return [value];
  }
  return [];
}

export function getToolEntryLabel(entry: Record<string, unknown>, index: number): string {
  const directName = typeof entry.name === 'string' ? entry.name : null;
  const directType = typeof entry.type === 'string' ? entry.type : null;
  const fnBlock = isObjectRecord(entry.function) ? entry.function : null;
  const fnName = fnBlock && typeof fnBlock.name === 'string' ? fnBlock.name : null;
  return fnName || directName || directType || `工具调用 ${index + 1}`;
}

export function formatTaskStatusColor(status?: string | null): string {
  return TASK_STATUS_COLORS[(status || '').trim()] || 'default';
}

export function formatBlockedReason(reason?: string | null): string {
  if (!reason) {
    return '-';
  }
  return BLOCKED_REASON_LABELS[reason] || reason;
}

export function formatHandoffKind(kind?: string | null): string {
  const normalized = (kind || '').trim();
  if (normalized === 'completed') {
    return '完成交接';
  }
  if (normalized === 'failed') {
    return '失败交接';
  }
  if (normalized === 'checkpoint') {
    return '暂停检查点';
  }
  if (normalized === 'cancelled') {
    return '停止交接';
  }
  if (normalized === 'skipped') {
    return '跳过交接';
  }
  return normalized || '-';
}

export function buildPlanImpact(items: ContactTask[]): {
  directDependentsByTaskId: Record<string, string[]>;
  descendantIdsByTaskId: Record<string, string[]>;
} {
  const directDependentsByTaskId: Record<string, string[]> = {};
  for (const task of items) {
    for (const dependencyTaskId of task.depends_on_task_ids || []) {
      const existing = directDependentsByTaskId[dependencyTaskId] || [];
      if (!existing.includes(task.id)) {
        existing.push(task.id);
      }
      directDependentsByTaskId[dependencyTaskId] = existing;
    }
  }

  const descendantIdsByTaskId: Record<string, string[]> = {};
  const collectDescendants = (taskId: string, seen = new Set<string>()): string[] => {
    const nextIds = directDependentsByTaskId[taskId] || [];
    for (const nextId of nextIds) {
      if (seen.has(nextId)) {
        continue;
      }
      seen.add(nextId);
      collectDescendants(nextId, seen);
    }
    return Array.from(seen);
  };

  for (const task of items) {
    descendantIdsByTaskId[task.id] = collectDescendants(task.id);
  }

  return {
    directDependentsByTaskId,
    descendantIdsByTaskId,
  };
}
