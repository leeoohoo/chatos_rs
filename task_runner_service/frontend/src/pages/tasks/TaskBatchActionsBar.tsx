import { Button, Space, Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';

type TaskBatchActionsBarProps = {
  t: TranslateFn;
  selectedCount: number;
  hasSelectedTasks: boolean;
  pending: boolean;
  batchRunLoading: boolean;
  batchUpdateLoading: boolean;
  batchDeleteLoading: boolean;
  onOpenBatchRun: () => void;
  onSetReady: () => void;
  onArchive: () => void;
  onDelete: () => void;
};

export function TaskBatchActionsBar({
  t,
  selectedCount,
  hasSelectedTasks,
  pending,
  batchRunLoading,
  batchUpdateLoading,
  batchDeleteLoading,
  onOpenBatchRun,
  onSetReady,
  onArchive,
  onDelete,
}: TaskBatchActionsBarProps) {
  const disabled = !hasSelectedTasks || pending;

  return (
    <Space style={{ justifyContent: 'space-between', width: '100%' }} wrap>
      <Typography.Text type="secondary">
        {t('tasks.selectedCount', { count: selectedCount })}
      </Typography.Text>
      <Space wrap>
        <Button disabled={disabled} loading={batchRunLoading} onClick={onOpenBatchRun}>
          {t('tasks.batchRun')}
        </Button>
        <Button disabled={disabled} loading={batchUpdateLoading} onClick={onSetReady}>
          {t('tasks.setReady')}
        </Button>
        <Button disabled={disabled} loading={batchUpdateLoading} onClick={onArchive}>
          {t('tasks.batchArchive')}
        </Button>
        <Button danger disabled={disabled} loading={batchDeleteLoading} onClick={onDelete}>
          {t('tasks.batchDelete')}
        </Button>
      </Space>
    </Space>
  );
}
