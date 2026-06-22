import { Space, Statistic } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';

type ModelStatsBarProps = {
  t: TranslateFn;
  visibleCount: number;
  enabledCount: number;
  taskCount: number;
  runCount: number;
};

export function ModelStatsBar({
  t,
  visibleCount,
  enabledCount,
  taskCount,
  runCount,
}: ModelStatsBarProps) {
  return (
    <Space size="large" wrap>
      <Statistic title={t('models.visible')} value={visibleCount} />
      <Statistic title={t('models.enabledCount')} value={enabledCount} />
      <Statistic title={t('models.column.boundTasks')} value={taskCount} />
      <Statistic title={t('models.runRecords')} value={runCount} />
    </Space>
  );
}
