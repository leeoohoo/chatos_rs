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
import type { RemoteServerAuthType } from '../../types';
import type { ServerEnabledFilter } from './serverPageUtils';

type ServerListToolbarProps = {
  t: TranslateFn;
  keywordFilter: string;
  authTypeFilter: 'all' | RemoteServerAuthType;
  enabledFilter: ServerEnabledFilter;
  authTypeFilterOptions: Array<{ label: string; value: string }>;
  enabledFilterOptions: Array<{ label: string; value: string }>;
  onKeywordFilterChange: (value: string) => void;
  onAuthTypeFilterChange: (value: 'all' | RemoteServerAuthType) => void;
  onEnabledFilterChange: (value: ServerEnabledFilter) => void;
  onClearFilters: () => void;
  onRefresh: () => void;
  onCreate: () => void;
};

export function ServerListToolbar({
  t,
  keywordFilter,
  authTypeFilter,
  enabledFilter,
  authTypeFilterOptions,
  enabledFilterOptions,
  onKeywordFilterChange,
  onAuthTypeFilterChange,
  onEnabledFilterChange,
  onClearFilters,
  onRefresh,
  onCreate,
}: ServerListToolbarProps) {
  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('servers.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('servers.subtitle')}
        </Typography.Text>
      </Space>
      <Space wrap>
        <Input
          allowClear
          placeholder={t('servers.searchPlaceholder')}
          style={{ width: 260 }}
          value={keywordFilter}
          onChange={(event) => onKeywordFilterChange(event.target.value)}
        />
        <Select
          style={{ width: 180 }}
          value={authTypeFilter}
          options={authTypeFilterOptions}
          onChange={(value) => onAuthTypeFilterChange(value as 'all' | RemoteServerAuthType)}
        />
        <Segmented
          value={enabledFilter}
          onChange={(value) => onEnabledFilterChange(value as ServerEnabledFilter)}
          options={enabledFilterOptions}
        />
        <Button onClick={onClearFilters}>
          {t('common.clearFilters')}
        </Button>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
        <Button type="primary" onClick={onCreate}>
          {t('servers.new')}
        </Button>
      </Space>
    </Space>
  );
}
