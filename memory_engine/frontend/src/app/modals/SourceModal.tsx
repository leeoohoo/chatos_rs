// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Col, Form, Input, Modal, Row, Switch, Typography } from 'antd';
import type { FormInstance } from 'antd';

import type { EngineSource } from '../../types';
import type { SourceFormValues } from '../types';
import { sourceFormInitialValues } from '../utils';

const { Paragraph } = Typography;

type SourceModalProps = {
  open: boolean;
  editingSource: EngineSource | null;
  form: FormInstance<SourceFormValues>;
  submitting: boolean;
  onCancel: () => void;
  onSubmit: () => void;
};

export function SourceModal(props: SourceModalProps) {
  const { open, editingSource, form, submitting, onCancel, onSubmit } = props;

  return (
    <Modal
      open={open}
      title={editingSource ? '编辑接入系统' : '新增接入系统'}
      onCancel={onCancel}
      onOk={onSubmit}
      confirmLoading={submitting}
      okText={editingSource ? '保存' : '创建'}
      cancelText="取消"
      width={760}
      destroyOnClose
    >
      <Paragraph type="secondary">
        接入系统由平台统一管理。这里只配置系统标识和展示名称，其他内部字段由平台自动维护。
      </Paragraph>
      <Form form={form} layout="vertical" initialValues={sourceFormInitialValues(editingSource)}>
        <Row gutter={[12, 0]}>
          <Col xs={24} md={12}>
            <Form.Item
              label="租户标识"
              name="tenant_id"
              rules={[{ required: true, message: '请输入租户标识' }]}
            >
              <Input placeholder="例如：tenant_prod_cn" />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item
              label="系统标识"
              name="source_id"
              rules={[{ required: true, message: '请输入系统标识' }]}
            >
              <Input placeholder="例如：chatos" disabled={Boolean(editingSource)} />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item
              label="名称"
              name="name"
              rules={[{ required: true, message: '请输入系统名称' }]}
            >
              <Input placeholder="例如：ChatOS" />
            </Form.Item>
          </Col>
          <Col span={24}>
            <Form.Item label="描述" name="description">
              <Input placeholder="可选，用于说明该系统的接入用途" />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="启用" name="enabled" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
        </Row>
      </Form>
    </Modal>
  );
}
