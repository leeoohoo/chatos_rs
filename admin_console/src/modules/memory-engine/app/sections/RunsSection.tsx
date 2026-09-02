// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Card, Space, Table, Tabs } from 'antd';

import { runColumns } from './runs/columns';
import { RunFiltersCard } from './runs/RunFiltersCard';
import type { RunsSectionProps } from './runs/types';

export function RunsSection(props: RunsSectionProps) {
  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <RunFiltersCard
        form={props.form}
        initialValues={props.initialValues}
        loading={props.loading}
        onApply={props.onApply}
        onReset={props.onReset}
      />
      <Card title="任务运行">
        <Tabs
          items={[
            {
              key: 'thread',
              label: `接口直触发 (${props.threadRuns.length})`,
              children: (
                <Table
                  rowKey="id"
                  dataSource={props.threadRuns}
                  loading={props.loading}
                  pagination={{ pageSize: 12 }}
                  scroll={{ x: 1900 }}
                  columns={runColumns()}
                />
              ),
            },
            {
              key: 'scheduler',
              label: `系统调度 (${props.schedulerRuns.length})`,
              children: (
                <Table
                  rowKey="id"
                  dataSource={props.schedulerRuns}
                  loading={props.loading}
                  pagination={{ pageSize: 12 }}
                  scroll={{ x: 1900 }}
                  columns={runColumns()}
                />
              ),
            },
          ]}
        />
      </Card>
    </Space>
  );
}
