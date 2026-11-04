// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined, SearchOutlined } from '@ant-design/icons';
import { Button, Card, Form, Input, InputNumber, Space, Typography } from 'antd';
import { useEffect } from 'react';

import type { AgentFiltersCardProps } from './types';

const { Text } = Typography;

export function AgentFiltersCard(props: AgentFiltersCardProps) {
  useEffect(() => {
    props.form.setFieldsValue(props.initialValues);
  }, [props.form, props.initialValues]);

  return (
    <Card className="engine-data-filters-card">
      <Form
        form={props.form}
        layout="vertical"
        initialValues={props.initialValues}
        onFinish={props.onApplyFilters}
      >
        <div className="engine-agent-filter-grid">
          <Form.Item label="用户 / Tenant ID" name="tenant_id">
            <Input allowClear placeholder="留空查看当前权限范围" />
          </Form.Item>
          <Form.Item label="Source ID" name="source_id">
            <Input allowClear placeholder="例如 chatos" />
          </Form.Item>
          <Form.Item label="Agent ID" name="agent_id">
            <Input allowClear placeholder="main-chat / approval agent" />
          </Form.Item>
          <Form.Item label="返回数量" name="limit">
            <InputNumber min={1} max={500} style={{ width: '100%' }} />
          </Form.Item>
          <div className="engine-agent-filter-actions">
            <Space wrap>
              <Text type="secondary">先选 Agent，再查看它的记忆、项目与会话。</Text>
              <Button onClick={props.onResetFilters}>重置</Button>
              <Button
                icon={<ReloadOutlined />}
                loading={props.loading}
                onClick={props.onReload}
              >
                刷新
              </Button>
              <Button
                htmlType="submit"
                type="primary"
                icon={<SearchOutlined />}
                loading={props.loading}
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
