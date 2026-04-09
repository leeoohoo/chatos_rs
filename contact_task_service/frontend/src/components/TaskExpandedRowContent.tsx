import { Alert, Card, Space, Tag, Typography } from 'antd';

import {
  EXECUTION_PAGE_SIZE,
  formatAssetTypeLabel,
  formatBlockedReason,
  formatBuiltinMcpLabel,
  formatTaskStatusColor,
} from '../appHelpers';
import type { ContactTask, TaskExecutionMessage, TaskResultBrief } from '../types';
import { TaskExecutionSection } from './TaskExecutionSection';
import { TaskHandoffSection } from './TaskHandoffSection';
import { TaskResultBriefSection } from './TaskResultBriefSection';

const { Text, Paragraph } = Typography;

type TaskExpandedRowContentProps = {
  record: ContactTask;
  formatRelatedTask: (taskId: string) => string;
  executionMessages: TaskExecutionMessage[];
  executionLoading: boolean;
  executionError?: string | null;
  executionSectionExpanded: boolean;
  executionPage: number;
  onExecutionExpandedChange: (expanded: boolean) => void;
  onExecutionRefresh: () => Promise<void> | void;
  onExecutionPageChange: (page: number) => void;
  resultBrief: TaskResultBrief | null | undefined;
  resultBriefLoading: boolean;
  resultBriefError?: string | null;
  onResultBriefRefresh: () => Promise<void> | void;
};

export function TaskExpandedRowContent({
  record,
  formatRelatedTask,
  executionMessages,
  executionLoading,
  executionError,
  executionSectionExpanded,
  executionPage,
  onExecutionExpandedChange,
  onExecutionRefresh,
  onExecutionPageChange,
  resultBrief,
  resultBriefLoading,
  resultBriefError,
  onResultBriefRefresh,
}: TaskExpandedRowContentProps) {
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
        <TaskHandoffSection taskId={record.id} payload={record.handoff_payload} />
      )}
      <TaskResultBriefSection
        resultBrief={resultBrief}
        loading={resultBriefLoading}
        error={resultBriefError}
        onRefresh={onResultBriefRefresh}
      />
      {record.last_error && (
        <>
          <Text strong>最后错误</Text>
          <Alert type="error" showIcon message={record.last_error} />
        </>
      )}
      <TaskExecutionSection
        expanded={executionSectionExpanded}
        executionMessages={executionMessages}
        loading={executionLoading}
        error={executionError}
        pagedExecutionMessages={pagedExecutionMessages}
        safeExecutionPage={safeExecutionPage}
        onExpandedChange={onExecutionExpandedChange}
        onRefresh={onExecutionRefresh}
        onPageChange={onExecutionPageChange}
      />
    </Space>
  );
}
