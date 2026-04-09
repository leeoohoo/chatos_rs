import { Alert, Button, Card, Input, Select, Space, Tag, Typography } from 'antd';

import { formatBlockedReason, formatTaskStatusColor, truncateText } from '../appHelpers';
import type { ContactTask } from '../types';

const { Text, Paragraph } = Typography;

export type SelectedPlanPanelModel = {
  planId: string;
  items: ContactTask[];
  title: string;
  taskCount: number;
  statusCounts: Record<string, number>;
  activeTaskId: string | null;
  blockedTaskCount: number;
};

type PlanRelationDraft = {
  dependsOnRefs: string;
  verificationOfRefs: string;
};

type SelectedPlanImpact = {
  directDependentsByTaskId: Record<string, string[]>;
  descendantIdsByTaskId: Record<string, string[]>;
};

type SelectedPlanPanelProps = {
  selectedPlan: SelectedPlanPanelModel;
  selectedPlanImpact: SelectedPlanImpact;
  planActionLoading: boolean;
  planRelationDraftsByTaskId: Record<string, PlanRelationDraft>;
  planRewireTargetByTaskId: Record<string, string>;
  formatRelatedTask: (taskId: string) => string;
  onExpandAllTasks: (taskIds: string[]) => void;
  onExit: () => void;
  onMoveTask: (planId: string, taskId: string, direction: -1 | 1) => Promise<void> | void;
  onSkipTask: (planId: string, taskId: string) => Promise<void> | void;
  onCascadeSkipTask: (planId: string, taskId: string) => Promise<void> | void;
  onRewireTargetChange: (taskId: string, value: string) => void;
  onRewireDirectDependents: (planId: string, taskId: string) => Promise<void> | void;
  onRelationDraftChange: (taskId: string, patch: Partial<PlanRelationDraft>) => void;
  onSavePlanTaskLinks: (planId: string, taskId: string) => Promise<void> | void;
};

function formatPlanNodeLabel(task: ContactTask): string {
  const taskRef = task.task_ref?.trim();
  return taskRef ? `${task.title} · ${taskRef}` : task.title;
}

export function SelectedPlanPanel({
  selectedPlan,
  selectedPlanImpact,
  planActionLoading,
  planRelationDraftsByTaskId,
  planRewireTargetByTaskId,
  formatRelatedTask,
  onExpandAllTasks,
  onExit,
  onMoveTask,
  onSkipTask,
  onCascadeSkipTask,
  onRewireTargetChange,
  onRewireDirectDependents,
  onRelationDraftChange,
  onSavePlanTaskLinks,
}: SelectedPlanPanelProps) {
  return (
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
            <Button size="small" onClick={() => onExpandAllTasks(selectedPlan.items.map((item) => item.id))}>
              展开此计划全部任务
            </Button>
            <Button size="small" onClick={onExit}>
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
                      onClick={() => { void onMoveTask(selectedPlan.planId, task.id, -1); }}
                    >
                      上移
                    </Button>
                    <Button
                      size="small"
                      disabled={planActionLoading || index === selectedPlan.items.length - 1}
                      onClick={() => { void onMoveTask(selectedPlan.planId, task.id, 1); }}
                    >
                      下移
                    </Button>
                    <Button
                      size="small"
                      danger
                      disabled={planActionLoading || ['running', 'completed', 'failed', 'cancelled', 'skipped'].includes(task.status)}
                      onClick={() => { void onSkipTask(selectedPlan.planId, task.id); }}
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
                      onClick={() => { void onCascadeSkipTask(selectedPlan.planId, task.id); }}
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
                          onRewireTargetChange(task.id, value);
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
                          void onRewireDirectDependents(selectedPlan.planId, task.id);
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
                      onRelationDraftChange(task.id, { dependsOnRefs: event.target.value });
                    }}
                    placeholder="前置任务引用，逗号分隔，可填 task_ref / 短 ID"
                    disabled={planActionLoading}
                  />
                  <Input
                    size="small"
                    value={planRelationDraftsByTaskId[task.id]?.verificationOfRefs || ''}
                    onChange={(event) => {
                      onRelationDraftChange(task.id, { verificationOfRefs: event.target.value });
                    }}
                    placeholder="验证对象引用，逗号分隔，可填 task_ref / 短 ID"
                    disabled={planActionLoading}
                  />
                  <Space wrap>
                    <Button
                      size="small"
                      disabled={planActionLoading}
                      onClick={() => { void onSavePlanTaskLinks(selectedPlan.planId, task.id); }}
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
  );
}
