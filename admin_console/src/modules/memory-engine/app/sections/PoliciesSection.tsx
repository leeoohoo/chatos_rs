// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Descriptions, Empty, Space, Tabs, Tag, Typography } from 'antd';
import type { TabsProps } from 'antd';
import { Link } from 'react-router-dom';

import type { EngineJobPolicy, EngineJobPolicyCatalog } from '../../types';
import type { PolicyViewKey } from '../types';

const { Text } = Typography;

type PoliciesSectionProps = {
  policies: EngineJobPolicy[];
  configurationStatus: EngineJobPolicyCatalog['configuration_status'] | null;
  loading: boolean;
  selectedKey: PolicyViewKey;
  onSelect: (value: PolicyViewKey) => void;
  onReload: () => void;
};

export function PoliciesSection(props: PoliciesSectionProps) {
  const {
    policies,
    configurationStatus,
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
          <Tag color="blue">随模型计算</Tag>
        </Space>
      ),
      children: (
        <Descriptions
          bordered
          column={2}
          items={[
            { key: 'model', label: '当前模型', children: policy.model ?? '-' },
            { key: 'provider', label: '模型供应商', children: policy.provider ?? '-' },
            { key: 'context', label: '模型上下文上限', children: policy.context_window_tokens ?? '-' },
            { key: 'output', label: '模型输出预留', children: policy.output_reserve_tokens ?? '-' },
            { key: 'input', label: '模型可用输入', children: policy.max_model_input_tokens ?? '-' },
            { key: 'revision', label: '模型能力版本', children: policy.model_capability_revision ?? '-' },
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
            本页显示当前账户选用模型的已发布能力。Memory Engine 按模型可用输入计算任务容量，发送前扣除实际提示词、协议和结构化输出定义的开销。
          </Text>
          <Text type="secondary">
            模型能力由正式配置提供，Prompt 由 Managed Agent 版本管理。缺少模型能力时停止生成并报告原因。
          </Text>
        </Space>
      </Card>
      {policies.length === 0 ? (
        <Card>
          <Empty description={configurationStatus === 'model_not_selected'
            ? '当前账户尚未选择 Memory Engine 摘要模型，生成任务尚不能启动。'
            : loading ? '正在读取模型能力配置' : '尚未加载到模型能力配置'}>
            {configurationStatus === 'model_not_selected' && (
              <Link to="/users/models">前往模型配置选择摘要模型</Link>
            )}
          </Empty>
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
