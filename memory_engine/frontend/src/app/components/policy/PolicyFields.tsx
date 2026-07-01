// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useState } from 'react';

import { BulbOutlined, TranslationOutlined } from '@ant-design/icons';
import {
  Alert,
  Button,
  Card,
  Col,
  Form,
  Input,
  InputNumber,
  Radio,
  Row,
  Select,
  Space,
  Switch,
  Typography,
} from 'antd';

import type { PolicyFieldsProps } from './types';

const { TextArea } = Input;
const { Text } = Typography;

export function PolicyFields(props: PolicyFieldsProps) {
  const {
    form,
    initialValues,
    meta,
    modelOptions,
    promptFieldName,
    promptLanguageFieldName,
    promptZhFieldName,
    promptEnFieldName,
    generatingPrompt,
    onGeneratePrompt,
  } = props;
  const [promptRequest, setPromptRequest] = useState('');

  const handleGeneratePrompt = async () => {
    await onGeneratePrompt(promptRequest);
  };

  return (
    <Form form={form} layout="vertical" initialValues={initialValues}>
      <Row gutter={[12, 0]}>
        <Col xs={24} md={8}>
          <Form.Item label="启用" name="enabled" valuePropName="checked">
            <Switch />
          </Form.Item>
        </Col>
        <Col xs={24} md={16}>
          <Form.Item label="模型配置" name="model_profile_id">
            <Select
              allowClear
              showSearch
              optionFilterProp="label"
              placeholder="留空表示使用全局默认模型"
              options={modelOptions}
            />
          </Form.Item>
        </Col>

        <Col span={24}>
          <Card
            size="small"
            title={
              <Space size={8}>
                <TranslationOutlined />
                <span>{meta.promptLabel}</span>
              </Space>
            }
          >
            <Space direction="vertical" size={16} style={{ width: '100%' }}>
              <Alert
                type="info"
                showIcon
                message="可以先描述你想要的总结风格、保留重点和输出倾向，AI 会一次生成中文和英文两版 prompt。"
              />

              <Form.Item label="Prompt 生成需求">
                <TextArea
                  rows={4}
                  value={promptRequest}
                  onChange={(event) => setPromptRequest(event.target.value)}
                  placeholder="例如：基于用户输入内容自动生成高密度总结，重点保留任务进展、阻塞点、下一步计划、关键文件路径，并分别提供中文和英文版本。"
                />
              </Form.Item>

              <Space wrap>
                <Button
                  type="default"
                  icon={<BulbOutlined />}
                  loading={generatingPrompt}
                  disabled={!promptRequest.trim()}
                  onClick={() => void handleGeneratePrompt()}
                >
                  AI 生成中英 Prompt
                </Button>
                <Text type="secondary">
                  生成后仍可手动调整，保存时会保留中英双版本。
                </Text>
              </Space>

              <Form.Item label="总结时使用哪种提示词" name={promptLanguageFieldName}>
                <Radio.Group
                  optionType="button"
                  buttonStyle="solid"
                  options={[
                    { label: '中文 Prompt', value: 'zh' },
                    { label: 'English Prompt', value: 'en' },
                  ]}
                />
              </Form.Item>

              <Row gutter={[12, 0]}>
                <Col xs={24} lg={12}>
                  <Form.Item
                    label="中文 Prompt"
                    name={promptZhFieldName}
                    extra={meta.promptPlaceholder ?? '为空时使用默认总结模板'}
                  >
                    <TextArea rows={10} placeholder="输入或生成中文总结提示词" />
                  </Form.Item>
                </Col>
                <Col xs={24} lg={12}>
                  <Form.Item
                    label="English Prompt"
                    name={promptEnFieldName}
                    extra="可填写与中文等价的英文提示词，便于切换使用。"
                  >
                    <TextArea rows={10} placeholder="Write or generate the English prompt" />
                  </Form.Item>
                </Col>
              </Row>

              <Form.Item name={promptFieldName} hidden>
                <Input />
              </Form.Item>
            </Space>
          </Card>
        </Col>

        <Col xs={24} md={8}>
          <Form.Item label={meta.tokenLimitLabel} name="token_limit">
            <InputNumber min={128} style={{ width: '100%' }} />
          </Form.Item>
        </Col>
        {meta.showTargetSummaryTokens === false ? null : (
          <Col xs={24} md={8}>
            <Form.Item label={meta.targetSummaryTokensLabel} name="target_summary_tokens">
              <InputNumber min={128} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        )}
        <Col xs={24} md={8}>
          <Form.Item label={meta.intervalSecondsLabel} name="interval_seconds">
            <InputNumber min={3} style={{ width: '100%' }} />
          </Form.Item>
        </Col>
        {meta.showMaxThreadsPerTick === false || !meta.maxThreadsPerTickLabel ? null : (
          <Col xs={24} md={8}>
            <Form.Item label={meta.maxThreadsPerTickLabel} name="max_threads_per_tick">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        )}
        {meta.countLimitLabel ? (
          <Col xs={24} md={8}>
            <Form.Item label={meta.countLimitLabel} name="count_limit">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        ) : null}
        {meta.showKeepLevel0 ? (
          <Col xs={24} md={8}>
            <Form.Item label={meta.keepLevel0Label} name="keep_level0_count">
              <InputNumber min={0} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        ) : null}
        {meta.showMaxLevel ? (
          <Col xs={24} md={8}>
            <Form.Item label={meta.maxLevelLabel} name="max_level">
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
        ) : null}
      </Row>
    </Form>
  );
}
