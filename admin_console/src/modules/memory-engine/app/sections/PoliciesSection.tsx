// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Descriptions, Empty, Space, Tabs, Tag, Typography } from 'antd';
import type { TabsProps } from 'antd';

import type { EngineJobPolicy } from '../../types';
import type { PolicyViewKey } from '../types';

const { Text } = Typography;

type PoliciesSectionProps = {
  policies: EngineJobPolicy[];
  loading: boolean;
  selectedKey: PolicyViewKey;
  onSelect: (value: PolicyViewKey) => void;
  onReload: () => void;
};

export function PoliciesSection(props: PoliciesSectionProps) {
  const {
    policies,
    loading,
    selectedKey,
    onSelect,
    onReload,
  } = props;

  const items: TabsProps['items'] = policies.map((policy) => ({
      key: policy.job_type,
      label: (
        <Space size={8}>
          <span>{policy.job_type}</span>
          <Tag color="blue">全局统一配置</Tag>
        </Space>
      ),
      children: (
        <Descriptions
          bordered
          column={2}
          items={[
            { key: 'context', label: '统一上下文上限', children: policy.context_window_tokens ?? '-' },
            { key: 'output', label: '统一输出预留', children: policy.output_reserve_tokens ?? '-' },
            { key: 'input', label: '统一输入容量', children: policy.max_model_input_tokens ?? '-' },
            { key: 'revision', label: '全局配置版本', children: policy.model_capability_revision ?? '-' },
          ]}
        />
      ),
    }));

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Card
        title="任务策略"
        extra={
          <Button icon={<ReloadOutlined />} loading={loading} onClick={onReload}>
            刷新
          </Button>
        }
      >
        <Space direction="vertical" size={6}>
          <Text type="secondary">
            本页展示配置中心发布的全局任务容量：根据各模型厂商的规格，取所有支持模型都能使用的最大共同容量。切换账户或执行模型不会改变这套配置。
          </Text>
          <Text type="secondary">
            摘要、聚合和主题记忆统一使用该容量，发送请求时扣除提示词、工具及协议开销。模型选择只决定实际调用谁执行任务；查看策略无需选择执行模型。
          </Text>
        </Space>
      </Card>
      {policies.length === 0 ? (
        <Card>
          <Empty description="尚未加载到全局任务容量，点击刷新重试" />
        </Card>
      ) : (
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Tabs
            className="engine-policy-tabs"
            activeKey={selectedKey}
            onChange={(value) => onSelect(value as PolicyViewKey)}
            items={items}
          />
        </Space>
      )}
    </Space>
  );
}
