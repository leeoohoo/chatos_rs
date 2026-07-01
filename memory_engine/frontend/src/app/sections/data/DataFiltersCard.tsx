// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined, SearchOutlined } from '@ant-design/icons';
import { Button, Card, Form, Input, InputNumber, Select, Space, Typography } from 'antd';
import { useEffect } from 'react';

import type { DataFiltersCardProps } from './types';

const { Text } = Typography;

const STATUS_OPTIONS = [
  { label: 'Active', value: 'active' },
  { label: 'Archived', value: 'archived' },
  { label: 'All', value: '' },
];

export function DataFiltersCard(props: DataFiltersCardProps) {
  const {
    form,
    initialValues,
    threadsLoading,
    onApplyFilters,
    onResetFilters,
    onReload,
  } = props;

  useEffect(() => {
    form.setFieldsValue(initialValues);
  }, [form, initialValues]);

  return (
    <Card className="engine-data-filters-card">
      <Form
        form={form}
        layout="vertical"
        initialValues={initialValues}
        onFinish={onApplyFilters}
      >
        <div className="engine-data-filter-grid">
          <Form.Item label="Tenant ID" name="tenant_id">
            <Input allowClear placeholder="用户 / 租户 ID" />
          </Form.Item>
          <Form.Item label="Source ID" name="source_id">
            <Input allowClear placeholder="chatos / task" />
          </Form.Item>
          <Form.Item label="Subject ID" name="subject_id">
            <Input allowClear placeholder="session / agent" />
          </Form.Item>
          <Form.Item label="外部线程" name="external_thread_id">
            <Input allowClear placeholder="external_thread_id" />
          </Form.Item>
          <Form.Item label="线程标签" name="thread_label">
            <Input allowClear placeholder="thread_label" />
          </Form.Item>
          <Form.Item label="状态" name="status">
            <Select options={STATUS_OPTIONS} />
          </Form.Item>
        </div>
        <div className="engine-data-filter-grid engine-data-filter-grid--advanced">
          <Form.Item label="Session ID" name="session_id">
            <Input allowClear placeholder="session_id" />
          </Form.Item>
          <Form.Item label="Contact ID" name="contact_id">
            <Input allowClear placeholder="contact_id" />
          </Form.Item>
          <Form.Item label="Project ID" name="project_id">
            <Input allowClear placeholder="project_id" />
          </Form.Item>
          <Form.Item label="Agent ID" name="agent_id">
            <Input allowClear placeholder="agent_id" />
          </Form.Item>
          <Form.Item label="Mapping Source" name="mapping_source">
            <Input allowClear placeholder="mapping_source" />
          </Form.Item>
          <Form.Item label="Mapping Version" name="mapping_version">
            <Input allowClear placeholder="mapping_version" />
          </Form.Item>
        </div>
        <div className="engine-data-filter-toolbar">
          <Form.Item className="engine-data-filter-limit" label="返回数量" name="limit">
            <InputNumber min={1} max={500} style={{ width: '100%' }} />
          </Form.Item>
          <div className="engine-data-filter-actions">
            <Space wrap>
              <Text className="engine-filter-hint" type="secondary">
                按 tenant/source/thread 维度查看 Memory Engine 数据。
              </Text>
              <Button onClick={onResetFilters}>重置</Button>
              <Button icon={<ReloadOutlined />} loading={threadsLoading} onClick={onReload}>
                刷新
              </Button>
              <Button
                htmlType="submit"
                type="primary"
                icon={<SearchOutlined />}
                loading={threadsLoading}
              >
                查询
              </Button>
            </Space>
          </div>
        </div>
      </Form>
    </Card>
  );
}
