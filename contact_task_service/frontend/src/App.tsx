import {
  LogoutOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import {
  Alert,
  App as AntdApp,
  Button,
  Card,
  Collapse,
  ConfigProvider,
  Input,
  Layout,
  Pagination,
  Select,
  Space,
  Spin,
  Table,
  Tag,
  Typography,
} from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { useEffect, useMemo, useState } from 'react';

import { api, authStore } from './api';
import type {
  AuthUser,
  ContactTask,
  MemoryContactSummary,
  MemoryProjectSummary,
  TaskExecutionMessage,
  TaskPlanOperationResult,
  TaskPlanView,
  TaskResultBrief,
} from './types';

const { Header, Content } = Layout;
const { Title, Text, Paragraph } = Typography;

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

const EXECUTION_PAGE_SIZE = 8;
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

const TERMINAL_PLAN_STATUSES = new Set(['completed', 'failed', 'cancelled', 'skipped']);

function formatBuiltinMcpLabel(id: string): string {
  return BUILTIN_MCP_LABELS[id] || id;
}

function formatAssetTypeLabel(assetType?: string | null): string {
  if (!assetType) {
    return '资产';
  }
  return ASSET_TYPE_LABELS[assetType] || assetType;
}

function describeSkipOperation(result?: TaskPlanOperationResult): string {
  if (!result) {
    return '节点及其后继已按计划操作跳过';
  }
  if (result.affected_count <= 0) {
    return '没有可跳过的节点，可能它们已经处于运行中或终态';
  }
  return `已跳过 ${result.affected_count} 个节点`;
}

function describeRewireOperation(result?: TaskPlanOperationResult): string {
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

function stringifyPretty(value: unknown): string {
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

function truncateText(value?: string | null, maxLength = 140): string {
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

function extractToolEntries(value: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(value)) {
    return value.filter((item): item is Record<string, unknown> => isObjectRecord(item));
  }
  if (isObjectRecord(value)) {
    return [value];
  }
  return [];
}

function getToolEntryLabel(entry: Record<string, unknown>, index: number): string {
  const directName = typeof entry.name === 'string' ? entry.name : null;
  const directType = typeof entry.type === 'string' ? entry.type : null;
  const fnBlock = isObjectRecord(entry.function) ? entry.function : null;
  const fnName = fnBlock && typeof fnBlock.name === 'string' ? fnBlock.name : null;
  return fnName || directName || directType || `工具调用 ${index + 1}`;
}

function formatTaskStatusColor(status?: string | null): string {
  return TASK_STATUS_COLORS[(status || '').trim()] || 'default';
}

function formatBlockedReason(reason?: string | null): string {
  if (!reason) {
    return '-';
  }
  return BLOCKED_REASON_LABELS[reason] || reason;
}

function formatHandoffKind(kind?: string | null): string {
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

function App() {
  const { message } = AntdApp.useApp();
  const [authUser, setAuthUser] = useState<AuthUser | null>(null);
  const [bootLoading, setBootLoading] = useState(true);
  const [loading, setLoading] = useState(false);
  const [loginLoading, setLoginLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('admin');
  const [tasks, setTasks] = useState<ContactTask[]>([]);
  const [taskPlans, setTaskPlans] = useState<TaskPlanView[]>([]);
  const [status, setStatus] = useState<string | undefined>(undefined);
  const [selectedPlanId, setSelectedPlanId] = useState<string | null>(null);
  const [selectedPlanDetails, setSelectedPlanDetails] = useState<TaskPlanView | null>(null);
  const [userIdFilter, setUserIdFilter] = useState('');
  const [contactAgentId, setContactAgentId] = useState('');
  const [projectId, setProjectId] = useState('');
  const [executionMessagesByTaskId, setExecutionMessagesByTaskId] = useState<Record<string, TaskExecutionMessage[]>>({});
  const [executionLoadingByTaskId, setExecutionLoadingByTaskId] = useState<Record<string, boolean>>({});
  const [executionErrorByTaskId, setExecutionErrorByTaskId] = useState<Record<string, string | null>>({});
  const [resultBriefByTaskId, setResultBriefByTaskId] = useState<Record<string, TaskResultBrief | null | undefined>>({});
  const [resultBriefLoadingByTaskId, setResultBriefLoadingByTaskId] = useState<Record<string, boolean>>({});
  const [resultBriefErrorByTaskId, setResultBriefErrorByTaskId] = useState<Record<string, string | null>>({});
  const [expandedTaskIds, setExpandedTaskIds] = useState<string[]>([]);
  const [executionSectionExpandedByTaskId, setExecutionSectionExpandedByTaskId] = useState<Record<string, boolean>>({});
  const [executionPageByTaskId, setExecutionPageByTaskId] = useState<Record<string, number>>({});
  const [contactNameByScopeKey, setContactNameByScopeKey] = useState<Record<string, string>>({});
  const [projectNameByScopeKey, setProjectNameByScopeKey] = useState<Record<string, string>>({});
  const [planActionLoading, setPlanActionLoading] = useState(false);
  const [planRelationDraftsByTaskId, setPlanRelationDraftsByTaskId] = useState<Record<string, {
    dependsOnRefs: string;
    verificationOfRefs: string;
  }>>({});
  const [planRewireTargetByTaskId, setPlanRewireTargetByTaskId] = useState<Record<string, string>>({});

  useEffect(() => {
    const init = async () => {
      if (!authStore.getToken()) {
        setBootLoading(false);
        return;
      }
      try {
        const me = await api.me();
        setAuthUser(me);
      } catch {
        authStore.clear();
      } finally {
        setBootLoading(false);
      }
    };
    void init();
  }, []);

  const isAdmin = authUser?.role === 'admin';

  const refresh = async () => {
    if (!authUser) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const params = {
        user_id: isAdmin ? userIdFilter.trim() || undefined : undefined,
        contact_agent_id: contactAgentId.trim() || undefined,
        project_id: projectId.trim() || undefined,
        status,
      };
      const [rows, planRows] = await Promise.all([
        api.listTasks(params),
        api.listTaskPlans(params),
      ]);
      setTasks(rows);
      setTaskPlans(planRows);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (authUser) {
      void refresh();
    }
  }, [authUser]);

  useEffect(() => {
    if (!authUser || tasks.length === 0) {
      setContactNameByScopeKey({});
      setProjectNameByScopeKey({});
      return;
    }

    let cancelled = false;

    const loadScopeDisplayNames = async () => {
      const userIds = Array.from(
        new Set(
          tasks
            .map((task) => task.user_id?.trim())
            .filter((value): value is string => Boolean(value)),
        ),
      );

      const contactEntries = await Promise.all(
        userIds.map(async (userId) => {
          try {
            const rows = await api.listMemoryContacts(userId);
            return rows.map((item) => {
              const scopeKey = `${item.user_id}:${item.agent_id}`;
              const displayName = item.agent_name_snapshot?.trim() || item.agent_id;
              return [scopeKey, displayName] as const;
            });
          } catch {
            return [] as Array<readonly [string, string]>;
          }
        }),
      );

      const projectEntries = await Promise.all(
        userIds.map(async (userId) => {
          try {
            const rows = await api.listMemoryProjects(userId);
            return rows.map((item) => {
              const scopeKey = `${item.user_id}:${item.project_id}`;
              const displayName = item.name?.trim() || item.project_id;
              return [scopeKey, displayName] as const;
            });
          } catch {
            return [] as Array<readonly [string, string]>;
          }
        }),
      );

      if (cancelled) {
        return;
      }

      setContactNameByScopeKey(Object.fromEntries(contactEntries.flat()));
      setProjectNameByScopeKey(Object.fromEntries(projectEntries.flat()));
    };

    void loadScopeDisplayNames();

    return () => {
      cancelled = true;
    };
  }, [authUser, tasks]);

  const doLogin = async () => {
    setLoginLoading(true);
    setError(null);
    try {
      const data = await api.login(username.trim(), password);
      authStore.setToken(data.token);
      setAuthUser({ username: data.username, role: data.role });
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoginLoading(false);
    }
  };

  const logout = () => {
    authStore.clear();
    setAuthUser(null);
    setTasks([]);
    setTaskPlans([]);
    setExpandedTaskIds([]);
    setExecutionMessagesByTaskId({});
    setExecutionLoadingByTaskId({});
    setExecutionErrorByTaskId({});
    setResultBriefByTaskId({});
    setResultBriefLoadingByTaskId({});
    setResultBriefErrorByTaskId({});
    setExecutionSectionExpandedByTaskId({});
    setExecutionPageByTaskId({});
    setContactNameByScopeKey({});
    setProjectNameByScopeKey({});
  };

  const getContactDisplayName = useMemo(() => (
    (record: ContactTask): string => {
      const scopeKey = `${record.user_id}:${record.contact_agent_id}`;
      return contactNameByScopeKey[scopeKey] || record.contact_agent_id || '-';
    }
  ), [contactNameByScopeKey]);

  const getProjectDisplayName = useMemo(() => (
    (record: ContactTask): string => {
      if (record.project_id === '0') {
        return '未指定项目';
      }
      const scopeKey = `${record.user_id}:${record.project_id}`;
      return projectNameByScopeKey[scopeKey] || record.project_id || '-';
    }
  ), [projectNameByScopeKey]);

  const taskLookupById = useMemo(() => {
    const entries = tasks.map((task) => [task.id, task] as const);
    return new Map<string, ContactTask>(entries);
  }, [tasks]);

  const sortedTasks = useMemo(() => {
    const items = [...tasks];
    items.sort((left, right) => {
      const leftPlan = left.task_plan_id || left.id;
      const rightPlan = right.task_plan_id || right.id;
      if (leftPlan !== rightPlan) {
        const rightUpdated = Date.parse(right.updated_at) || 0;
        const leftUpdated = Date.parse(left.updated_at) || 0;
        return rightUpdated - leftUpdated;
      }
      const leftQueue = typeof left.queue_position === 'number' ? left.queue_position : 0;
      const rightQueue = typeof right.queue_position === 'number' ? right.queue_position : 0;
      if (leftQueue !== rightQueue) {
        return leftQueue - rightQueue;
      }
      const leftCreated = Date.parse(left.created_at) || 0;
      const rightCreated = Date.parse(right.created_at) || 0;
      return leftCreated - rightCreated;
    });
    return items;
  }, [tasks]);

  const visibleTasks = useMemo(() => {
    const normalizedPlanId = (selectedPlanId || '').trim();
    if (!normalizedPlanId) {
      return sortedTasks;
    }
    return sortedTasks.filter((task) => (task.task_plan_id || task.id).trim() === normalizedPlanId);
  }, [selectedPlanId, sortedTasks]);

  const selectedPlan = useMemo(() => {
    const normalizedPlanId = (selectedPlanId || '').trim();
    if (!normalizedPlanId) {
      return null;
    }
    if (selectedPlanDetails && selectedPlanDetails.plan_id === normalizedPlanId) {
      return {
        planId: selectedPlanDetails.plan_id,
        items: selectedPlanDetails.tasks,
        title: selectedPlanDetails.title,
        taskCount: selectedPlanDetails.task_count,
        statusCounts: selectedPlanDetails.status_counts,
        latestUpdatedAt: Date.parse(selectedPlanDetails.latest_updated_at) || 0,
        activeTaskId: selectedPlanDetails.active_task_id || null,
        blockedTaskCount: selectedPlanDetails.blocked_task_count,
      };
    }
    const localPlan = taskPlans.find((plan) => plan.plan_id === normalizedPlanId) || null;
    if (!localPlan) {
      return null;
    }
    return {
      planId: localPlan.plan_id,
      items: localPlan.tasks,
      title: localPlan.title,
      taskCount: localPlan.task_count,
      statusCounts: localPlan.status_counts,
      latestUpdatedAt: Date.parse(localPlan.latest_updated_at) || 0,
      activeTaskId: localPlan.active_task_id || null,
      blockedTaskCount: localPlan.blocked_task_count,
    };
  }, [selectedPlanDetails, selectedPlanId, taskPlans]);

  const formatRelatedTask = useMemo(() => (
    (taskId: string): string => {
      const normalizedId = taskId.trim();
      if (!normalizedId) {
        return '-';
      }
      const target = taskLookupById.get(normalizedId);
      if (!target) {
        return normalizedId;
      }
      const taskRef = target.task_ref?.trim();
      const refPart = taskRef ? ` · ${taskRef}` : '';
      return `${target.title}${refPart} (${normalizedId.slice(0, 8)})`;
    }
  ), [taskLookupById]);

  const formatPlanNodeLabel = useMemo(() => (
    (task: ContactTask): string => {
      const taskRef = task.task_ref?.trim();
      return taskRef ? `${task.title} · ${taskRef}` : task.title;
    }
  ), []);

  const selectedPlanImpact = useMemo(() => {
    if (!selectedPlan) {
      return {
        directDependentsByTaskId: {} as Record<string, string[]>,
        descendantIdsByTaskId: {} as Record<string, string[]>,
      };
    }
    const directDependentsByTaskId: Record<string, string[]> = {};
    for (const task of selectedPlan.items) {
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

    for (const task of selectedPlan.items) {
      descendantIdsByTaskId[task.id] = collectDescendants(task.id);
    }

    return {
      directDependentsByTaskId,
      descendantIdsByTaskId,
    };
  }, [selectedPlan]);

  const focusTaskPlan = (planId: string) => {
    const normalizedPlanId = planId.trim();
    if (!normalizedPlanId) {
      return;
    }
    setSelectedPlanId(normalizedPlanId);
    const plan = taskPlans.find((item) => item.plan_id === normalizedPlanId);
    if (plan) {
      setExpandedTaskIds(plan.tasks.map((item) => item.id));
    }
  };

  const clearTaskPlanFocus = () => {
    setSelectedPlanId(null);
    setSelectedPlanDetails(null);
    setPlanRelationDraftsByTaskId({});
    setPlanRewireTargetByTaskId({});
  };

  const updatePlanOrdering = async (planId: string, orderedTaskIds: string[]) => {
    setPlanActionLoading(true);
    setError(null);
    try {
      await api.updateTaskPlan(planId, { ordered_task_ids: orderedTaskIds });
      await refresh();
      setSelectedPlanId(planId);
      setExpandedTaskIds(orderedTaskIds);
      message.success('计划顺序已更新');
    } catch (err) {
      const detail = (err as Error).message;
      setError(detail);
      message.error(detail);
    } finally {
      setPlanActionLoading(false);
    }
  };

  const movePlanTask = async (planId: string, taskId: string, direction: -1 | 1) => {
    const plan = await api.getTaskPlan(planId);
    if (!plan) {
      message.error('未找到对应的任务计划');
      return;
    }
    const orderedTaskIds = plan.tasks.map((item) => item.id);
    const currentIndex = orderedTaskIds.findIndex((item) => item === taskId);
    const targetIndex = currentIndex + direction;
    if (currentIndex < 0 || targetIndex < 0 || targetIndex >= orderedTaskIds.length) {
      return;
    }
    const nextOrder = [...orderedTaskIds];
    const [movedTaskId] = nextOrder.splice(currentIndex, 1);
    nextOrder.splice(targetIndex, 0, movedTaskId);
    await updatePlanOrdering(planId, nextOrder);
  };

  const setPlanTaskStatus = async (
    planId: string,
    taskId: string,
    status: 'cancelled' | 'skipped',
    successMessage: string,
  ) => {
    setPlanActionLoading(true);
    setError(null);
    try {
      await api.updateTaskPlan(planId, {
        updates: [
          {
            task_id: taskId,
            status,
            blocked_reason: null,
          },
        ],
      });
      await refresh();
      setSelectedPlanId(planId);
      message.success(successMessage);
    } catch (err) {
      const detail = (err as Error).message;
      setError(detail);
      message.error(detail);
    } finally {
      setPlanActionLoading(false);
    }
  };

  const cascadeSkipPlanTask = async (planId: string, taskId: string) => {
    setPlanActionLoading(true);
    setError(null);
    try {
      const response = await api.updateTaskPlan(planId, {
        operations: [
          {
            kind: 'skip_with_descendants',
            task_id: taskId,
          },
        ],
      });
      await refresh();
      setSelectedPlanId(planId);
      message.success(describeSkipOperation(response?.operation_results?.[0]));
    } catch (err) {
      const detail = (err as Error).message;
      setError(detail);
      message.error(detail);
    } finally {
      setPlanActionLoading(false);
    }
  };

  const rewireDirectDependents = async (planId: string, sourceTaskId: string) => {
    const replacementRaw = (planRewireTargetByTaskId[sourceTaskId] || '').trim();
    const replacementTaskId = replacementRaw && replacementRaw !== '__remove__'
      ? replacementRaw
      : null;

    setPlanActionLoading(true);
    setError(null);
    try {
      const response = await api.updateTaskPlan(planId, {
        operations: [
          {
            kind: 'rewire_direct_dependents',
            task_id: sourceTaskId,
            replacement_task_id: replacementTaskId,
          },
        ],
      });
      await refresh();
      setSelectedPlanId(planId);
      message.success(describeRewireOperation(response?.operation_results?.[0]));
    } catch (err) {
      const detail = (err as Error).message;
      setError(detail);
      message.error(detail);
    } finally {
      setPlanActionLoading(false);
    }
  };

  const savePlanTaskLinks = async (planId: string, taskId: string) => {
    const plan = await api.getTaskPlan(planId);
    if (!plan) {
      message.error('未找到对应的任务计划');
      return;
    }
    const relationDraft = planRelationDraftsByTaskId[taskId] || {
      dependsOnRefs: '',
      verificationOfRefs: '',
    };
    const refLookup = new Map<string, string>();
    for (const item of plan.tasks) {
      refLookup.set(item.id, item.id);
      refLookup.set(item.id.slice(0, 8), item.id);
      const taskRef = item.task_ref?.trim();
      if (taskRef) {
        refLookup.set(taskRef, item.id);
      }
    }
    const parseReferences = (value: string) => (
      value
        .split(/[\n,，]/)
        .map((item) => item.trim())
        .filter((item, index, arr) => Boolean(item) && arr.indexOf(item) === index)
    );
    const dependsOnRefs = parseReferences(relationDraft.dependsOnRefs);
    const verificationOfRefs = parseReferences(relationDraft.verificationOfRefs);
    const unresolvedDepends = dependsOnRefs.filter((item) => !refLookup.has(item));
    const unresolvedVerification = verificationOfRefs.filter((item) => !refLookup.has(item));
    if (unresolvedDepends.length > 0 || unresolvedVerification.length > 0) {
      const problems = [
        unresolvedDepends.length > 0 ? `未识别的前置引用: ${unresolvedDepends.join(' / ')}` : '',
        unresolvedVerification.length > 0 ? `未识别的验证引用: ${unresolvedVerification.join(' / ')}` : '',
      ].filter(Boolean);
      const detail = problems.join('；');
      setError(detail);
      message.error(detail);
      return;
    }

    setPlanActionLoading(true);
    setError(null);
    try {
      await api.updateTaskPlan(planId, {
        updates: [
          {
            task_id: taskId,
            depends_on_task_ids: dependsOnRefs.map((item) => refLookup.get(item) || item),
            verification_of_task_ids: verificationOfRefs.map((item) => refLookup.get(item) || item),
          },
        ],
      });
      await refresh();
      setSelectedPlanId(planId);
      message.success('节点依赖已更新');
    } catch (err) {
      const detail = (err as Error).message;
      setError(detail);
      message.error(detail);
    } finally {
      setPlanActionLoading(false);
    }
  };

  const loadExecutionMessages = async (taskId: string, force = false) => {
    if (!taskId) {
      return;
    }
    if (!force && (executionLoadingByTaskId[taskId] || executionMessagesByTaskId[taskId])) {
      return;
    }
    setExecutionLoadingByTaskId((prev) => ({ ...prev, [taskId]: true }));
    setExecutionErrorByTaskId((prev) => ({ ...prev, [taskId]: null }));
    try {
      const items = await api.listTaskExecutionMessages(taskId);
      setExecutionMessagesByTaskId((prev) => ({ ...prev, [taskId]: items }));
    } catch (err) {
      setExecutionErrorByTaskId((prev) => ({
        ...prev,
        [taskId]: (err as Error).message,
      }));
    } finally {
      setExecutionLoadingByTaskId((prev) => ({ ...prev, [taskId]: false }));
    }
  };

  const loadTaskResultBrief = async (taskId: string, force = false) => {
    if (!taskId) {
      return;
    }
    if (!force && (resultBriefLoadingByTaskId[taskId] || Object.prototype.hasOwnProperty.call(resultBriefByTaskId, taskId))) {
      return;
    }
    setResultBriefLoadingByTaskId((prev) => ({ ...prev, [taskId]: true }));
    setResultBriefErrorByTaskId((prev) => ({ ...prev, [taskId]: null }));
    try {
      const item = await api.getTaskResultBrief(taskId);
      setResultBriefByTaskId((prev) => ({ ...prev, [taskId]: item }));
    } catch (err) {
      setResultBriefErrorByTaskId((prev) => ({
        ...prev,
        [taskId]: (err as Error).message,
      }));
    } finally {
      setResultBriefLoadingByTaskId((prev) => ({ ...prev, [taskId]: false }));
    }
  };

  useEffect(() => {
    if (!authUser || expandedTaskIds.length === 0) {
      return;
    }
    for (const taskId of expandedTaskIds) {
      void loadExecutionMessages(taskId, true);
      void loadTaskResultBrief(taskId, true);
    }
  }, [tasks]);

  useEffect(() => {
    if (!authUser || !selectedPlanId) {
      setSelectedPlanDetails(null);
      return;
    }
    let cancelled = false;
    const loadSelectedPlan = async () => {
      try {
        const item = await api.getTaskPlan(selectedPlanId);
        if (!cancelled) {
          setSelectedPlanDetails(item);
        }
      } catch {
        if (!cancelled) {
          setSelectedPlanDetails(null);
        }
      }
    };
    void loadSelectedPlan();
    return () => {
      cancelled = true;
    };
  }, [authUser, selectedPlanId, tasks]);

  useEffect(() => {
    if (!selectedPlan) {
      setPlanRelationDraftsByTaskId({});
      setPlanRewireTargetByTaskId({});
      return;
    }
    const nextDrafts = Object.fromEntries(selectedPlan.items.map((task) => {
      const dependsOnRefs = (task.depends_on_task_ids || []).map((taskId) => {
        const target = selectedPlan.items.find((item) => item.id === taskId);
        return target?.task_ref?.trim() || target?.id.slice(0, 8) || taskId;
      }).join(', ');
      const verificationOfRefs = (task.verification_of_task_ids || []).map((taskId) => {
        const target = selectedPlan.items.find((item) => item.id === taskId);
        return target?.task_ref?.trim() || target?.id.slice(0, 8) || taskId;
      }).join(', ');
      return [
        task.id,
        {
          dependsOnRefs,
          verificationOfRefs,
        },
      ] as const;
    }));
    setPlanRelationDraftsByTaskId(nextDrafts);
    setPlanRewireTargetByTaskId(Object.fromEntries(
      selectedPlan.items.map((task) => [task.id, '__remove__'] as const),
    ));
  }, [selectedPlan]);

  useEffect(() => {
    if (!authUser) {
      return;
    }
    const runningExpandedTaskIds = expandedTaskIds.filter((taskId) =>
      tasks.some((task) => task.id === taskId && task.status === 'running'),
    );
    if (runningExpandedTaskIds.length === 0) {
      return;
    }
    const timer = window.setInterval(() => {
      void refresh();
      for (const taskId of runningExpandedTaskIds) {
        void loadExecutionMessages(taskId, true);
        void loadTaskResultBrief(taskId, true);
      }
    }, 5000);
    return () => window.clearInterval(timer);
  }, [authUser, expandedTaskIds, tasks, status, userIdFilter, contactAgentId, projectId]);

  const columns = useMemo(
    () => [
      {
        title: '任务',
        dataIndex: 'title',
        key: 'title',
        render: (value: string, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text strong>{value}</Text>
            <Text type="secondary">
              {record.id.slice(0, 8)}
              {typeof record.queue_position === 'number' ? ` · 队列 ${record.queue_position}` : ''}
            </Text>
          </Space>
        ),
      },
      {
        title: '用户 / 联系人 / 项目',
        key: 'scope',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>用户: {record.user_id}</Text>
            <Text>
              联系人:
              {' '}
              {getContactDisplayName(record)}
            </Text>
            <Text type="secondary">
              项目:
              {' '}
              {getProjectDisplayName(record)}
            </Text>
            <Text type="secondary">
              IDs:
              {' '}
              {record.contact_agent_id}
              {' / '}
              {record.project_id}
            </Text>
          </Space>
        ),
      },
      {
        title: '计划 / 图谱',
        key: 'plan',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>{record.task_plan_id || '-'}</Text>
            <Text type="secondary">
              {record.task_ref || '-'}
              {record.task_kind ? ` · ${record.task_kind}` : ''}
            </Text>
            <Text type="secondary">
              依赖:
              {' '}
              {record.depends_on_task_ids?.length ?? 0}
              {' / '}
              验证:
              {' '}
              {record.verification_of_task_ids?.length ?? 0}
            </Text>
          </Space>
        ),
      },
      {
        title: '优先级',
        dataIndex: 'priority',
        key: 'priority',
        render: (value: string) => (
          <Tag color={value === 'high' ? 'red' : value === 'low' ? 'default' : 'blue'}>
            {value}
          </Tag>
        ),
      },
      {
        title: '状态',
        dataIndex: 'status',
        key: 'status',
        render: (value: string, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Tag color={formatTaskStatusColor(value)}>{value}</Tag>
            {record.blocked_reason ? (
              <Text type="secondary">{formatBlockedReason(record.blocked_reason)}</Text>
            ) : null}
          </Space>
        ),
      },
      {
        title: '执行模型',
        dataIndex: 'model_config_id',
        key: 'model_config_id',
        render: (value?: string | null) => value || '-',
      },
      {
        title: '计划资源',
        key: 'planned_resources',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>{record.planned_builtin_mcp_ids?.length ?? 0} 个 MCP</Text>
            <Text type="secondary">{record.planned_context_assets?.length ?? 0} 个上下文资产</Text>
          </Space>
        ),
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        key: 'updated_at',
        render: (value: string) => new Date(value).toLocaleString(),
      },
    ],
    [getContactDisplayName, getProjectDisplayName],
  );

  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          colorPrimary: '#8b3a2e',
          borderRadius: 12,
          fontFamily: '"IBM Plex Sans","Noto Sans SC",sans-serif',
        },
      }}
    >
      <AntdApp>
        {bootLoading ? (
          <div className="auth-wrap">
            <Spin />
          </div>
        ) : !authUser ? (
          <div className="auth-wrap">
            <Card title="任务服务登录" style={{ width: 380 }}>
              <Space direction="vertical" size={12} style={{ width: '100%' }}>
                {error && <Alert type="error" showIcon message={error} />}
                <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="用户名" />
                <Input.Password
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="密码"
                  onPressEnter={doLogin}
                />
                <Button type="primary" loading={loginLoading} onClick={doLogin}>
                  登录
                </Button>
              </Space>
            </Card>
          </div>
        ) : (
          <Layout className="page-shell">
            <Header style={{ background: 'transparent', padding: 0, marginBottom: 20 }}>
              <Card>
                <Space style={{ width: '100%', justifyContent: 'space-between' }} wrap>
                  <div>
                    <Title level={3} style={{ margin: 0 }}>Contact Task Service</Title>
                    <Text type="secondary">
                      任务域看板，AI 执行仍由 agent_orchestrator 负责。
                    </Text>
                  </div>
                  <Space>
                    <Tag color={isAdmin ? 'gold' : 'default'}>
                      {authUser.username} ({authUser.role})
                    </Tag>
                    <Button icon={<LogoutOutlined />} onClick={logout}>退出</Button>
                  </Space>
                </Space>
              </Card>
            </Header>
            <Content>
              <Card
                title="任务看板"
                extra={(
                  <Space wrap>
                    {isAdmin && (
                      <Input
                        value={userIdFilter}
                        onChange={(e) => setUserIdFilter(e.target.value)}
                        placeholder="筛选 user_id"
                        style={{ width: 180 }}
                      />
                    )}
                    <Input
                      value={contactAgentId}
                      onChange={(e) => setContactAgentId(e.target.value)}
                      placeholder="筛选 contact_agent_id"
                      style={{ width: 210 }}
                    />
                    <Input
                      value={projectId}
                      onChange={(e) => setProjectId(e.target.value)}
                      placeholder="筛选 project_id"
                      style={{ width: 180 }}
                    />
                    <Select
                      allowClear
                      value={status}
                      onChange={(value) => setStatus(value)}
                      placeholder="状态"
                      style={{ width: 180 }}
                      options={[
                        { value: 'pending_confirm', label: 'pending_confirm' },
                        { value: 'pending_execute', label: 'pending_execute' },
                        { value: 'running', label: 'running' },
                        { value: 'paused', label: 'paused' },
                        { value: 'blocked', label: 'blocked' },
                        { value: 'completed', label: 'completed' },
                        { value: 'failed', label: 'failed' },
                        { value: 'cancelled', label: 'cancelled' },
                        { value: 'skipped', label: 'skipped' },
                      ]}
                    />
                    <Button type="primary" icon={<ReloadOutlined />} loading={loading} onClick={() => { void refresh(); }}>
                      刷新
                    </Button>
                  </Space>
                )}
              >
                {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
                <Paragraph type="secondary" style={{ marginTop: 0 }}>
                  admin 可看所有任务；普通用户仅能看到自己的任务。调度接口在后端提供，不在看板前端直接触发。
                </Paragraph>
                {selectedPlanId && (
                  <Alert
                    type="info"
                    showIcon
                    style={{ marginBottom: 12 }}
                    message={`当前只看计划 ${selectedPlanId}`}
                    action={(
                      <Button size="small" onClick={clearTaskPlanFocus}>
                        清除计划筛选
                      </Button>
                    )}
                  />
                )}
                {selectedPlan && (
                  <Card
                    size="small"
                    style={{ marginBottom: 16, borderColor: '#d9b08c', background: '#fffaf5' }}
                    bodyStyle={{ padding: 16 }}
                  >
                    <Space direction="vertical" size={12} style={{ width: '100%' }}>
                      <Space wrap style={{ justifyContent: 'space-between', width: '100%' }}>
                        <Space wrap>
                          <Tag color="geekblue">{selectedPlan.planId}</Tag>
                          <Tag>{`${selectedPlan.taskCount} 个节点`}</Tag>
                          <Tag>{`已交接 ${selectedPlan.items.filter((task) => Boolean(task.handoff_payload?.summary)).length}`}</Tag>
                          <Tag color="volcano">
                            {`阻塞 ${selectedPlan.blockedTaskCount}`}
                          </Tag>
                        </Space>
                        <Space wrap>
                          <Button size="small" onClick={() => setExpandedTaskIds(selectedPlan.items.map((item) => item.id))}>
                            展开此计划全部任务
                          </Button>
                          <Button size="small" onClick={clearTaskPlanFocus}>
                            退出计划详情
                          </Button>
                        </Space>
                      </Space>
                      <Text strong>{selectedPlan.title}</Text>
                      <Space wrap>
                        {Object.entries(selectedPlan.statusCounts).map(([statusKey, count]) => (
                          <Tag key={`selected-plan-${statusKey}`} color={formatTaskStatusColor(statusKey)}>
                            {`${statusKey}: ${count}`}
                          </Tag>
                        ))}
                      </Space>
                      {selectedPlan.activeTaskId ? (
                        <Text type="secondary">
                          当前活跃节点:
                          {' '}
                          {formatRelatedTask(selectedPlan.activeTaskId)}
                        </Text>
                      ) : (
                        <Text type="secondary">当前活跃节点: 无</Text>
                      )}
                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                        {selectedPlan.items.map((task, index) => (
                          <Card key={`selected-plan-node-${task.id}`} size="small" bodyStyle={{ padding: 12 }}>
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              <Space wrap style={{ justifyContent: 'space-between', width: '100%' }}>
                                <Space wrap>
                                  <Tag color="geekblue">{`节点 ${index + 1}`}</Tag>
                                  <Tag color={formatTaskStatusColor(task.status)}>{task.status}</Tag>
                                  {task.task_kind ? <Tag color="purple">{task.task_kind}</Tag> : null}
                                  {task.task_ref ? <Tag>{task.task_ref}</Tag> : null}
                                  {typeof task.queue_position === 'number' ? <Tag>{`队列 ${task.queue_position}`}</Tag> : null}
                                </Space>
                                <Space wrap>
                                  <Button
                                    size="small"
                                    disabled={planActionLoading || index === 0}
                                    onClick={() => { void movePlanTask(selectedPlan.planId, task.id, -1); }}
                                  >
                                    上移
                                  </Button>
                                  <Button
                                    size="small"
                                    disabled={planActionLoading || index === selectedPlan.items.length - 1}
                                    onClick={() => { void movePlanTask(selectedPlan.planId, task.id, 1); }}
                                  >
                                    下移
                                  </Button>
                                  <Button
                                    size="small"
                                    danger
                                    disabled={planActionLoading || ['running', 'completed', 'failed', 'cancelled', 'skipped'].includes(task.status)}
                                    onClick={() => {
                                      void setPlanTaskStatus(selectedPlan.planId, task.id, 'skipped', '节点已跳过');
                                    }}
                                  >
                                    跳过节点
                                  </Button>
                                  <Button
                                    size="small"
                                    disabled={
                                      planActionLoading
                                      || ['running', 'completed', 'failed', 'cancelled', 'skipped'].includes(task.status)
                                      || (selectedPlanImpact.descendantIdsByTaskId[task.id]?.length || 0) === 0
                                    }
                                    onClick={() => {
                                      void cascadeSkipPlanTask(selectedPlan.planId, task.id);
                                    }}
                                  >
                                    级联跳过后继
                                  </Button>
                                </Space>
                              </Space>
                              <Text strong>{formatPlanNodeLabel(task)}</Text>
                              <Text type="secondary">{truncateText(task.content, 160)}</Text>
                              {(task.depends_on_task_ids?.length ?? 0) > 0 ? (
                                <Text type="secondary">
                                  {`前置任务: ${task.depends_on_task_ids.map((taskId) => formatRelatedTask(taskId)).join(' / ')}`}
                                </Text>
                              ) : (
                                <Text type="secondary">前置任务: 无</Text>
                              )}
                              {(task.verification_of_task_ids?.length ?? 0) > 0 ? (
                                <Text type="secondary">
                                  {`验证对象: ${task.verification_of_task_ids.map((taskId) => formatRelatedTask(taskId)).join(' / ')}`}
                                </Text>
                              ) : null}
                              {(selectedPlanImpact.descendantIdsByTaskId[task.id]?.length || 0) > 0 ? (
                                <Text type="secondary">
                                  {`受影响后继: ${selectedPlanImpact.descendantIdsByTaskId[task.id].map((taskId) => formatRelatedTask(taskId)).join(' / ')}`}
                                </Text>
                              ) : (
                                <Text type="secondary">受影响后继: 无</Text>
                              )}
                              {(selectedPlanImpact.directDependentsByTaskId[task.id]?.length || 0) > 0 ? (
                                <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                  <Text type="secondary">
                                    {`直接后继: ${selectedPlanImpact.directDependentsByTaskId[task.id].map((taskId) => formatRelatedTask(taskId)).join(' / ')}`}
                                  </Text>
                                  <Space wrap style={{ width: '100%' }}>
                                    <Select
                                      size="small"
                                      style={{ minWidth: 280 }}
                                      value={planRewireTargetByTaskId[task.id] || '__remove__'}
                                      onChange={(value) => {
                                        setPlanRewireTargetByTaskId((prev) => ({
                                          ...prev,
                                          [task.id]: value,
                                        }));
                                      }}
                                      disabled={planActionLoading}
                                      options={[
                                        { value: '__remove__', label: '移除这个前置依赖' },
                                        ...selectedPlan.items
                                          .filter((candidate) =>
                                            candidate.id !== task.id
                                            && !(selectedPlanImpact.descendantIdsByTaskId[task.id] || []).includes(candidate.id))
                                          .map((candidate) => ({
                                            value: candidate.id,
                                            label: formatPlanNodeLabel(candidate),
                                          })),
                                      ]}
                                    />
                                    <Button
                                      size="small"
                                      disabled={planActionLoading}
                                      onClick={() => {
                                        void rewireDirectDependents(selectedPlan.planId, task.id);
                                      }}
                                    >
                                      重挂直接后继
                                    </Button>
                                  </Space>
                                </Space>
                              ) : null}
                              <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                <Input
                                  size="small"
                                  value={planRelationDraftsByTaskId[task.id]?.dependsOnRefs || ''}
                                  onChange={(event) => {
                                    const value = event.target.value;
                                    setPlanRelationDraftsByTaskId((prev) => ({
                                      ...prev,
                                      [task.id]: {
                                        dependsOnRefs: value,
                                        verificationOfRefs: prev[task.id]?.verificationOfRefs || '',
                                      },
                                    }));
                                  }}
                                  placeholder="前置任务引用，逗号分隔，可填 task_ref / 短 ID"
                                  disabled={planActionLoading}
                                />
                                <Input
                                  size="small"
                                  value={planRelationDraftsByTaskId[task.id]?.verificationOfRefs || ''}
                                  onChange={(event) => {
                                    const value = event.target.value;
                                    setPlanRelationDraftsByTaskId((prev) => ({
                                      ...prev,
                                      [task.id]: {
                                        dependsOnRefs: prev[task.id]?.dependsOnRefs || '',
                                        verificationOfRefs: value,
                                      },
                                    }));
                                  }}
                                  placeholder="验证对象引用，逗号分隔，可填 task_ref / 短 ID"
                                  disabled={planActionLoading}
                                />
                                <Space wrap>
                                  <Button
                                    size="small"
                                    disabled={planActionLoading}
                                    onClick={() => { void savePlanTaskLinks(selectedPlan.planId, task.id); }}
                                  >
                                    保存依赖
                                  </Button>
                                  <Text type="secondary">
                                    可直接用当前计划中的 `task_ref` 来重挂前置和验证关系
                                  </Text>
                                </Space>
                              </Space>
                              {task.handoff_payload?.summary ? (
                                <Paragraph type="secondary" style={{ marginBottom: 0, whiteSpace: 'pre-wrap' }}>
                                  {`最近交接: ${task.handoff_payload.summary}`}
                                </Paragraph>
                              ) : (
                                <Text type="secondary">最近交接: 暂无</Text>
                              )}
                              {task.blocked_reason ? (
                                <Alert
                                  type="warning"
                                  showIcon
                                  message="阻塞"
                                  description={formatBlockedReason(task.blocked_reason)}
                                />
                              ) : null}
                            </Space>
                          </Card>
                        ))}
                      </Space>
                    </Space>
                  </Card>
                )}
                {taskPlans.length > 0 && (
                  <>
                    <Space
                      wrap
                      size={12}
                      style={{ width: '100%', marginBottom: 16, alignItems: 'stretch' }}
                    >
                      {taskPlans.slice(0, 6).map((plan) => (
                        <Card
                          key={plan.plan_id}
                          size="small"
                          hoverable
                          style={{
                            width: 260,
                            borderColor: selectedPlanId === plan.plan_id ? '#8b3a2e' : undefined,
                          }}
                          onClick={() => focusTaskPlan(plan.plan_id)}
                        >
                          <Space direction="vertical" size={6} style={{ width: '100%' }}>
                            <Space wrap>
                              <Tag color="geekblue">{plan.plan_id}</Tag>
                              <Tag>{`${plan.task_count} 个任务`}</Tag>
                            </Space>
                            <Text strong>{truncateText(plan.title, 36)}</Text>
                            <Space wrap>
                              {Object.entries(plan.status_counts).map(([statusKey, count]) => (
                                <Tag key={`${plan.plan_id}-${statusKey}`} color={formatTaskStatusColor(statusKey)}>
                                  {`${statusKey}: ${count}`}
                                </Tag>
                              ))}
                            </Space>
                            <Text type="secondary">
                              最近更新:
                              {' '}
                              {(Date.parse(plan.latest_updated_at) || 0) > 0
                                ? new Date(plan.latest_updated_at).toLocaleString()
                                : '-'}
                            </Text>
                            <Space wrap>
                              <Button
                                size="small"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  focusTaskPlan(plan.plan_id);
                                }}
                              >
                                只看此计划
                              </Button>
                              <Button
                                size="small"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  setExpandedTaskIds(plan.tasks.map((item) => item.id));
                                }}
                              >
                                展开节点
                              </Button>
                            </Space>
                          </Space>
                        </Card>
                      ))}
                    </Space>
                    <Collapse
                      ghost
                      style={{ marginBottom: 16 }}
                      items={taskPlans.slice(0, 12).map((plan) => ({
                        key: `plan-${plan.plan_id}`,
                        label: (
                          <Space wrap>
                            <Text strong>{plan.plan_id}</Text>
                            <Tag>{`${plan.task_count} 个节点`}</Tag>
                            {Object.entries(plan.status_counts).map(([statusKey, count]) => (
                              <Tag key={`${plan.plan_id}-collapse-${statusKey}`} color={formatTaskStatusColor(statusKey)}>
                                {`${statusKey}: ${count}`}
                              </Tag>
                            ))}
                          </Space>
                        ),
                        children: (
                          <Space direction="vertical" size={10} style={{ width: '100%' }}>
                            <Space wrap>
                              <Button size="small" onClick={() => focusTaskPlan(plan.plan_id)}>
                                只看此计划
                              </Button>
                              <Button
                                size="small"
                                onClick={() => setExpandedTaskIds(plan.tasks.map((item) => item.id))}
                              >
                                展开全部节点
                              </Button>
                            </Space>
                            {plan.tasks.map((task, index) => (
                              <Card key={`${plan.plan_id}-${task.id}`} size="small" bodyStyle={{ padding: 12 }}>
                                <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                  <Space wrap>
                                    <Tag color="geekblue">{`节点 ${index + 1}`}</Tag>
                                    <Tag color={formatTaskStatusColor(task.status)}>{task.status}</Tag>
                                    {task.task_kind ? <Tag color="purple">{task.task_kind}</Tag> : null}
                                    {task.task_ref ? <Tag>{task.task_ref}</Tag> : null}
                                  </Space>
                                  <Text strong>{task.title}</Text>
                                  <Text type="secondary">{truncateText(task.content, 120)}</Text>
                                  <Space wrap>
                                    <Text type="secondary">{`联系人: ${getContactDisplayName(task)}`}</Text>
                                    <Text type="secondary">{`项目: ${getProjectDisplayName(task)}`}</Text>
                                  </Space>
                                  {(task.depends_on_task_ids?.length ?? 0) > 0 ? (
                                    <Text type="secondary">
                                      {`依赖: ${task.depends_on_task_ids.map((taskId) => formatRelatedTask(taskId)).join(' / ')}`}
                                    </Text>
                                  ) : null}
                                  {(task.verification_of_task_ids?.length ?? 0) > 0 ? (
                                    <Text type="secondary">
                                      {`验证: ${task.verification_of_task_ids.map((taskId) => formatRelatedTask(taskId)).join(' / ')}`}
                                    </Text>
                                  ) : null}
                                  {task.blocked_reason ? (
                                    <Alert
                                      type="warning"
                                      showIcon
                                      message="阻塞"
                                      description={formatBlockedReason(task.blocked_reason)}
                                    />
                                  ) : null}
                                </Space>
                              </Card>
                            ))}
                          </Space>
                        ),
                      }))}
                    />
                  </>
                )}
                <Table<ContactTask>
                  rowKey="id"
                  loading={loading}
                  columns={columns}
                  dataSource={visibleTasks}
                  pagination={{ pageSize: 20, showSizeChanger: false }}
                  expandable={{
                    expandedRowKeys: expandedTaskIds,
                    expandRowByClick: true,
                    onExpand: (expanded, record) => {
                      setExpandedTaskIds((prev) => (
                        expanded
                          ? Array.from(new Set([...prev, record.id]))
                          : prev.filter((item) => item !== record.id)
                      ));
                      if (expanded) {
                        void loadExecutionMessages(record.id, true);
                        void loadTaskResultBrief(record.id, true);
                      }
                    },
                    expandedRowRender: (record) => {
                      const executionMessages = executionMessagesByTaskId[record.id] || [];
                      const executionPage = executionPageByTaskId[record.id] || 1;
                      const safeExecutionPage = Math.min(
                        executionPage,
                        Math.max(1, Math.ceil(executionMessages.length / EXECUTION_PAGE_SIZE)),
                      );
                      const pagedExecutionMessages = executionMessages.slice(
                        (safeExecutionPage - 1) * EXECUTION_PAGE_SIZE,
                        safeExecutionPage * EXECUTION_PAGE_SIZE,
                      );

                      return (
                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                        <Text strong>任务内容</Text>
                        <Paragraph style={{ marginBottom: 0 }}>{record.content}</Paragraph>
                        <Text strong>任务图谱</Text>
                        <Space direction="vertical" size={4} style={{ width: '100%' }}>
                          <Space wrap>
                            <Tag color="geekblue">{record.task_plan_id || '未分组计划'}</Tag>
                            {record.task_ref ? <Tag>{record.task_ref}</Tag> : null}
                            {record.task_kind ? <Tag color="purple">{record.task_kind}</Tag> : null}
                            <Tag color={formatTaskStatusColor(record.status)}>{record.status}</Tag>
                          </Space>
                          {typeof record.queue_position === 'number' ? (
                            <Text type="secondary">
                              执行顺位:
                              {' '}
                              {record.queue_position}
                            </Text>
                          ) : null}
                          {record.conversation_turn_id ? (
                            <Text type="secondary">
                              来源轮次:
                              {' '}
                              {record.conversation_turn_id}
                            </Text>
                          ) : null}
                          {(record.depends_on_task_ids?.length ?? 0) > 0 ? (
                            <>
                              <Text type="secondary">前置任务:</Text>
                              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                {record.depends_on_task_ids.map((taskId) => (
                                  <Text key={`${record.id}-depends-${taskId}`}>{formatRelatedTask(taskId)}</Text>
                                ))}
                              </Space>
                            </>
                          ) : (
                            <Text type="secondary">前置任务: 无</Text>
                          )}
                          {(record.verification_of_task_ids?.length ?? 0) > 0 ? (
                            <>
                              <Text type="secondary">验证对象:</Text>
                              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                {record.verification_of_task_ids.map((taskId) => (
                                  <Text key={`${record.id}-verify-${taskId}`}>{formatRelatedTask(taskId)}</Text>
                                ))}
                              </Space>
                            </>
                          ) : null}
                          {(record.acceptance_criteria?.length ?? 0) > 0 ? (
                            <>
                              <Text type="secondary">验收标准:</Text>
                              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                {record.acceptance_criteria.map((criterion, index) => (
                                  <Text key={`${record.id}-criterion-${index}`}>{`${index + 1}. ${criterion}`}</Text>
                                ))}
                              </Space>
                            </>
                          ) : null}
                          {record.blocked_reason ? (
                            <Alert
                              type="warning"
                              showIcon
                              message="当前阻塞"
                              description={formatBlockedReason(record.blocked_reason)}
                            />
                          ) : null}
                        </Space>
                        <Text strong>计划使用的内置 MCP</Text>
                        {(record.planned_builtin_mcp_ids?.length ?? 0) > 0 ? (
                          <Space wrap>
                            {record.planned_builtin_mcp_ids.map((mcpId) => (
                              <Tag key={mcpId} color="processing">
                                {formatBuiltinMcpLabel(mcpId)}
                                {' '}
                                ({mcpId})
                              </Tag>
                            ))}
                          </Space>
                        ) : (
                          <Text type="secondary">未配置计划使用的内置 MCP。</Text>
                        )}
                        <Text strong>计划使用的上下文资产</Text>
                        {(record.planned_context_assets?.length ?? 0) > 0 ? (
                          <Space direction="vertical" size={8} style={{ width: '100%' }}>
                            {record.planned_context_assets.map((asset) => (
                              <Card
                                key={`${asset.asset_type}:${asset.asset_id}`}
                                size="small"
                                bodyStyle={{ padding: 12 }}
                                style={{ width: '100%' }}
                              >
                                <Space direction="vertical" size={4} style={{ width: '100%' }}>
                                  <Space wrap>
                                    <Tag color="cyan">{formatAssetTypeLabel(asset.asset_type)}</Tag>
                                    <Text strong>{asset.display_name || asset.asset_id}</Text>
                                  </Space>
                                  <Text type="secondary">ID: {asset.asset_id}</Text>
                                  {asset.source_type && (
                                    <Text type="secondary">来源类型: {asset.source_type}</Text>
                                  )}
                                  {asset.source_path && (
                                    <Paragraph type="secondary" style={{ marginBottom: 0 }}>
                                      来源路径: {asset.source_path}
                                    </Paragraph>
                                  )}
                                </Space>
                              </Card>
                            ))}
                          </Space>
                        ) : (
                          <Text type="secondary">未配置计划使用的上下文资产。</Text>
                        )}
                        <Text strong>执行上下文</Text>
                        <Space direction="vertical" size={4} style={{ width: '100%' }}>
                          {record.project_root ? (
                            <Paragraph style={{ marginBottom: 0 }}>
                              项目路径:
                              {' '}
                              {record.project_root}
                            </Paragraph>
                          ) : (
                            <Text type="secondary">未记录 project_root。</Text>
                          )}
                          {record.remote_connection_id ? (
                            <Text type="secondary">
                              远程连接:
                              {' '}
                              {record.remote_connection_id}
                            </Text>
                          ) : (
                            <Text type="secondary">未记录 remote_connection_id。</Text>
                          )}
                          {record.paused_at ? (
                            <Text type="secondary">
                              暂停时间:
                              {' '}
                              {new Date(record.paused_at).toLocaleString()}
                            </Text>
                          ) : null}
                          {record.pause_reason ? (
                            <Paragraph type="secondary" style={{ marginBottom: 0 }}>
                              暂停原因:
                              {' '}
                              {record.pause_reason}
                            </Paragraph>
                          ) : null}
                          {record.last_checkpoint_summary ? (
                            <Paragraph type="secondary" style={{ marginBottom: 0 }}>
                              最近检查点:
                              {' '}
                              {record.last_checkpoint_summary}
                            </Paragraph>
                          ) : null}
                          {record.resume_note ? (
                            <Paragraph type="secondary" style={{ marginBottom: 0 }}>
                              最近恢复说明:
                              {' '}
                              {record.resume_note}
                            </Paragraph>
                          ) : null}
                        </Space>
                        {record.execution_result_contract && (
                          <>
                            <Text strong>结果要求</Text>
                            <Space wrap>
                              <Tag color={record.execution_result_contract.result_required ? 'green' : 'default'}>
                                {record.execution_result_contract.result_required ? '必须产出结果' : '结果非必填'}
                              </Tag>
                              {record.execution_result_contract.preferred_format && (
                                <Tag>{record.execution_result_contract.preferred_format}</Tag>
                              )}
                            </Space>
                          </>
                        )}
                        {record.planning_snapshot && (
                          <>
                            <Text strong>规划快照</Text>
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              {record.planning_snapshot.selected_model_config_id && (
                                <Text type="secondary">
                                  规划时模型配置:
                                  {' '}
                                  {record.planning_snapshot.selected_model_config_id}
                                </Text>
                              )}
                              {record.planning_snapshot.planned_at && (
                                <Text type="secondary">
                                  规划时间:
                                  {' '}
                                  {new Date(record.planning_snapshot.planned_at).toLocaleString()}
                                </Text>
                              )}
                              {record.planning_snapshot.source_user_goal_summary && (
                                <>
                                  <Text type="secondary">来源用户目标摘要:</Text>
                                  <Paragraph style={{ marginBottom: 0 }}>
                                    {record.planning_snapshot.source_user_goal_summary}
                                  </Paragraph>
                                </>
                              )}
                              {record.planning_snapshot.source_constraints_summary && (
                                <>
                                  <Text type="secondary">来源约束摘要:</Text>
                                  <Paragraph style={{ marginBottom: 0 }}>
                                    {record.planning_snapshot.source_constraints_summary}
                                  </Paragraph>
                                </>
                              )}
                              <Text type="secondary">当时联系人已授权的内置 MCP:</Text>
                              {(record.planning_snapshot.contact_authorized_builtin_mcp_ids?.length ?? 0) > 0 ? (
                                <Space wrap>
                                  {record.planning_snapshot.contact_authorized_builtin_mcp_ids.map((mcpId) => (
                                    <Tag key={`authorized-${mcpId}`}>
                                      {formatBuiltinMcpLabel(mcpId)}
                                      {' '}
                                      ({mcpId})
                                    </Tag>
                                  ))}
                                </Space>
                              ) : (
                                <Text type="secondary">当时没有可用的联系人授权内置 MCP。</Text>
                              )}
                            </Space>
                          </>
                        )}
                        {record.result_summary && (
                          <>
                            <Text strong>执行结果摘要</Text>
                            <Paragraph style={{ marginBottom: 0 }}>{record.result_summary}</Paragraph>
                          </>
                        )}
                        {record.handoff_payload && (
                          <>
                            <Text strong>任务交接</Text>
                            <Card size="small" bodyStyle={{ padding: 12 }} style={{ width: '100%' }}>
                              <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                <Space wrap>
                                  <Tag color="magenta">{formatHandoffKind(record.handoff_payload.handoff_kind)}</Tag>
                                  {record.handoff_payload.generated_at && (
                                    <Text type="secondary">
                                      生成时间:
                                      {' '}
                                      {new Date(record.handoff_payload.generated_at).toLocaleString()}
                                    </Text>
                                  )}
                                </Space>
                                <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                  {record.handoff_payload.summary}
                                </Paragraph>
                                {record.handoff_payload.result_summary && (
                                  <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                    {record.handoff_payload.result_summary}
                                  </Paragraph>
                                )}
                                {record.handoff_payload.key_changes.length > 0 && (
                                  <>
                                    <Text type="secondary">关键变化</Text>
                                    <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                      {record.handoff_payload.key_changes.map((item, index) => (
                                        <Text key={`${record.id}-handoff-change-${index}`}>{`${index + 1}. ${item}`}</Text>
                                      ))}
                                    </Space>
                                  </>
                                )}
                                {record.handoff_payload.verification_suggestions.length > 0 && (
                                  <>
                                    <Text type="secondary">验证建议</Text>
                                    <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                      {record.handoff_payload.verification_suggestions.map((item, index) => (
                                        <Text key={`${record.id}-handoff-verify-${index}`}>{`${index + 1}. ${item}`}</Text>
                                      ))}
                                    </Space>
                                  </>
                                )}
                                {record.handoff_payload.open_risks.length > 0 && (
                                  <>
                                    <Text type="secondary">遗留风险</Text>
                                    <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                      {record.handoff_payload.open_risks.map((item, index) => (
                                        <Text key={`${record.id}-handoff-risk-${index}`} type="danger">{`${index + 1}. ${item}`}</Text>
                                      ))}
                                    </Space>
                                  </>
                                )}
                                {record.handoff_payload.artifact_refs.length > 0 && (
                                  <>
                                    <Text type="secondary">关联引用</Text>
                                    <Space direction="vertical" size={2} style={{ width: '100%' }}>
                                      {record.handoff_payload.artifact_refs.map((item, index) => (
                                        <Text key={`${record.id}-handoff-artifact-${index}`}>{item}</Text>
                                      ))}
                                    </Space>
                                  </>
                                )}
                              </Space>
                            </Card>
                          </>
                        )}
                        <Text strong>任务结果桥接摘要</Text>
                        <Space>
                          <Button
                            size="small"
                            icon={<ReloadOutlined />}
                            loading={resultBriefLoadingByTaskId[record.id]}
                            onClick={() => { void loadTaskResultBrief(record.id, true); }}
                          >
                            刷新结果桥接
                          </Button>
                        </Space>
                        {resultBriefErrorByTaskId[record.id] && (
                          <Alert
                            type="error"
                            showIcon
                            message={resultBriefErrorByTaskId[record.id] || '加载任务结果桥接失败'}
                          />
                        )}
                        {resultBriefLoadingByTaskId[record.id] && (
                          <Space>
                            <Spin size="small" />
                            <Text type="secondary">任务结果桥接加载中...</Text>
                          </Space>
                        )}
                        {!resultBriefLoadingByTaskId[record.id]
                          && !resultBriefErrorByTaskId[record.id]
                          && resultBriefByTaskId[record.id] === null && (
                          <Text type="secondary">当前任务还没有生成结果桥接摘要，通常会在任务进入终态后出现。</Text>
                        )}
                        {resultBriefByTaskId[record.id] && (
                          <Card size="small" bodyStyle={{ padding: 12 }} style={{ width: '100%' }}>
                            <Space direction="vertical" size={6} style={{ width: '100%' }}>
                              <Space wrap>
                                <Tag color={formatTaskStatusColor(resultBriefByTaskId[record.id]?.task_status)}>
                                  {resultBriefByTaskId[record.id]?.task_status}
                                </Tag>
                                {resultBriefByTaskId[record.id]?.result_format && (
                                  <Tag>{resultBriefByTaskId[record.id]?.result_format}</Tag>
                                )}
                                {resultBriefByTaskId[record.id]?.finished_at && (
                                  <Text type="secondary">
                                    完成时间:
                                    {' '}
                                    {new Date(resultBriefByTaskId[record.id]!.finished_at as string).toLocaleString()}
                                  </Text>
                                )}
                              </Space>
                              <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                {resultBriefByTaskId[record.id]?.result_summary}
                              </Paragraph>
                              {resultBriefByTaskId[record.id]?.result_message_id && (
                                <Text type="secondary">
                                  结果消息 ID:
                                  {' '}
                                  {resultBriefByTaskId[record.id]?.result_message_id}
                                </Text>
                              )}
                              {resultBriefByTaskId[record.id]?.source_session_id && (
                                <Text type="secondary">
                                  来源会话:
                                  {' '}
                                  {resultBriefByTaskId[record.id]?.source_session_id}
                                </Text>
                              )}
                            </Space>
                          </Card>
                        )}
                        {record.last_error && (
                          <>
                            <Text strong>最后错误</Text>
                            <Alert type="error" showIcon message={record.last_error} />
                          </>
                        )}
                        <Collapse
                          ghost
                          activeKey={executionSectionExpandedByTaskId[record.id] ? ['execution'] : []}
                          onChange={(keys) => {
                            const nextKeys = Array.isArray(keys) ? keys : [keys];
                            setExecutionSectionExpandedByTaskId((prev) => ({
                              ...prev,
                              [record.id]: nextKeys.includes('execution'),
                            }));
                          }}
                          items={[
                            {
                              key: 'execution',
                              label: (
                                <Space wrap>
                                  <Text strong>执行过程</Text>
                                  <Tag>{executionMessages.length} 条</Tag>
                                  {executionLoadingByTaskId[record.id] && <Tag color="processing">加载中</Tag>}
                                </Space>
                              ),
                              children: (
                                <Space direction="vertical" size={10} style={{ width: '100%' }}>
                                  <Space>
                                    <Button
                                      size="small"
                                      icon={<ReloadOutlined />}
                                      loading={executionLoadingByTaskId[record.id]}
                                      onClick={(event) => {
                                        event.stopPropagation();
                                        void loadExecutionMessages(record.id, true);
                                      }}
                                    >
                                      刷新执行记录
                                    </Button>
                                  </Space>
                                  {executionErrorByTaskId[record.id] && (
                                    <Alert
                                      type="error"
                                      showIcon
                                      message={executionErrorByTaskId[record.id] || '加载执行过程失败'}
                                    />
                                  )}
                                  {executionLoadingByTaskId[record.id] && (
                                    <Space>
                                      <Spin size="small" />
                                      <Text type="secondary">执行记录加载中...</Text>
                                    </Space>
                                  )}
                                  {!executionLoadingByTaskId[record.id]
                                    && !executionErrorByTaskId[record.id]
                                    && executionMessages.length === 0 && (
                                    <Text type="secondary">暂无执行记录。</Text>
                                  )}
                                  {pagedExecutionMessages.map((message) => {
                                    const toolEntries = extractToolEntries(message.tool_calls);
                                    const hasToolDetails =
                                      message.role === 'tool'
                                      || toolEntries.length > 0
                                      || Boolean(message.tool_call_id);

                                    return (
                                      <Card
                                        key={message.id}
                                        size="small"
                                        bodyStyle={{ padding: 12 }}
                                        style={{ width: '100%' }}
                                      >
                                        <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                          <Space wrap>
                                            <Tag color={message.role === 'assistant' ? 'blue' : message.role === 'tool' ? 'purple' : 'default'}>
                                              {message.role}
                                            </Tag>
                                            {message.message_source && <Tag>{message.message_source}</Tag>}
                                            {message.summary_status && <Tag>{message.summary_status}</Tag>}
                                            <Text type="secondary">{new Date(message.created_at).toLocaleString()}</Text>
                                          </Space>
                                          {hasToolDetails ? (
                                            <>
                                              <Text type="secondary">
                                                {truncateText(message.content, 160)}
                                              </Text>
                                              <Collapse
                                                ghost
                                                items={[
                                                  {
                                                    key: 'tool-details',
                                                    label: (
                                                      <Space wrap>
                                                        <Text>工具调用详情</Text>
                                                        {toolEntries.length > 0 && <Tag>{toolEntries.length} 个</Tag>}
                                                      </Space>
                                                    ),
                                                    children: (
                                                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                                                        <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                          {message.content || '-'}
                                                        </Paragraph>
                                                        {message.tool_call_id && (
                                                          <Text type="secondary">
                                                            tool_call_id:
                                                            {' '}
                                                            {message.tool_call_id}
                                                          </Text>
                                                        )}
                                                        {toolEntries.length > 0 && (
                                                          <Space direction="vertical" size={8} style={{ width: '100%' }}>
                                                            {toolEntries.map((entry, index) => (
                                                              <Card
                                                                key={`${message.id}-tool-${index}`}
                                                                size="small"
                                                                bodyStyle={{ padding: 10 }}
                                                              >
                                                                <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                                                  <Text strong>{getToolEntryLabel(entry, index)}</Text>
                                                                  <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                                    {stringifyPretty(entry)}
                                                                  </pre>
                                                                </Space>
                                                              </Card>
                                                            ))}
                                                          </Space>
                                                        )}
                                                        {message.metadata && (
                                                          <Card size="small" bodyStyle={{ padding: 10 }}>
                                                            <Space direction="vertical" size={4} style={{ width: '100%' }}>
                                                              <Text strong>metadata</Text>
                                                              <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                                {stringifyPretty(message.metadata)}
                                                              </pre>
                                                            </Space>
                                                          </Card>
                                                        )}
                                                        {message.reasoning && (
                                                          <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                            {message.reasoning}
                                                          </Paragraph>
                                                        )}
                                                      </Space>
                                                    ),
                                                  },
                                                ]}
                                              />
                                            </>
                                          ) : (
                                            <>
                                              <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                {message.content || '-'}
                                              </Paragraph>
                                              {message.reasoning && (
                                                <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                                                  {message.reasoning}
                                                </Paragraph>
                                              )}
                                              {message.metadata && (
                                                <Collapse
                                                  ghost
                                                  items={[
                                                    {
                                                      key: 'message-metadata',
                                                      label: '附加信息',
                                                      children: (
                                                        <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                                          {stringifyPretty(message.metadata)}
                                                        </pre>
                                                      ),
                                                    },
                                                  ]}
                                                />
                                              )}
                                            </>
                                          )}
                                        </Space>
                                      </Card>
                                    );
                                  })}
                                  {executionMessages.length > EXECUTION_PAGE_SIZE && (
                                    <Pagination
                                      align="end"
                                      current={safeExecutionPage}
                                      pageSize={EXECUTION_PAGE_SIZE}
                                      total={executionMessages.length}
                                      showSizeChanger={false}
                                      onChange={(page) => {
                                        setExecutionPageByTaskId((prev) => ({ ...prev, [record.id]: page }));
                                      }}
                                    />
                                  )}
                                </Space>
                              ),
                            },
                          ]}
                        />
                      </Space>
                      );
                    },
                  }}
                />
              </Card>
            </Content>
          </Layout>
        )}
      </AntdApp>
    </ConfigProvider>
  );
}

export default App;
