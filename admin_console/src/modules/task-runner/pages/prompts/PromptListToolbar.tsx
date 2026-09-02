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
import type { AskUserPromptStatus } from '../../types';
import type { PromptStatusFilter } from './promptPageUtils';

type PromptListToolbarProps = {
  t: TranslateFn;
  taskFilterId?: string;
  runFilterId?: string;
  statusFilter: PromptStatusFilter;
  taskOptions: Array<{ label: string; value: string }>;
  runOptions: Array<{ label: string; value: string }>;
  promptStatusOptions: Array<{ label: string; value: string }>;
  onTaskSearch: (value: string) => void;
  onRunSearch: (value: string) => void;
  onFilterChange: (key: string, value?: string) => void;
  onClearFilters: () => void;
  onRefresh: () => void;
};

export function PromptListToolbar({
  t,
  taskFilterId,
  runFilterId,
  statusFilter,
  taskOptions,
  runOptions,
  promptStatusOptions,
  onTaskSearch,
  onRunSearch,
  onFilterChange,
  onClearFilters,
  onRefresh,
}: PromptListToolbarProps) {
  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('prompts.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('prompts.subtitle')}
        </Typography.Text>
      </Space>
      <Space>
        <Select
          allowClear
          showSearch
          filterOption={false}
          placeholder={t('prompts.taskFilter')}
          style={{ width: 220 }}
          value={taskFilterId}
          options={taskOptions}
          onSearch={onTaskSearch}
          onChange={(value: string | undefined) => onFilterChange('task_id', value)}
        />
        <Select
          allowClear
          showSearch
          filterOption={false}
          placeholder={t('prompts.runFilter')}
          style={{ width: 220 }}
          value={runFilterId}
          options={runOptions}
          onSearch={onRunSearch}
          onChange={(value: string | undefined) => onFilterChange('run_id', value)}
        />
        <Segmented
          value={statusFilter}
          onChange={(value) =>
            onFilterChange(
              'status',
              value === 'all' ? undefined : (value as AskUserPromptStatus),
            )
          }
          options={promptStatusOptions}
        />
        <Button onClick={onClearFilters}>
          {t('common.clearFilters')}
        </Button>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
      </Space>
    </Space>
  );
}
