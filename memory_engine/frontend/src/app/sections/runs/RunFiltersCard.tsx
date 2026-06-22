import { ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Col, Form, Input, InputNumber, Row, Select, Space } from 'antd';

import { JOB_TYPE_OPTIONS, RUN_STATUS_OPTIONS } from '../../constants';
import type { RunsSectionProps } from './types';

const TRIGGER_TYPE_OPTIONS = [
  { label: '线程直触发', value: 'thread_direct' },
  { label: '记忆直触发', value: 'subject_direct' },
  { label: '系统调度', value: 'scheduler' },
];

type RunFiltersCardProps = Pick<
  RunsSectionProps,
  'form' | 'initialValues' | 'loading' | 'onApply' | 'onReset'
>;

export function RunFiltersCard(props: RunFiltersCardProps) {
  const { form, initialValues, loading, onApply, onReset } = props;

  return (
    <Card title="任务运行筛选">
      <Form form={form} layout="vertical" initialValues={initialValues}>
        <Row gutter={[12, 0]}>
          <Col xs={24} md={6}>
            <Form.Item label="任务类型" name="job_type">
              <Select
                allowClear
                placeholder="全部"
                options={JOB_TYPE_OPTIONS.map((value) => ({ label: value, value }))}
              />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="状态" name="status">
              <Select
                allowClear
                placeholder="全部"
                options={RUN_STATUS_OPTIONS.map((value) => ({ label: value, value }))}
              />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="触发方式" name="trigger_type">
              <Select allowClear placeholder="全部" options={TRIGGER_TYPE_OPTIONS} />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="线程 ID" name="thread_id">
              <Input placeholder="按 thread_id 筛选" />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="租户" name="tenant_id">
              <Input placeholder="按租户筛选" />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="来源系统" name="source_id">
              <Input placeholder="按 source_id 筛选" />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="返回条数" name="limit">
              <InputNumber min={1} max={1000} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col span={24}>
            <Space>
              <Button type="primary" loading={loading} onClick={onApply}>
                应用筛选
              </Button>
              <Button onClick={onReset}>重置</Button>
              <Button icon={<ReloadOutlined />} loading={loading} onClick={onApply}>
                刷新
              </Button>
            </Space>
          </Col>
        </Row>
      </Form>
    </Card>
  );
}
