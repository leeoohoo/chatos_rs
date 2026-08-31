// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Col, Form, Input, InputNumber, Modal, Row, Select, Switch, Typography } from 'antd';
import type { FormInstance } from 'antd';

import type { EngineModelProfile } from '../../types';
import { THINKING_LEVEL_OPTIONS } from '../constants';
import type { ModelFormValues } from '../types';
import { modelFormInitialValues } from '../utils';

const { Paragraph } = Typography;

type ModelModalProps = {
  open: boolean;
  editingModel: EngineModelProfile | null;
  form: FormInstance<ModelFormValues>;
  submitting: boolean;
  onCancel: () => void;
  onSubmit: () => void;
};

export function ModelModal(props: ModelModalProps) {
  const { open, editingModel, form, submitting, onCancel, onSubmit } = props;

  return (
    <Modal
      open={open}
      title={editingModel ? '编辑模型配置' : '新建模型配置'}
      onCancel={onCancel}
      onOk={onSubmit}
      confirmLoading={submitting}
      okText={editingModel ? '保存' : '创建'}
      cancelText="取消"
      width={760}
      destroyOnClose
    >
      <Paragraph type="secondary">
        模型配置由 memory_engine 统一管理。编辑时如果不改 API Key，留空即可保留原值。
      </Paragraph>
      <Form form={form} layout="vertical" initialValues={modelFormInitialValues(editingModel)}>
        <Row gutter={[12, 0]}>
          <Col xs={24} md={12}>
            <Form.Item
              label="名称"
              name="name"
              rules={[{ required: true, message: '请输入配置名称' }]}
            >
              <Input placeholder="例如：summary-default" />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item
              label="Provider"
              name="provider"
              rules={[{ required: true, message: '请输入 provider' }]}
            >
              <Input placeholder="例如：openai" />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item
              label="Model"
              name="model"
              rules={[{ required: true, message: '请输入 model' }]}
            >
              <Input placeholder="例如：gpt-4.1" />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="Base URL" name="base_url">
              <Input placeholder="可选" />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="API Key" name="api_key">
              <Input.Password placeholder={editingModel ? '留空表示保持不变' : '可选'} />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="Thinking Level" name="thinking_level">
              <Select
                allowClear
                options={THINKING_LEVEL_OPTIONS}
                placeholder="按模型能力选择"
              />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="Temperature" name="temperature">
              <InputNumber min={0} max={2} step={0.1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="启用" name="enabled" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
          <Col xs={24} md={12}>
            <Form.Item label="设为默认模型" name="is_default" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="支持图片" name="supports_images" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="支持推理" name="supports_reasoning" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="支持 Responses" name="supports_responses" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
        </Row>
      </Form>
    </Modal>
  );
}
