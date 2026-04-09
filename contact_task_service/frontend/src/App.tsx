import {
  LogoutOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import {
  Alert,
  App as AntdApp,
  Button,
  Card,
  ConfigProvider,
  Input,
  Layout,
  Select,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { useEffect, useMemo, useState } from 'react';

import { api, authStore } from './api';
import {
  buildPlanImpact,
  describeRewireOperation,
  describeSkipOperation,
} from './appHelpers';
import { SelectedPlanPanel } from './components/SelectedPlanPanel';
import { TaskBoardTable } from './components/TaskBoardTable';
import { TaskPlanOverviewSection } from './components/TaskPlanOverviewSection';
import type {
  AuthUser,
  ContactTask,
  MemoryContactSummary,
  MemoryProjectSummary,
  TaskExecutionMessage,
  TaskPlanView,
  TaskResultBrief,
} from './types';

const { Header, Content } = Layout;
const { Title, Text, Paragraph } = Typography;

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

  const selectedPlanImpact = useMemo(() => {
    if (!selectedPlan) {
      return {
        directDependentsByTaskId: {} as Record<string, string[]>,
        descendantIdsByTaskId: {} as Record<string, string[]>,
      };
    }
    return buildPlanImpact(selectedPlan.items);
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

  const updatePlanRewireTarget = (taskId: string, value: string) => {
    setPlanRewireTargetByTaskId((prev) => ({
      ...prev,
      [taskId]: value,
    }));
  };

  const updatePlanRelationDraft = (
    taskId: string,
    patch: Partial<{ dependsOnRefs: string; verificationOfRefs: string }>,
  ) => {
    setPlanRelationDraftsByTaskId((prev) => ({
      ...prev,
      [taskId]: {
        dependsOnRefs: patch.dependsOnRefs ?? prev[taskId]?.dependsOnRefs ?? '',
        verificationOfRefs: patch.verificationOfRefs ?? prev[taskId]?.verificationOfRefs ?? '',
      },
    }));
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
                  <SelectedPlanPanel
                    selectedPlan={selectedPlan}
                    selectedPlanImpact={selectedPlanImpact}
                    planActionLoading={planActionLoading}
                    planRelationDraftsByTaskId={planRelationDraftsByTaskId}
                    planRewireTargetByTaskId={planRewireTargetByTaskId}
                    formatRelatedTask={formatRelatedTask}
                    onExpandAllTasks={(taskIds) => setExpandedTaskIds(taskIds)}
                    onExit={clearTaskPlanFocus}
                    onMoveTask={movePlanTask}
                    onSkipTask={(planId, taskId) => setPlanTaskStatus(planId, taskId, 'skipped', '节点已跳过')}
                    onCascadeSkipTask={cascadeSkipPlanTask}
                    onRewireTargetChange={updatePlanRewireTarget}
                    onRewireDirectDependents={rewireDirectDependents}
                    onRelationDraftChange={updatePlanRelationDraft}
                    onSavePlanTaskLinks={savePlanTaskLinks}
                  />
                )}
                <TaskPlanOverviewSection
                  taskPlans={taskPlans}
                  selectedPlanId={selectedPlanId}
                  formatRelatedTask={formatRelatedTask}
                  getContactDisplayName={getContactDisplayName}
                  getProjectDisplayName={getProjectDisplayName}
                  onFocusTaskPlan={focusTaskPlan}
                  onExpandTaskIds={(taskIds) => setExpandedTaskIds(taskIds)}
                />
                <TaskBoardTable
                  loading={loading}
                  visibleTasks={visibleTasks}
                  expandedTaskIds={expandedTaskIds}
                  formatRelatedTask={formatRelatedTask}
                  getContactDisplayName={getContactDisplayName}
                  getProjectDisplayName={getProjectDisplayName}
                  executionMessagesByTaskId={executionMessagesByTaskId}
                  executionLoadingByTaskId={executionLoadingByTaskId}
                  executionErrorByTaskId={executionErrorByTaskId}
                  executionSectionExpandedByTaskId={executionSectionExpandedByTaskId}
                  executionPageByTaskId={executionPageByTaskId}
                  resultBriefByTaskId={resultBriefByTaskId}
                  resultBriefLoadingByTaskId={resultBriefLoadingByTaskId}
                  resultBriefErrorByTaskId={resultBriefErrorByTaskId}
                  onExpandedTaskIdsChange={setExpandedTaskIds}
                  onLoadExecutionMessages={loadExecutionMessages}
                  onLoadTaskResultBrief={loadTaskResultBrief}
                  onExecutionSectionExpandedChange={(taskId, expanded) => {
                    setExecutionSectionExpandedByTaskId((prev) => ({
                      ...prev,
                      [taskId]: expanded,
                    }));
                  }}
                  onExecutionPageChange={(taskId, page) => {
                    setExecutionPageByTaskId((prev) => ({ ...prev, [taskId]: page }));
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
