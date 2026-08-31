// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Card, Col, Empty, Row, Skeleton, Space, Statistic, Tag } from 'antd';

import type { DashboardStats } from '../types';

type DashboardSectionProps = {
  loading: boolean;
  dashboardStats: DashboardStats;
  jobStats: Record<string, Record<string, number>>;
};

export function DashboardSection(props: DashboardSectionProps) {
  const { loading, dashboardStats, jobStats } = props;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Skeleton active loading={loading} paragraph={{ rows: 8 }}>
        <Row gutter={[12, 12]}>
          <Col xs={12} lg={4}>
            <Statistic title="接入系统" value={dashboardStats.sources} />
          </Col>
          <Col xs={12} lg={4}>
            <Statistic title="模型配置" value={dashboardStats.models} />
          </Col>
          <Col xs={12} lg={4}>
            <Statistic title="任务策略" value={dashboardStats.policies} />
          </Col>
          <Col xs={12} lg={4}>
            <Statistic title="运行中" value={dashboardStats.running} />
          </Col>
          <Col xs={12} lg={4}>
            <Statistic title="已完成" value={dashboardStats.done} />
          </Col>
          <Col xs={12} lg={4}>
            <Statistic title="失败" value={dashboardStats.failed} />
          </Col>
        </Row>
        <Row gutter={[12, 12]}>
          {Object.keys(jobStats).length === 0 ? (
            <Col span={24}>
              <Card>
                <Empty description="最近没有任务运行记录" />
              </Card>
            </Col>
          ) : (
            Object.entries(jobStats).map(([jobType, stats]) => (
              <Col key={jobType} xs={24} lg={12}>
                <Card size="small" title={<Tag>{jobType}</Tag>}>
                  <Row gutter={[12, 12]}>
                    {Object.entries(stats).map(([status, count]) => (
                      <Col key={status} xs={12}>
                        <Statistic title={status} value={count} />
                      </Col>
                    ))}
                  </Row>
                </Card>
              </Col>
            ))
          )}
        </Row>
      </Skeleton>
    </Space>
  );
}
