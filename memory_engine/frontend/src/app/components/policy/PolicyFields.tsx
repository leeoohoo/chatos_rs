// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Alert, Button, Descriptions, Space, Tag, Typography } from 'antd';

import type { PolicyFieldsProps } from './types';

const { Text } = Typography;

const configCenterUrl =
  (import.meta.env.VITE_CONFIG_CENTER_URL as string | undefined) ||
  'http://localhost:39271';

export function PolicyFields({ policy, meta }: PolicyFieldsProps) {
  const value = (input: string | number | null | undefined, suffix = '') =>
    input === null || input === undefined || input === '' ? '-' : `${input}${suffix}`;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Alert
        type="info"
        showIcon
        message="运行参数已由全局配置中心统一管理"
        description={
          <Space direction="vertical" size="small">
            <Text>
              云端 Memory Engine 和 Local Connector 客户端读取同一份已发布策略；客户端会缓存
              last-known-good，离线时继续使用最近一次有效配置。
            </Text>
            <Text>
              模型供应商、模型名、Base URL 和 API Key 仍在“模型配置”中维护，本页和配置中心都不重复保存模型信息。
            </Text>
            <Button href={configCenterUrl} target="_blank" rel="noreferrer" size="small">
              打开配置中心
            </Button>
          </Space>
        }
      />

      {meta.managedAgentKey ? (
        <Alert
          type="success"
          showIcon
          message={`${meta.promptLabel} 已由系统 Agent 统一管理`}
          description={`请在 Plugin Management 的系统智能体页面编辑并发布 ${meta.managedAgentKey}；运行时会按当前模型厂商读取对应 Published Prompt。`}
        />
      ) : null}

      <Descriptions bordered size="small" column={{ xs: 1, sm: 2, lg: 3 }}>
        <Descriptions.Item label="启用">
          <Tag color={policy.enabled ? 'green' : 'default'}>
            {policy.enabled ? '已启用' : '已停用'}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label={meta.tokenLimitLabel}>
          {value(policy.token_limit)}
        </Descriptions.Item>
        {meta.showTargetSummaryTokens === false ? null : (
          <Descriptions.Item label={meta.targetSummaryTokensLabel}>
            {value(policy.target_summary_tokens)}
          </Descriptions.Item>
        )}
        <Descriptions.Item label={meta.intervalSecondsLabel}>
          {value(policy.interval_seconds, ' 秒')}
        </Descriptions.Item>
        {meta.showMaxThreadsPerTick === false || !meta.maxThreadsPerTickLabel ? null : (
          <Descriptions.Item label={meta.maxThreadsPerTickLabel}>
            {value(policy.max_threads_per_tick)}
          </Descriptions.Item>
        )}
        {meta.countLimitLabel ? (
          <Descriptions.Item label={meta.countLimitLabel}>
            {value(policy.count_limit)}
          </Descriptions.Item>
        ) : null}
        {meta.showKeepLevel0 ? (
          <Descriptions.Item label={meta.keepLevel0Label}>
            {value(policy.keep_level0_count)}
          </Descriptions.Item>
        ) : null}
        {meta.showMaxLevel ? (
          <Descriptions.Item label={meta.maxLevelLabel}>
            {value(policy.max_level)}
          </Descriptions.Item>
        ) : null}
      </Descriptions>
    </Space>
  );
}
