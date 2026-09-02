// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Empty, Space, Tabs, Tag, Typography } from 'antd';
import type { TabsProps } from 'antd';

import { PolicyEditorCard } from '../components/PolicyEditorCard';
import { PIPELINE_POLICY_META, PIPELINE_POLICY_VIEWS } from '../constants';
import type { EngineJobPolicy } from '../../types';
import type {
  JobTypeKey,
  PolicyMap,
  PolicyViewKey,
} from '../types';

const { Text } = Typography;

type PoliciesSectionProps = {
  policies: EngineJobPolicy[];
  loading: boolean;
  selectedKey: PolicyViewKey;
  onSelect: (value: PolicyViewKey) => void;
  onReload: () => void;
  policyMap: PolicyMap;
};

export function PoliciesSection(props: PoliciesSectionProps) {
  const {
    policies,
    loading,
    selectedKey,
    onSelect,
    onReload,
    policyMap,
  } = props;

  const items: TabsProps['items'] = PIPELINE_POLICY_VIEWS.map((view) => {
    const policy = policyMap[view.jobType as JobTypeKey];
    if (!policy) {
      return null;
    }
    const meta = PIPELINE_POLICY_META[view.key];
    return {
      key: view.key,
      label: (
        <Space size={8}>
          <span>{meta.tabLabel}</span>
          <Tag color={meta.tagColor}>{policy.job_type}</Tag>
        </Space>
      ),
      children: (
        <PolicyEditorCard
          policy={policy}
          meta={meta}
        />
      ),
    };
  }).filter((item): item is NonNullable<typeof item> => Boolean(item));

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
            主链路按四个阶段推进：消息总结 {'->'} 总结再总结 {'->'} 总结生成记忆 {'->'} 记忆再总结。
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
