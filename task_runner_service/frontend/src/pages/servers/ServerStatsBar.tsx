import { Space, Statistic } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';

type ServerStatsBarProps = {
  t: TranslateFn;
  visible: number;
  enabled: number;
  testPassed: number;
  strict: number;
};

export function ServerStatsBar({
  t,
  visible,
  enabled,
  testPassed,
  strict,
}: ServerStatsBarProps) {
  return (
    <Space size="large" wrap>
      <Statistic title={t('servers.visible')} value={visible} />
      <Statistic title={t('servers.enabledCount')} value={enabled} />
      <Statistic title={t('servers.testPassed')} value={testPassed} />
      <Statistic title={t('servers.strictCheck')} value={strict} />
    </Space>
  );
}
