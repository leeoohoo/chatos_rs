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
          <Tag color="blue">已发布</Tag>
        </Space>
      ),
      children: (
        <Descriptions
          bordered
          column={2}
          items={[
            { key: 'dispatch', label: '派发阈值', children: policy.dispatch_threshold_tokens ?? '-' },
            { key: 'source', label: '源批次硬上限', children: policy.max_source_batch_tokens ?? '-' },
            { key: 'model', label: '模型输入硬上限', children: policy.max_model_input_tokens ?? '-' },
            { key: 'leaf', label: '叶节点硬上限', children: policy.max_leaf_tokens ?? '-' },
            { key: 'merge', label: '最大归并轮数', children: policy.max_merge_rounds ?? '-' },
            { key: 'count', label: '归并条数', children: policy.count_limit ?? '-' },
            { key: 'keep', label: '保留 L0', children: policy.keep_level0_count ?? '-' },
            { key: 'level', label: '最大层级', children: policy.max_level ?? '-' },
            { key: 'updated', label: '配置快照时间', children: policy.updated_at, span: 2 },
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
            当前页面展示配置中心已经发布并实际生效的 Memory Policy，不再在 Memory Engine 内单独保存。
          </Text>
          <Text type="secondary">
            Prompt 由 Managed Agent 版本管理；本页只读展示当前配置快照，Memory Engine 不提供本地修改或兼容入口。
          </Text>
        </Space>
      </Card>
      {policies.length === 0 ? (
        <Card>
          <Empty description="暂无任务策略，点击右上角刷新重试" />
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
