// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { RobotOutlined } from '@ant-design/icons';
import {
  Alert,
  Button,
  Collapse,
  Empty,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Spin,
  Switch,
  Typography,
} from 'antd';
import type { FormInstance } from 'antd';
import type { Dispatch, SetStateAction } from 'react';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import { mcpDisplayName, runtimeKindLabel } from '../../i18n/labels';
import type { McpProviderSkill, McpRecord, McpToolDescriptor, RuntimeKind } from '../../types';
import { CatalogIdentityFields } from '../catalogForm/CatalogIdentityFields';
import { runtimeUsesCommand, runtimeUsesHttp, runtimeUsesLocalConnector } from './support';

type DescriptorData = Awaited<ReturnType<typeof api.getMcpDescriptor>>;
type AdminAiModels = Awaited<ReturnType<typeof api.listAdminAiModels>>;

interface DescriptorModalState {
  record: McpRecord;
  view: 'skills' | 'tools';
}

interface OptimizeTarget {
  record: McpRecord;
  skill: McpProviderSkill;
}

interface McpCatalogDialogsProps {
  form: FormInstance;
  optimizeForm: FormInstance;
  editing: McpRecord | null;
  editingSystemManaged: boolean;
  modalOpen: boolean;
  closeModal: () => void;
  onSave: (values: Record<string, unknown>) => void;
  savePending: boolean;
  isAdmin: boolean;
  runtimeKinds: RuntimeKind[];
  runtimeKind?: RuntimeKind;
  descriptorModal: DescriptorModalState | null;
  descriptorData?: DescriptorData;
  descriptorLoading: boolean;
  descriptorError: unknown;
  activeProviderSkillId: string | null;
  setActiveProviderSkillId: Dispatch<SetStateAction<string | null>>;
  onCloseDescriptor: () => void;
  onStartOptimize: () => void;
  optimizeTarget: OptimizeTarget | null;
  aiModels?: AdminAiModels;
  aiModelsLoading: boolean;
  optimizedInstructions: string;
  setOptimizedInstructions: Dispatch<SetStateAction<string>>;
  optimizationThinking: string;
  optimizeStreaming: boolean;
  onCloseOptimize: () => void;
  onStream: (values: { model_config_id: string; requirement: string }) => void | Promise<void>;
  onSaveOptimized: () => void;
  saveOptimizedPending: boolean;
}

export function McpCatalogDialogs({
  form,
  optimizeForm,
  editing,
  editingSystemManaged,
  modalOpen,
  closeModal,
  onSave,
  savePending,
  isAdmin,
  runtimeKinds,
  runtimeKind,
  descriptorModal,
  descriptorData,
  descriptorLoading,
  descriptorError,
  activeProviderSkillId,
  setActiveProviderSkillId,
  onCloseDescriptor,
  onStartOptimize,
  optimizeTarget,
  aiModels,
  aiModelsLoading,
  optimizedInstructions,
  setOptimizedInstructions,
  optimizationThinking,
  optimizeStreaming,
  onCloseOptimize,
  onStream,
  onSaveOptimized,
  saveOptimizedPending,
}: McpCatalogDialogsProps) {
  const { t } = useI18n();
  const saveMutation = { isPending: savePending, mutate: onSave };
  const descriptorQuery = {
    data: descriptorData,
    isLoading: descriptorLoading,
    error: descriptorError,
  };
  const aiModelsQuery = { data: aiModels, isLoading: aiModelsLoading };
  const saveOptimizedSkillMutation = {
    isPending: saveOptimizedPending,
    mutate: onSaveOptimized,
  };
  const streamProviderSkillOptimization = onStream;

  return (
    <>
      <Modal
        title={t(
          editingSystemManaged
            ? 'mcp.systemConfigTitle'
            : editing
              ? 'mcp.editTitle'
              : 'mcp.addTitle',
        )}
        open={modalOpen}
        onCancel={closeModal}
        onOk={() => form.submit()}
        confirmLoading={saveMutation.isPending}
        width={editingSystemManaged ? 480 : 760}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={(values) => saveMutation.mutate(values)}>
          {editingSystemManaged && editing ? (
            <>
              <Form.Item label={t('table.name')}>
                <Space direction="vertical" size={0}>
                  <Typography.Text strong>{mcpDisplayName(editing, t)}</Typography.Text>
                  <Typography.Text type="secondary">{editing.name}</Typography.Text>
                </Space>
              </Form.Item>
              <Form.Item name="enabled" label={t('field.enabled')} valuePropName="checked">
                <Switch />
              </Form.Item>
            </>
          ) : (
            <>
              <CatalogIdentityFields isAdmin={isAdmin} />
              <Form.Item name="runtime_kind" label={t('field.runtimeKind')} rules={[{ required: true }]}>
                <Select
                  options={runtimeKinds.map((value) => ({ value, label: runtimeKindLabel(value, t) }))}
                />
              </Form.Item>
              {runtimeUsesCommand(runtimeKind) ? (
                <div className="form-grid two">
                  <Form.Item name="command" label={t('field.command')} rules={[{ required: true }]}>
                    <Input />
                  </Form.Item>
                  <Form.Item name="cwd" label={t('field.cwd')}>
                    <Input />
                  </Form.Item>
                  <Form.Item name="args_json" label={t('field.argsJson')}>
                    <Input.TextArea rows={4} />
                  </Form.Item>
                  <Form.Item name="env_json" label={t('field.envJson')}>
                    <Input.TextArea rows={4} />
                  </Form.Item>
                </div>
              ) : null}
              {runtimeUsesHttp(runtimeKind) ? (
                <div className="form-grid two">
                  <Form.Item name="url" label={t('field.url')} rules={[{ required: true }]}>
                    <Input />
                  </Form.Item>
                  <Form.Item name="headers_json" label={t('field.headersJson')}>
                    <Input.TextArea rows={4} />
                  </Form.Item>
                </div>
              ) : null}
              {runtimeUsesLocalConnector(runtimeKind) ? (
                <Form.Item
                  name="local_connector_json"
                  label={t('field.localConnectorJson')}
                  rules={[{ required: true }]}
                >
                  <Input.TextArea rows={4} />
                </Form.Item>
              ) : null}
            </>
          )}
        </Form>
      </Modal>
      <Modal
        title={
          descriptorModal
            ? t(
                descriptorModal.view === 'skills'
                  ? 'mcp.providerSkillsTitle'
                  : 'mcp.toolCatalogTitle',
                { name: mcpDisplayName(descriptorModal.record, t) },
              )
            : ''
        }
        open={Boolean(descriptorModal)}
        onCancel={onCloseDescriptor}
        footer={
          isAdmin &&
          descriptorModal?.view === 'skills' &&
          descriptorQuery.data?.provider_skills.length ? (
            <Button
              type="primary"
              icon={<RobotOutlined />}
              onClick={onStartOptimize}
            >
              {t('mcp.aiOptimize')}
            </Button>
          ) : null
        }
        width={920}
        destroyOnClose
      >
        <Spin spinning={descriptorQuery.isLoading}>
          {descriptorQuery.error ? (
            <Alert
              type="error"
              showIcon
              message={t('mcp.descriptorLoadFailed')}
              description={(descriptorQuery.error as Error).message}
            />
          ) : null}
          {descriptorModal?.view === 'skills' && descriptorQuery.data ? (
            descriptorQuery.data.provider_skills.length ? (
              <Collapse
                defaultActiveKey={[descriptorQuery.data.provider_skills[0].id]}
                onChange={(keys) => {
                  const selected = Array.isArray(keys) ? keys[0] : keys;
                  setActiveProviderSkillId(selected ? String(selected) : null);
                }}
                items={descriptorQuery.data.provider_skills.map((skill) => ({
                  key: skill.id,
                  label: (
                    <Space direction="vertical" size={0}>
                      <Typography.Text strong>{skill.name}</Typography.Text>
                      {skill.description ? (
                        <Typography.Text type="secondary">{skill.description}</Typography.Text>
                      ) : null}
                    </Space>
                  ),
                  children: (
                    <Typography.Paragraph
                      style={{ whiteSpace: 'pre-wrap', maxHeight: 560, overflow: 'auto' }}
                    >
                      {skill.instructions}
                    </Typography.Paragraph>
                  ),
                }))}
              />
            ) : (
              <Empty description={t('mcp.noProviderSkills')} />
            )
          ) : null}
          {descriptorModal?.view === 'tools' && descriptorQuery.data ? (
            <Space direction="vertical" size="middle" style={{ width: '100%' }}>
              {descriptorQuery.data.tools_error ? (
                <Alert
                  type={descriptorQuery.data.tools.length ? 'warning' : 'error'}
                  showIcon
                  message={t(`mcp.toolsStatus.${descriptorQuery.data.tools_status}`)}
                  description={descriptorQuery.data.tools_error}
                />
              ) : null}
              {descriptorQuery.data.tools.length ? (
                <Collapse
                  items={descriptorQuery.data.tools.map((tool, index) =>
                    toolCollapseItem(tool, index, t),
                  )}
                />
              ) : (
                <Empty description={t('mcp.noToolsDeclared')} />
              )}
            </Space>
          ) : null}
        </Spin>
      </Modal>
      <Modal
        title={
          optimizeTarget
            ? `${t('mcp.aiOptimizeTitle')} · ${optimizeTarget.skill.name}`
            : t('mcp.aiOptimizeTitle')
        }
        open={Boolean(optimizeTarget)}
        onCancel={onCloseOptimize}
        footer={null}
        width={1120}
        style={{ top: 24 }}
        styles={{ body: { maxHeight: 'calc(100vh - 120px)', overflowY: 'auto' } }}
        destroyOnClose
      >
        <Form
          form={optimizeForm}
          layout="vertical"
          onFinish={(values) => void streamProviderSkillOptimization(values)}
        >
          <Form.Item
            name="requirement"
            label={t('mcp.aiRequirement')}
            rules={[{ required: true }]}
          >
            <Input.TextArea rows={4} placeholder={t('mcp.aiRequirementPlaceholder')} />
          </Form.Item>
          <Space align="start" wrap style={{ width: '100%', justifyContent: 'flex-end' }}>
            <Form.Item
              name="model_config_id"
              rules={[{ required: true }]}
              style={{ width: 700, maxWidth: '70vw', marginBottom: 0 }}
            >
              <Select
                showSearch
                loading={aiModelsQuery.isLoading}
                placeholder={t('mcp.aiModelPlaceholder')}
                optionFilterProp="label"
                notFoundContent={t('mcp.noAiModels')}
                options={(aiModelsQuery.data || []).map((model) => ({
                  value: model.id,
                  label: `${model.name} · ${model.model || model.model_name} (${model.provider})`,
                }))}
              />
            </Form.Item>
            <Button type="primary" htmlType="submit" loading={optimizeStreaming}>
              {t('mcp.aiSend')}
            </Button>
          </Space>
        </Form>
        {optimizedInstructions || optimizeStreaming ? (
          <Space direction="vertical" size="middle" style={{ width: '100%', marginTop: 24 }}>
            <Space>
              <Typography.Text strong>{t('mcp.aiResult')}</Typography.Text>
              {optimizeStreaming ? (
                <Typography.Text type="secondary">{t('mcp.aiStreaming')}</Typography.Text>
              ) : null}
            </Space>
            {optimizationThinking && optimizeStreaming ? (
              <Typography.Paragraph
                type="secondary"
                style={{ maxHeight: 80, overflow: 'auto', whiteSpace: 'pre-wrap', marginBottom: 0 }}
              >
                {t('mcp.aiThinking')}: {optimizationThinking}
              </Typography.Paragraph>
            ) : null}
            <Input.TextArea
              rows={24}
              value={optimizedInstructions}
              onChange={(event) => setOptimizedInstructions(event.target.value)}
            />
            <Space style={{ width: '100%', justifyContent: 'flex-end' }}>
              <Button
                type="primary"
                loading={saveOptimizedSkillMutation.isPending}
                disabled={optimizeStreaming || !optimizedInstructions.trim()}
                onClick={() => saveOptimizedSkillMutation.mutate()}
              >
                {t('mcp.aiSave')}
              </Button>
            </Space>
          </Space>
        ) : null}
      </Modal>
    </>
  );
}

function toolCollapseItem(
  tool: McpToolDescriptor,
  index: number,
  t: (key: string) => string,
) {
  const name = typeof tool.name === 'string' && tool.name.trim() ? tool.name : `tool_${index + 1}`;
  const description = typeof tool.description === 'string' ? tool.description : '';
  const inputSchema = tool.inputSchema ?? tool.input_schema;
  const outputSchema = tool.outputSchema ?? tool.output_schema;
  return {
    key: `${name}-${index}`,
    label: (
      <Space direction="vertical" size={0}>
        <Typography.Text strong code>
          {name}
        </Typography.Text>
        {description ? <Typography.Text type="secondary">{description}</Typography.Text> : null}
      </Space>
    ),
    children: (
      <div className="form-grid two">
        <SchemaPanel
          title={t('mcp.inputSchema')}
          schema={inputSchema}
          notDeclared={t('mcp.schemaNotDeclared')}
        />
        <SchemaPanel
          title={t('mcp.outputSchema')}
          schema={outputSchema}
          notDeclared={t('mcp.schemaNotDeclared')}
        />
      </div>
    ),
  };
}

function SchemaPanel({
  title,
  schema,
  notDeclared,
}: {
  title: string;
  schema: unknown;
  notDeclared: string;
}) {
  return (
    <div>
      <Typography.Text strong>{title}</Typography.Text>
      <pre
        style={{
          marginTop: 8,
          padding: 12,
          maxHeight: 420,
          overflow: 'auto',
          borderRadius: 6,
          background: 'rgba(127, 127, 127, 0.08)',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-word',
        }}
      >
        {schema === undefined ? notDeclared : JSON.stringify(schema, null, 2)}
      </pre>
    </div>
  );
}
