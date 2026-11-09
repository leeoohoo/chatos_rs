// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Alert, App, Button, Card, Form, Input, InputNumber, Select, Space, Table, Tabs, Tag } from 'antd';
import { useState } from 'react';

import { api } from '../../api';
import type {
  ComposeDryRunItem,
  DerivationLineage,
  PartitionRebuildBacklogItem,
  RecordCoverage,
  ReliabilityOperatorAction,
  ReliabilityQueueItem,
  ReliabilityMigrationRun,
  ReliabilityDashboard,
  ReliabilityScope,
} from '../../types';

type ReliabilityForm = ReliabilityScope & {
  record_id?: string;
  output_kind?: 'thread_summary' | 'subject_memory';
  output_id?: string;
  dry_thread_id?: string;
  dry_project_id?: string;
  dry_query?: string;
  model_capability_revision?: string;
  max_memory_tokens?: number;
  rearm_reason?: string;
  resummarize_summary_id?: string;
  resummarize_reason?: string;
  restore_record_id?: string;
  restore_reason?: string;
  queue_rearm_reason?: string;
};

export function ReliabilitySection() {
  const { message, modal } = App.useApp();
  const [form] = Form.useForm<ReliabilityForm>();
  const [loading, setLoading] = useState(false);
  const [runs, setRuns] = useState<ReliabilityMigrationRun[]>([]);
  const [backlog, setBacklog] = useState<PartitionRebuildBacklogItem[]>([]);
  const [coverage, setCoverage] = useState<RecordCoverage[]>([]);
  const [lineage, setLineage] = useState<DerivationLineage[]>([]);
  const [dryRun, setDryRun] = useState<ComposeDryRunItem[]>([]);
  const [dryRunMeta, setDryRunMeta] = useState<unknown>(null);
  const [queues, setQueues] = useState<ReliabilityQueueItem[]>([]);
  const [actions, setActions] = useState<ReliabilityOperatorAction[]>([]);
  const [dashboard, setDashboard] = useState<ReliabilityDashboard | null>(null);

  const scope = async (): Promise<ReliabilityForm> => {
    const values = await form.validateFields(['tenant_id', 'source_id']);
    return { ...values, limit: 500 };
  };
  const execute = async (work: () => Promise<void>) => {
    setLoading(true);
    try {
      await work();
    } catch (error) {
      message.error(error instanceof Error ? error.message : '加载可靠性数据失败');
    } finally {
      setLoading(false);
    }
  };
  const rearm = (item: PartitionRebuildBacklogItem) => {
    void form.validateFields(['tenant_id', 'source_id', 'rearm_reason']).then((values) => {
      modal.confirm({
        title: '确认重新武装失败的重建项？',
        content: `${item.thread_id} / ${item.project_partition_key} / ${item.rebuild_kind}`,
        okText: '重新入队',
        okButtonProps: { danger: true },
        onOk: async () => execute(async () => {
          await api.rearmPartitionRebuild({
            tenant_id: values.tenant_id!,
            source_id: values.source_id!,
            thread_id: item.thread_id,
            project_partition_key: item.project_partition_key,
            rebuild_kind: item.rebuild_kind,
            reason: values.rearm_reason!,
          });
          setBacklog(await api.listPartitionRebuildBacklog(await scope()));
          message.success('失败重建项已重新入队');
        }),
      });
    });
  };

  return (
    <Form form={form}>
      <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Alert
        type="info"
        showIcon
        message="这里展示正式 coverage、完整 lineage、历史迁移和分区重建状态；不展示原始敏感内容。"
      />
      <Card title="可靠性范围">
        <Space align="start">
          <Form.Item name="tenant_id" label="Tenant" rules={[{ required: true }]}>
            <Input style={{ width: 280 }} />
          </Form.Item>
          <Form.Item name="source_id" label="Source" rules={[{ required: true }]}>
            <Input style={{ width: 220 }} />
          </Form.Item>
          <Button
            type="primary"
            loading={loading}
            onClick={() => void execute(async () => {
              const values = await scope();
              const [nextDashboard, nextRuns, nextBacklog, nextQueues, nextActions] = await Promise.all([
                api.getReliabilityDashboard(values),
                api.listReliabilityMigrations(values),
                api.listPartitionRebuildBacklog(values),
                api.listReliabilityQueues(values),
                api.listReliabilityOperatorActions(values),
              ]);
              setDashboard(nextDashboard);
              setRuns(nextRuns);
              setBacklog(nextBacklog);
              setQueues(nextQueues);
              setActions(nextActions);
            })}
          >
            加载迁移与重建状态
          </Button>
        </Space>
      </Card>
      <Tabs items={[
        {
          key: 'dashboard',
          label: `可靠性 Dashboard (${dashboard?.finding_count ?? 0})`,
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Alert
              type={dashboard?.alerts.some((alert) => alert.severity === 'critical') ? 'error' : 'success'}
              showIcon
              message={dashboard ? `审计时间 ${dashboard.generated_at}；发现 ${dashboard.finding_count} 项` : '请先加载可靠性范围'}
              description={compactJSON(dashboard?.baseline)}
            />
            <Table rowKey="code" dataSource={dashboard?.alerts ?? []} pagination={false} columns={[
              { title: '告警代码', dataIndex: 'code' },
              { title: '级别', dataIndex: 'severity', width: 120, render: statusTag },
              { title: '数量', dataIndex: 'count', width: 100 },
            ]} />
            <Table rowKey="component" dataSource={dashboard?.runtime_readiness.components ?? []} pagination={false} columns={[
              { title: '运行面', dataIndex: 'component' },
              { title: '状态', dataIndex: 'state', width: 120, render: statusTag },
              { title: 'Source Unit', dataIndex: 'source_unit_ready', width: 120, render: (value) => String(value) },
              { title: 'Assertion', dataIndex: 'runtime_assertion_ready', width: 120, render: (value) => String(value) },
              { title: 'Capability', dataIndex: 'model_capability_ready', width: 120, render: (value) => String(value) },
              { title: '缺失', dataIndex: 'missing', render: compactJSON },
            ]} />
          </Space>,
        },
        {
          key: 'migrations',
          label: `迁移运行 (${runs.length})`,
          children: <Table rowKey="id" loading={loading} dataSource={runs} scroll={{ x: 1300 }} columns={[
            { title: 'Run', dataIndex: 'id', width: 250 },
            { title: '类型', dataIndex: 'migration_kind', width: 190 },
            { title: '状态', dataIndex: 'status', width: 100, render: statusTag },
            { title: '开始', dataIndex: 'started_at', width: 190 },
            { title: '完成', dataIndex: 'finished_at', width: 190 },
            { title: '统计', dataIndex: 'statistics', render: compactJSON },
            { title: '错误', dataIndex: 'error_detail', render: compactJSON },
          ]} />,
        },
        {
          key: 'backlog',
          label: `分区重建 (${backlog.length})`,
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Form.Item name="rearm_reason" label="重新入队原因" rules={[{ min: 8 }]}>
              <Input style={{ width: 520 }} placeholder="仅失败项可重入队；原因至少 8 个字符" />
            </Form.Item>
            <Table rowKey={(item) => `${item.thread_id}:${item.project_partition_key}:${item.rebuild_kind}`} loading={loading} dataSource={backlog} scroll={{ x: 1700 }} columns={[
              { title: 'Thread', dataIndex: 'thread_id', width: 250 },
              { title: 'Partition', dataIndex: 'project_partition_key', width: 220 },
              { title: '重建类型', dataIndex: 'rebuild_kind', width: 160 },
              { title: '状态', dataIndex: 'status', width: 100, render: statusTag },
              { title: '尝试', dataIndex: 'attempt_count', width: 80 },
              { title: '原因', dataIndex: 'reason', width: 280 },
              { title: '更新', dataIndex: 'updated_at', width: 190 },
              { title: '错误', dataIndex: 'last_error', render: compactJSON },
              { title: '操作', width: 120, fixed: 'right', render: (_, item) => (
                <Button danger size="small" disabled={item.status !== 'failed'} onClick={() => rearm(item)}>
                  重新入队
                </Button>
              ) },
            ]} />
          </Space>,
        },
        {
          key: 'queues',
          label: `处理队列 (${queues.length})`,
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Form.Item name="queue_rearm_reason" label="Dead-letter 重新入队原因" rules={[{ min: 8 }]}>
              <Input style={{ width: 520 }} placeholder="至少 8 个字符，写入操作者审计" />
            </Form.Item>
            <Table rowKey={(item) => `${item.stream}:${item.item_id}:${item.version}`} loading={loading} dataSource={queues} scroll={{ x: 1450 }} columns={[
            { title: '队列', dataIndex: 'stream', width: 180 },
            { title: 'Item', dataIndex: 'item_id', width: 320 },
            { title: 'Partition', dataIndex: 'project_partition_key', width: 220 },
            { title: '状态', dataIndex: 'status', width: 110, render: statusTag },
            { title: '版本/尝试', dataIndex: 'version', width: 100 },
            { title: '错误码', dataIndex: 'error_code', width: 240 },
            { title: '更新', dataIndex: 'updated_at', width: 190 },
            { title: '操作', width: 120, fixed: 'right', render: (_, item) => (
              <Button danger size="small" disabled={item.status !== 'dead_letter'} onClick={() => { void form.validateFields([
                'tenant_id', 'source_id', 'queue_rearm_reason',
              ]).then((values) => execute(async () => {
                await api.rearmReliabilityQueue({
                  tenant_id: values.tenant_id!, source_id: values.source_id!, stream: item.stream,
                  item_id: item.item_id, project_partition_key: item.project_partition_key || undefined,
                  version: item.version, reason: values.queue_rearm_reason!,
                });
                setQueues(await api.listReliabilityQueues(await scope()));
                message.success('Dead-letter 项已重新入队');
              })); }}>重新入队</Button>
            ) },
          ]} />
          </Space>,
        },
        {
          key: 'dry-run',
          label: 'Compose Dry-run',
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Space wrap align="start">
              <Form.Item name="dry_thread_id" label="Thread ID" rules={[{ required: true }]}>
                <Input style={{ width: 330 }} />
              </Form.Item>
              <Form.Item name="dry_project_id" label="Project Subject">
                <Input style={{ width: 240 }} placeholder="project:..." />
              </Form.Item>
              <Form.Item name="model_capability_revision" label="Capability Revision" rules={[{ required: true }]}>
                <Input style={{ width: 260 }} />
              </Form.Item>
              <Form.Item name="max_memory_tokens" label="Memory Token" initialValue={12000} rules={[{ required: true }]}>
                <InputNumber min={512} max={1000000} />
              </Form.Item>
            </Space>
            <Space align="start">
              <Form.Item name="dry_query" label="检索文本">
                <Input.TextArea style={{ width: 640 }} autoSize={{ minRows: 2, maxRows: 5 }} />
              </Form.Item>
              <Button type="primary" loading={loading} onClick={() => void execute(async () => {
                const values = await form.validateFields([
                  'tenant_id', 'source_id', 'dry_thread_id', 'dry_project_id', 'dry_query',
                  'model_capability_revision', 'max_memory_tokens',
                ]);
                const max = values.max_memory_tokens!;
                const result = await api.composeReliabilityDryRun({
                  tenant_id: values.tenant_id,
                  source_id: values.source_id,
                  thread_id: values.dry_thread_id!,
                  context_project_subject_id: values.dry_project_id || undefined,
                  query_text: values.dry_query || undefined,
                  model_capability_revision: values.model_capability_revision!,
                  policy: {
                    max_memory_tokens: max,
                    max_recent_record_tokens: Math.min(4000, max),
                    max_thread_summary_tokens: Math.min(4000, max),
                    max_project_memory_tokens: Math.min(3000, max),
                    max_agent_memory_tokens: Math.min(2000, max),
                  },
                });
                setDryRun(result.selected);
                setDryRunMeta(result.meta);
              })}>执行只读 Dry-run</Button>
            </Space>
            <Alert type="info" showIcon message="只返回 ID、类型、分数、token 和选择原因，不返回记忆正文或查询文本。" description={compactJSON(dryRunMeta)} />
            <Table rowKey="id" loading={loading} dataSource={dryRun} scroll={{ x: 1450 }} columns={[
              { title: 'ID', dataIndex: 'id', width: 320 },
              { title: '类型', dataIndex: 'item_type', width: 200 },
              { title: '池', dataIndex: 'pool', width: 180 },
              { title: 'Partition', dataIndex: 'project_partition_key', width: 220 },
              { title: 'Token', dataIndex: 'estimated_tokens', width: 100 },
              { title: '分数', dataIndex: 'score_components', width: 320, render: compactJSON },
              { title: '选择原因', dataIndex: 'selection_reason' },
            ]} />
          </Space>,
        },
        {
          key: 'operator-actions',
          label: `写操作审计 (${actions.length})`,
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Alert type="warning" showIcon message="Covered-range 重汇总会冻结现有 active coverage，在模型生成成功后原子替换 Summary 与 coverage，并退出旧下游召回。" />
            <Space align="start" wrap>
              <Form.Item name="resummarize_summary_id" label="Summary ID" rules={[{ required: true }]}>
                <Input style={{ width: 420 }} />
              </Form.Item>
              <Form.Item name="resummarize_reason" label="重汇总原因" rules={[{ required: true }, { min: 8 }]}>
                <Input style={{ width: 420 }} placeholder="至少 8 个字符，写入操作者审计" />
              </Form.Item>
              <Button danger loading={loading} onClick={() => { void form.validateFields([
                'tenant_id', 'source_id', 'resummarize_summary_id', 'resummarize_reason',
              ]).then((values) => {
                modal.confirm({
                  title: '确认原子重汇总该 covered range？',
                  content: values.resummarize_summary_id,
                  okText: '开始重汇总',
                  okButtonProps: { danger: true },
                  onOk: async () => execute(async () => {
                    const result = await api.resummarizeCoveredRange({
                      tenant_id: values.tenant_id!,
                      source_id: values.source_id!,
                      summary_id: values.resummarize_summary_id!,
                      reason: values.resummarize_reason!,
                    });
                    setActions(await api.listReliabilityOperatorActions(await scope()));
                    message.success(result.running ? `重汇总任务已受理：${result.job_run_id}` : `重汇总已完成：${result.summary_id}`);
                  }),
                });
              }); }}>Covered-range 重汇总</Button>
              <Button danger loading={loading} onClick={() => { void form.validateFields([
                'tenant_id', 'source_id', 'resummarize_summary_id', 'resummarize_reason',
              ]).then((values) => {
                modal.confirm({
                  title: '确认退休 Summary 并恢复其完整 covered range 为 pending？',
                  content: values.resummarize_summary_id,
                  okText: '退休并恢复 pending', okButtonProps: { danger: true },
                  onOk: async () => execute(async () => {
                    await api.retireSummary({
                      tenant_id: values.tenant_id!, source_id: values.source_id!,
                      summary_id: values.resummarize_summary_id!, reason: values.resummarize_reason!,
                    });
                    setActions(await api.listReliabilityOperatorActions(await scope()));
                    message.success('Summary 已退休，covered records 已恢复 pending');
                  }),
                });
              }); }}>退休 Summary</Button>
            </Space>
            <Space align="start" wrap>
              <Form.Item name="restore_record_id" label="无 active coverage 的 Record ID" rules={[{ required: true }]}>
                <Input style={{ width: 420 }} />
              </Form.Item>
              <Form.Item name="restore_reason" label="恢复 pending 原因" rules={[{ required: true }, { min: 8 }]}>
                <Input style={{ width: 420 }} />
              </Form.Item>
              <Button danger loading={loading} onClick={() => { void form.validateFields([
                'tenant_id', 'source_id', 'restore_record_id', 'restore_reason',
              ]).then((values) => execute(async () => {
                await api.restoreRecordPending({
                  tenant_id: values.tenant_id!, source_id: values.source_id!,
                  record_id: values.restore_record_id!, reason: values.restore_reason!,
                });
                setActions(await api.listReliabilityOperatorActions(await scope()));
                message.success('Record 已恢复 pending');
              })); }}>恢复 pending</Button>
            </Space>
            <Table rowKey="id" loading={loading} dataSource={actions} scroll={{ x: 1800 }} columns={[
            { title: '时间', dataIndex: 'created_at', width: 190 },
            { title: '操作者', dataIndex: 'operator_id', width: 220 },
            { title: '操作', dataIndex: 'action_kind', width: 150 },
            { title: '对象类型', dataIndex: 'object_kind', width: 170 },
            { title: '对象', dataIndex: 'object_id', width: 320 },
            { title: 'Partition', dataIndex: 'project_partition_key', width: 220 },
            { title: '原因', dataIndex: 'reason', width: 280 },
            { title: '前状态', dataIndex: 'before_state', width: 320, render: compactJSON },
            { title: '后状态', dataIndex: 'after_state', width: 320, render: compactJSON },
          ]} />
          </Space>,
        },
        {
          key: 'coverage',
          label: 'Record Coverage',
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Space>
              <Form.Item name="record_id" label="Record ID" rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                <Input style={{ width: 420 }} />
              </Form.Item>
              <Button loading={loading} onClick={() => void execute(async () => {
                const values = await form.validateFields(['tenant_id', 'source_id', 'record_id']);
                setCoverage(await api.getRecordCoverage(values.record_id!, values));
              })}>查询 active/history coverage</Button>
            </Space>
            <Table rowKey="id" loading={loading} dataSource={coverage} scroll={{ x: 1450 }} columns={[
              { title: 'Summary', dataIndex: 'summary_id', width: 260 },
              { title: 'Partition', dataIndex: 'project_partition_key', width: 220 },
              { title: '序号', dataIndex: 'ordinal', width: 80 },
              { title: 'Source Ref', dataIndex: 'source_ref', width: 260 },
              { title: '状态', dataIndex: 'status', width: 110, render: statusTag },
              { title: 'Generation', dataIndex: 'generation_id', width: 260 },
              { title: '退休原因', dataIndex: 'retired_reason' },
            ]} />
          </Space>,
        },
        {
          key: 'lineage',
          label: '完整 Lineage',
          children: <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Space>
              <Form.Item name="output_kind" label="输出类型" initialValue="thread_summary" style={{ marginBottom: 0 }}>
                <Select style={{ width: 180 }} options={[
                  { value: 'thread_summary', label: 'Thread Summary' },
                  { value: 'subject_memory', label: 'Subject Memory' },
                ]} />
              </Form.Item>
              <Form.Item name="output_id" label="Output ID" rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                <Input style={{ width: 420 }} />
              </Form.Item>
              <Button loading={loading} onClick={() => void execute(async () => {
                const values = await form.validateFields(['tenant_id', 'source_id', 'output_kind', 'output_id']);
                setLineage(await api.getOutputLineage(values.output_kind!, values.output_id!, values));
              })}>查询直接输入与 lineage</Button>
            </Space>
            <Table rowKey="id" loading={loading} dataSource={lineage} scroll={{ x: 1600 }} columns={[
              { title: '派生类型', dataIndex: 'derivation_kind', width: 180 },
              { title: '输入类型', dataIndex: 'input_kind', width: 160 },
              { title: 'Input ID', dataIndex: 'input_id', width: 280 },
              { title: 'Partition', dataIndex: 'input_project_partition_key', width: 220 },
              { title: '序号', dataIndex: 'ordinal', width: 80 },
              { title: '状态', dataIndex: 'status', width: 110, render: statusTag },
              { title: 'Generation', dataIndex: 'generation_id', width: 280 },
              { title: '退休原因', dataIndex: 'retired_reason' },
            ]} />
          </Space>,
        },
      ]} />
      </Space>
    </Form>
  );
}

function statusTag(status: string) {
  const color = status === 'completed' || status === 'active'
    ? 'green'
    : status === 'failed' || status === 'retired'
      ? 'red'
      : status === 'running'
        ? 'blue'
        : 'gold';
  return <Tag color={color}>{status}</Tag>;
}

function compactJSON(value: unknown) {
  if (value == null) return '-';
  const text = JSON.stringify(value);
  return text.length > 300 ? `${text.slice(0, 300)}…` : text;
}
