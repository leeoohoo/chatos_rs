import {
  Button,
  Input,
  Select,
  Segmented,
  Space,
  Switch,
  Typography,
  type SegmentedProps,
  type SelectProps,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskStatus } from '../../types';

type TaskListToolbarProps = {
  t: TranslateFn;
  keywordFilter: string;
  tagFilter?: string;
  modelConfigId?: string;
  statusFilter: 'all' | TaskStatus;
  scheduledOnly: boolean;
  tagOptions: SelectProps['options'];
  modelOptions: SelectProps['options'];
  statusFilterOptions: SegmentedProps['options'];
  onKeywordFilterChange: (value: string) => void;
  onTagFilterChange: (value?: string) => void;
  onModelFilterChange: (value?: string) => void;
  onStatusFilterChange: (value: 'all' | TaskStatus) => void;
  onScheduledOnlyChange: (value: boolean) => void;
  onRefresh: () => void;
  onCreateTask: () => void;
};

export function TaskListToolbar({
  t,
  keywordFilter,
  tagFilter,
  modelConfigId,
  statusFilter,
  scheduledOnly,
  tagOptions,
  modelOptions,
  statusFilterOptions,
  onKeywordFilterChange,
  onTagFilterChange,
  onModelFilterChange,
  onStatusFilterChange,
  onScheduledOnlyChange,
  onRefresh,
  onCreateTask,
}: TaskListToolbarProps) {
  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('tasks.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('tasks.subtitle')}
        </Typography.Text>
      </Space>
      <Space wrap>
        <Input.Search
          allowClear
          placeholder={t('tasks.searchPlaceholder')}
          style={{ width: 260 }}
          value={keywordFilter}
          onChange={(event) => onKeywordFilterChange(event.target.value)}
        />
        <Select
          allowClear
          placeholder={t('tasks.tagFilter')}
          style={{ width: 180 }}
          value={tagFilter}
          options={tagOptions}
          onChange={onTagFilterChange}
        />
        <Select
          allowClear
          placeholder={t('tasks.modelFilter')}
          style={{ width: 220 }}
          value={modelConfigId}
          options={modelOptions}
          onChange={onModelFilterChange}
        />
        <Segmented
          value={statusFilter}
          onChange={(value) => onStatusFilterChange(value as 'all' | TaskStatus)}
          options={statusFilterOptions}
        />
        <Space size={8}>
          <Typography.Text type="secondary">{t('tasks.scheduledOnly')}</Typography.Text>
          <Switch checked={scheduledOnly} onChange={onScheduledOnlyChange} />
        </Space>
        <Button onClick={onRefresh}>{t('common.refresh')}</Button>
        <Button type="primary" onClick={onCreateTask}>
          {t('tasks.newTask')}
        </Button>
      </Space>
    </Space>
  );
}
