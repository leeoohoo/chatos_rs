import { Alert, Button, Card, Collapse, Space, Tag, Typography } from 'antd';

import { formatBlockedReason, formatTaskStatusColor, truncateText } from '../appHelpers';
import type { ContactTask, TaskPlanView } from '../types';

const { Text } = Typography;

type TaskPlanOverviewSectionProps = {
  taskPlans: TaskPlanView[];
  selectedPlanId: string | null;
  formatRelatedTask: (taskId: string) => string;
  getContactDisplayName: (task: ContactTask) => string;
  getProjectDisplayName: (task: ContactTask) => string;
  onFocusTaskPlan: (planId: string) => void;
  onExpandTaskIds: (taskIds: string[]) => void;
};

export function TaskPlanOverviewSection({
  taskPlans,
  selectedPlanId,
  formatRelatedTask,
  getContactDisplayName,
  getProjectDisplayName,
  onFocusTaskPlan,
  onExpandTaskIds,
}: TaskPlanOverviewSectionProps) {
  if (taskPlans.length === 0) {
    return null;
  }

  return (
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
            onClick={() => onFocusTaskPlan(plan.plan_id)}
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
                    onFocusTaskPlan(plan.plan_id);
                  }}
                >
                  只看此计划
                </Button>
                <Button
                  size="small"
                  onClick={(event) => {
                    event.stopPropagation();
                    onExpandTaskIds(plan.tasks.map((item) => item.id));
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
                <Button size="small" onClick={() => onFocusTaskPlan(plan.plan_id)}>
                  只看此计划
                </Button>
                <Button
                  size="small"
                  onClick={() => onExpandTaskIds(plan.tasks.map((item) => item.id))}
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
  );
}
