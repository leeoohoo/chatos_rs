import { Card, Col, Descriptions, Empty, List, Row, Space, Statistic, Tag } from 'antd';
import { useQuery } from '@tanstack/react-query';
import dayjs from 'dayjs';

import { api } from './api';

export function Dashboard({ environment }: { environment: string }) {
  const effective = useQuery({
    queryKey: ['config-center', 'effective', environment],
    queryFn: () => api.effective(environment),
  });
  const releases = useQuery({
    queryKey: ['config-center', 'releases', environment],
    queryFn: () => api.releases(environment),
  });
  const instances = useQuery({ queryKey: ['config-center', 'instances'], queryFn: api.instances });
  const matching = instances.data?.filter((item) => item.environment === environment) || [];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Row gutter={16}>
        <Col span={6}><Card><Statistic title="当前 Revision" value={effective.data?.revision || 0} /></Card></Col>
        <Col span={6}><Card><Statistic title="已发布版本" value={releases.data?.length || 0} /></Card></Col>
        <Col span={6}><Card><Statistic title="在线实例记录" value={matching.length} /></Card></Col>
        <Col span={6}><Card><Statistic title="待重启实例" value={matching.filter((item) => item.pending_restart_keys.length > 0).length} /></Card></Col>
      </Row>
      <Card title="当前配置">
        <Descriptions column={2}>
          <Descriptions.Item label="环境">{environment}</Descriptions.Item>
          <Descriptions.Item label="Release ID">{effective.data?.release_id || '-'}</Descriptions.Item>
          <Descriptions.Item label="配置数量">{Object.keys(effective.data?.values || {}).length}</Descriptions.Item>
          <Descriptions.Item label="状态"><Tag color="green">已发布</Tag></Descriptions.Item>
        </Descriptions>
      </Card>
      <Card title="最近发布">
        <List
          dataSource={(releases.data || []).slice(0, 6)}
          locale={{ emptyText: <Empty description="暂无发布记录" /> }}
          renderItem={(release) => (
            <List.Item>
              <List.Item.Meta
                title={<Space><Tag color="blue">r{release.revision}</Tag>{release.publish_message}</Space>}
                description={dayjs(release.published_at || release.created_at).format('YYYY-MM-DD HH:mm:ss')}
              />
              <Tag color={release.status === 'published' ? 'green' : 'red'}>{release.status}</Tag>
            </List.Item>
          )}
        />
      </Card>
    </Space>
  );
}
