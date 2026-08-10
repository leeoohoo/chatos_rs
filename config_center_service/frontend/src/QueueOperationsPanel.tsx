import { useState } from 'react';
import {
  App as AntdApp,
  Button,
  Card,
  Col,
  Input,
  InputNumber,
  Modal,
  Row,
  Select,
  Space,
  Statistic,
  Table,
  Tag,
  Typography,
} from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from './api';
import type { QueueOperationsStream } from './types';

type QueueOperationsPanelProps = {
  environment: string;
};

export function QueueOperationsPanel({ environment }: QueueOperationsPanelProps) {
  const { message } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const [replayTarget, setReplayTarget] = useState<QueueOperationsStream | null>(null);
  const [replayItemId, setReplayItemId] = useState('');
  const [replayTenantId, setReplayTenantId] = useState('');
  const [replaySourceId, setReplaySourceId] = useState('');
  const [replayVersion, setReplayVersion] = useState<number | null>(null);
  const [replayEventType, setReplayEventType] = useState<string>();
  const [replayReason, setReplayReason] = useState('');
  const query = useQuery({
    queryKey: ['queue-operations', environment],
    queryFn: () => api.queueOperations(environment),
    refetchInterval: 10000,
  });
  const streams = query.data?.streams || [];
  const unavailable = streams.filter((stream) => !stream.runtime.available).length;
  const deadLetters = streams.reduce(
    (total, stream) => total + queueRuntime(stream, 'dead_letter').messages,
    0,
  );
  const mainBacklog = streams.reduce(
    (total, stream) => total + queueRuntime(stream, 'main').messages,
    0,
  );
  const consumerGaps = streams.filter((stream) => {
    const main = queueRuntime(stream, 'main');
    return main.messages > 0 && main.consumers === 0;
  }).length;
  const replay = useMutation({
    mutationFn: () =>
      api.replayQueueItem(environment, {
        service: replayTarget?.service || '',
        stream: replayTarget?.stream || '',
        item_id: replayItemId.trim(),
        tenant_id: replayTarget?.service === 'memory-engine' ? replayTenantId.trim() : undefined,
        source_id: replayTarget?.service === 'memory-engine' ? replaySourceId.trim() : undefined,
        version: ['memory-engine', 'plugin-management'].includes(replayTarget?.service || '')
          ? replayVersion || undefined
          : undefined,
        event_type: replayTarget?.stream === 'subject_memory' ? replayEventType : undefined,
        reason: replayReason.trim(),
      }),
    onSuccess: async (result) => {
      message.success(
        replayTarget?.service === 'mcp-management'
          ? 'MCP 终态死信已归档，工具不会重新执行'
          : result.dead_letter_archived
            ? '重放已入队，旧死信已归档'
            : '重放已入队，旧死信待归档',
      );
      setReplayTarget(null);
      setReplayItemId('');
      setReplayTenantId('');
      setReplaySourceId('');
      setReplayVersion(null);
      setReplayEventType(undefined);
      setReplayReason('');
      await queryClient.invalidateQueries({ queryKey: ['queue-operations', environment] });
      await queryClient.invalidateQueries({ queryKey: ['audit'] });
    },
    onError: (error: Error) => message.error(error.message),
  });

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Row gutter={[16, 16]}>
        <Col xs={24} md={6}>
          <Card>
            <Statistic title="生效 Revision" value={query.data?.revision || 0} />
          </Card>
        </Col>
        <Col xs={24} md={6}>
          <Card>
            <Statistic
              title="主队列积压"
              value={mainBacklog}
              valueStyle={{ color: mainBacklog > 0 ? '#d97706' : '#15803d' }}
            />
          </Card>
        </Col>
        <Col xs={24} md={6}>
          <Card>
            <Statistic
              title="DLQ 消息"
              value={deadLetters}
              valueStyle={{ color: deadLetters > 0 ? '#b42318' : '#15803d' }}
            />
          </Card>
        </Col>
        <Col xs={24} md={6}>
          <Card>
            <Statistic
              title="异常流"
              value={unavailable + consumerGaps}
              valueStyle={{ color: unavailable + consumerGaps > 0 ? '#b42318' : '#15803d' }}
            />
          </Card>
        </Col>
      </Row>
      <Card
        title="统一异步流状态"
        extra={
          <Typography.Text type="secondary">
            每 10 秒刷新，仅使用 {environment} 当前生效配置
          </Typography.Text>
        }
      >
        <Table
          rowKey={(stream) => `${stream.service}:${stream.stream}`}
          loading={query.isLoading || query.isFetching}
          dataSource={streams}
          pagination={false}
          scroll={{ x: 1050 }}
          columns={[
            { title: '服务', dataIndex: 'service', width: 170 },
            { title: '异步流', dataIndex: 'stream', width: 170, render: (value) => <Tag>{value}</Tag> },
            {
              title: '状态',
              width: 120,
              render: (_, stream) => queueHealthTag(stream),
            },
            {
              title: '主队列',
              width: 240,
              render: (_, stream) => queueCell(stream.main_queue, queueRuntime(stream, 'main')),
            },
            {
              title: '延迟重试',
              width: 220,
              render: (_, stream) => queueCell(stream.retry_queue, queueRuntime(stream, 'retry')),
            },
            {
              title: '死信队列',
              width: 240,
              render: (_, stream) =>
                queueCell(stream.dead_letter_queue, queueRuntime(stream, 'dead_letter'), true),
            },
            {
              title: '操作',
              fixed: 'right',
              width: 110,
              render: (_, stream) => {
                const supported =
                  (stream.service === 'task-runner' && stream.stream === 'run_post_process') ||
                  (
                    stream.service === 'memory-engine' &&
                    ['summary', 'rollup', 'subject_memory'].includes(stream.stream)
                  ) ||
                  (stream.service === 'plugin-management' && stream.stream === 'catalog_sync') ||
                  (stream.service === 'mcp-management' && stream.stream === 'async_tool');
                return (
                  <Button
                    danger
                    size="small"
                    disabled={!supported || queueRuntime(stream, 'dead_letter').messages === 0}
                    onClick={() => {
                      setReplayTarget(stream);
                      setReplayItemId('');
                      setReplayTenantId('');
                      setReplaySourceId('');
                      setReplayVersion(null);
                      setReplayEventType(undefined);
                      setReplayReason('');
                    }}
                  >
                    {stream.service === 'mcp-management' ? '人工归档' : '人工重放'}
                  </Button>
                );
              },
            },
          ]}
        />
      </Card>
      <Modal
        title={queueReplayTitle(replayTarget)}
        open={Boolean(replayTarget)}
        okText={replayTarget?.service === 'mcp-management' ? '确认归档' : '确认重放'}
        cancelText="取消"
        confirmLoading={replay.isPending}
        okButtonProps={{
          danger: true,
          disabled:
            replayItemId.trim().length === 0 ||
            replayReason.trim().length < 8 ||
            (replayTarget?.service === 'memory-engine' &&
              (
                replayTenantId.trim().length === 0 ||
                replaySourceId.trim().length === 0 ||
                !replayVersion ||
                (replayTarget.stream === 'subject_memory' && !replayEventType)
              )) ||
            (replayTarget?.service === 'plugin-management' && !replayVersion),
        }}
        onCancel={() => setReplayTarget(null)}
        onOk={() => replay.mutate()}
      >
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Typography.Text type="secondary">
            {queueReplayDescription(replayTarget)}
          </Typography.Text>
          {replayTarget?.service === 'memory-engine' && (
            <>
              <Input
                value={replayTenantId}
                onChange={(event) => setReplayTenantId(event.target.value)}
                placeholder="Tenant ID"
              />
              <Input
                value={replaySourceId}
                onChange={(event) => setReplaySourceId(event.target.value)}
                placeholder="Source ID"
              />
              <InputNumber
                value={replayVersion}
                onChange={setReplayVersion}
                min={1}
                precision={0}
                placeholder="Dead-letter version"
                style={{ width: '100%' }}
              />
              {replayTarget.stream === 'subject_memory' && (
                <Select
                  value={replayEventType}
                  onChange={setReplayEventType}
                  placeholder="选择 Subject Memory 事件类型"
                  options={[
                    { value: 'source_available', label: '摘要来源事件 (source_available)' },
                    { value: 'scope_requested', label: '记忆范围事件 (scope_requested)' },
                  ]}
                />
              )}
            </>
          )}
          {replayTarget?.service === 'plugin-management' && (
            <InputNumber
              value={replayVersion}
              onChange={setReplayVersion}
              min={1}
              precision={0}
              placeholder="Dead-letter version"
              style={{ width: '100%' }}
            />
          )}
          <Input
            value={replayItemId}
            onChange={(event) => setReplayItemId(event.target.value)}
            placeholder={queueReplayItemPlaceholder(replayTarget)}
          />
          <Input.TextArea
            value={replayReason}
            onChange={(event) => setReplayReason(event.target.value)}
            placeholder={`${replayTarget?.service === 'mcp-management' ? '归档' : '重放'}原因，至少 8 个字符`}
            rows={4}
            maxLength={500}
            showCount
          />
        </Space>
      </Modal>
    </Space>
  );
}

function queueReplayItemPlaceholder(stream: QueueOperationsStream | null) {
  if (!stream || stream.service === 'task-runner') return 'Run ID';
  if (stream.service === 'mcp-management') return 'Invocation ID';
  if (stream.service === 'plugin-management') return 'Marketplace ID';
  if (stream.stream === 'summary') return 'Thread ID';
  if (stream.stream === 'rollup') return 'Summary ID';
  return 'Summary ID 或 Scope Key';
}

function queueReplayTitle(stream: QueueOperationsStream | null) {
  if (stream?.service === 'mcp-management') return '人工归档 MCP 终态死信';
  if (stream?.service === 'memory-engine') return '人工重放 Memory Engine 死信';
  if (stream?.service === 'plugin-management') return '人工重放 Plugin Catalog 死信';
  return '人工重放 Run 后处理死信';
}

function queueReplayDescription(stream: QueueOperationsStream | null) {
  if (stream?.service === 'mcp-management') {
    return '该失败结果已经返回原 AI 调用方。系统只归档与 Invocation 身份及耗尽重试次数完全匹配的旧 DLQ 消息，不会恢复 Outbox、重新投递或再次执行工具。';
  }
  if (stream?.service === 'memory-engine') {
    return '系统只会重放租户、来源、业务 ID 和旧版本完全匹配的死信；新 Outbox 经确认发布后，才归档对应旧消息。';
  }
  if (stream?.service === 'plugin-management') {
    return '系统只会恢复 Marketplace ID 与旧版本完全匹配的 Catalog Outbox；新版本确认发布后，才归档对应旧 DLQ 消息。';
  }
  return '系统将重置该 Run 的后处理 dead-letter 状态、重建 Outbox，并在确认发布后归档匹配的旧 DLQ 消息。';
}

function queueRuntime(stream: QueueOperationsStream, role: 'main' | 'retry' | 'dead_letter') {
  return stream.runtime.queues.find((queue) => queue.role === role) || {
    role,
    name: '',
    messages: 0,
    consumers: 0,
  };
}

function queueHealthTag(stream: QueueOperationsStream) {
  if (!stream.runtime.available) {
    return <Tag color="red">不可观测</Tag>;
  }
  const main = queueRuntime(stream, 'main');
  const deadLetter = queueRuntime(stream, 'dead_letter');
  if (deadLetter.messages > 0) {
    return <Tag color="red">存在死信</Tag>;
  }
  if (main.messages > 0 && main.consumers === 0) {
    return <Tag color="volcano">无消费者</Tag>;
  }
  if (main.messages > 0 || queueRuntime(stream, 'retry').messages > 0) {
    return <Tag color="gold">处理中</Tag>;
  }
  return <Tag color="green">正常</Tag>;
}

function queueCell(
  name: string,
  runtime: { messages: number; consumers: number },
  danger = false,
) {
  return (
    <Space direction="vertical" size={2}>
      <Typography.Text className="queue-name">{name}</Typography.Text>
      <Space size={6}>
        <Tag color={danger && runtime.messages > 0 ? 'red' : runtime.messages > 0 ? 'gold' : 'default'}>
          消息 {runtime.messages}
        </Tag>
        <Tag color={runtime.consumers > 0 ? 'blue' : 'default'}>消费者 {runtime.consumers}</Tag>
      </Space>
    </Space>
  );
}
