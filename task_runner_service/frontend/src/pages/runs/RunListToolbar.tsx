// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Segmented,
  Select,
  Space,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RunStatusFilter } from './runPageUtils';

type RunListToolbarProps = {
  t: TranslateFn;
  taskFilterId?: string;
  routeModelConfigId?: string;
  statusFilter: RunStatusFilter;
  taskOptions: Array<{ label: string; value: string }>;
  modelOptions: Array<{ label: string; value: string }>;
  runStatusOptions: Array<{ label: string; value: string }>;
  onTaskSearch: (value: string) => void;
  onTaskFilterChange: (value?: string) => void;
  onModelFilterChange: (value?: string) => void;
  onStatusFilterChange: (value: RunStatusFilter) => void;
  onClearFilters: () => void;
  onRefresh: () => void;
};

export function RunListToolbar({
  t,
  taskFilterId,
  routeModelConfigId,
  statusFilter,
  taskOptions,
  modelOptions,
  runStatusOptions,
  onTaskSearch,
  onTaskFilterChange,
  onModelFilterChange,
  onStatusFilterChange,
  onClearFilters,
  onRefresh,
}: RunListToolbarProps) {
  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('runs.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('runs.subtitle')}
        </Typography.Text>
      </Space>
      <Space>
        <Select
          allowClear
          showSearch
          filterOption={false}
          placeholder={t('runs.taskFilter')}
          style={{ width: 220 }}
          value={taskFilterId}
          options={taskOptions}
          onSearch={onTaskSearch}
          onChange={onTaskFilterChange}
        />
        <Select
          allowClear
          placeholder={t('runs.modelFilter')}
          style={{ width: 220 }}
          value={routeModelConfigId}
          options={modelOptions}
          onChange={onModelFilterChange}
        />
        <Segmented
          value={statusFilter}
          onChange={(value) => onStatusFilterChange(value as RunStatusFilter)}
          options={runStatusOptions}
        />
        <Button onClick={onClearFilters}>
          {t('common.clearFilters')}
        </Button>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
      </Space>
    </Space>
  );
}
