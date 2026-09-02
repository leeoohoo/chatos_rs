// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Input,
  Segmented,
  Select,
  Space,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ModelEnabledFilter } from './modelPageUtils';

type ModelListToolbarProps = {
  t: TranslateFn;
  keywordFilter: string;
  providerFilter: string;
  enabledFilter: ModelEnabledFilter;
  providerOptions: Array<{ label: string; value: string }>;
  enabledFilterOptions: Array<{ label: string; value: string }>;
  onKeywordFilterChange: (value: string) => void;
  onProviderFilterChange: (value: string) => void;
  onEnabledFilterChange: (value: ModelEnabledFilter) => void;
  onClearFilters: () => void;
  onRefresh: () => void;
  onCreate: () => void;
};

export function ModelListToolbar({
  t,
  keywordFilter,
  providerFilter,
  enabledFilter,
  providerOptions,
  enabledFilterOptions,
  onKeywordFilterChange,
  onProviderFilterChange,
  onEnabledFilterChange,
  onClearFilters,
  onRefresh,
  onCreate,
}: ModelListToolbarProps) {
  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('models.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('models.subtitle')}
        </Typography.Text>
      </Space>
      <Space>
        <Input
          allowClear
          placeholder={t('models.searchPlaceholder')}
          style={{ width: 240 }}
          value={keywordFilter}
          onChange={(event) => onKeywordFilterChange(event.target.value)}
        />
        <Select
          style={{ width: 220 }}
          value={providerFilter}
          options={providerOptions}
          onChange={onProviderFilterChange}
        />
        <Segmented
          value={enabledFilter}
          onChange={(value) => onEnabledFilterChange(value as ModelEnabledFilter)}
          options={enabledFilterOptions}
        />
        <Button onClick={onClearFilters}>
          {t('common.clearFilters')}
        </Button>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
        <Button type="primary" onClick={onCreate}>
          {t('models.new')}
        </Button>
      </Space>
    </Space>
  );
}
