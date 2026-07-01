// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState } from 'react';
import {
  Button,
  Col,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  List,
  Modal,
  Row,
  Select,
  Space,
  Tag,
  Typography,
} from 'antd';
import type { FormInstance } from 'antd/es/form';

import { api } from '../../api/client';
import { MarkdownPreview } from '../../components/MarkdownPreview';
import type {
  CreateRequirementPayload,
  ProjectWorkItemRecord,
  RequirementDocumentRecord,
  RequirementRecord,
} from '../../types';
import {
  requirementDocumentTypeLabel,
  requirementStatusOptions,
  requirementTypeOptions,
  workItemStatusOptions,
} from './options';
import { RequirementDetailPreview, WorkItemDetailPreview } from './renderers';
import {
  technicalDocumentsLayoutStyle,
  technicalDocumentsListBodyStyle,
  technicalDocumentsListHeaderStyle,
  technicalDocumentsListPaneStyle,
  technicalOverviewModalBodyStyle,
  technicalOverviewPreviewBodyStyle,
  technicalOverviewPreviewHeaderStyle,
  technicalOverviewPreviewPaneStyle,
} from './styles';
import type { ExecutionOptionLabelMap, WorkItemFormValues } from './types';

interface SelectOption {
  value: string;
  label: string;
}

const emptyRequirementDocuments: RequirementDocumentRecord[] = [];

interface ProjectDetailOverlaysProps {
  requirementModalOpen: boolean;
  onCloseRequirementModal: () => void;
  requirementForm: FormInstance<CreateRequirementPayload>;
  createRequirementPending: boolean;
  onCreateRequirement: (values: CreateRequirementPayload) => void;
  requirementDepTarget: RequirementRecord | null;
  onCloseRequirementDeps: () => void;
  onSaveRequirementDeps: () => void;
  saveRequirementDepsPending: boolean;
  requirementDepsLoading: boolean;
  requirementDepIds: string[];
  onRequirementDepIdsChange: (ids: string[]) => void;
  selectableRequirements: RequirementRecord[];
  docTarget: RequirementRecord | null;
  onCloseDoc: () => void;
  docLoading: boolean;
  docDocuments?: RequirementDocumentRecord[];
  workItemModalOpen: boolean;
  onCloseWorkItemModal: () => void;
  workItemForm: FormInstance<WorkItemFormValues>;
  createWorkItemPending: boolean;
  onCreateWorkItem: (values: WorkItemFormValues) => void;
  taskRunnerModelOptions: SelectOption[];
  taskRunnerToolOptions: SelectOption[];
  taskRunnerSkillOptions: SelectOption[];
  executionOptionsLoading: boolean;
  executionOptionsErrorMessage?: string;
  workItemDepTarget: ProjectWorkItemRecord | null;
  onCloseWorkItemDeps: () => void;
  onSaveWorkItemDeps: () => void;
  saveWorkItemDepsPending: boolean;
  workItemDepsLoading: boolean;
  workItemDepIds: string[];
  onWorkItemDepIdsChange: (ids: string[]) => void;
  selectableWorkItems: ProjectWorkItemRecord[];
  requirementDetailTarget: RequirementRecord | null;
  onCloseRequirementDetail: () => void;
  workItemDetailTarget: ProjectWorkItemRecord | null;
  onCloseWorkItemDetail: () => void;
  taskRunnerModelLabelMap: ExecutionOptionLabelMap;
  taskRunnerToolLabelMap: ExecutionOptionLabelMap;
  taskRunnerSkillLabelMap: ExecutionOptionLabelMap;
  requirements: RequirementRecord[];
}

export function ProjectDetailOverlays({
  requirementModalOpen,
  onCloseRequirementModal,
  requirementForm,
  createRequirementPending,
  onCreateRequirement,
  requirementDepTarget,
  onCloseRequirementDeps,
  onSaveRequirementDeps,
  saveRequirementDepsPending,
  requirementDepsLoading,
  requirementDepIds,
  onRequirementDepIdsChange,
  selectableRequirements,
  docTarget,
  onCloseDoc,
  docLoading,
  docDocuments,
  workItemModalOpen,
  onCloseWorkItemModal,
  workItemForm,
  createWorkItemPending,
  onCreateWorkItem,
  taskRunnerModelOptions,
  taskRunnerToolOptions,
  taskRunnerSkillOptions,
  executionOptionsLoading,
  executionOptionsErrorMessage,
  workItemDepTarget,
  onCloseWorkItemDeps,
  onSaveWorkItemDeps,
  saveWorkItemDepsPending,
  workItemDepsLoading,
  workItemDepIds,
  onWorkItemDepIdsChange,
  selectableWorkItems,
  requirementDetailTarget,
  onCloseRequirementDetail,
  workItemDetailTarget,
  onCloseWorkItemDetail,
  taskRunnerModelLabelMap,
  taskRunnerToolLabelMap,
  taskRunnerSkillLabelMap,
  requirements,
}: ProjectDetailOverlaysProps) {
  const [selectedDocumentId, setSelectedDocumentId] = useState<string | undefined>();
  const documents = docDocuments || emptyRequirementDocuments;
  const selectedDocument =
    documents.find((item) => item.id === selectedDocumentId) || documents[0];

  useEffect(() => {
    if (!docTarget) {
      setSelectedDocumentId(undefined);
      return;
    }
    setSelectedDocumentId((current) => {
      if (current && documents.some((item) => item.id === current)) {
        return current;
      }
      return documents[0]?.id;
    });
  }, [docTarget, documents]);

  return (
    <>
      <Modal
        title="新建需求"
        open={requirementModalOpen}
        onCancel={onCloseRequirementModal}
        onOk={() => requirementForm.submit()}
        confirmLoading={createRequirementPending}
        destroyOnClose
      >
        <Form<CreateRequirementPayload>
          form={requirementForm}
          layout="vertical"
          initialValues={{ requirement_type: 'requirement', status: 'draft', priority: 0 }}
          onFinish={onCreateRequirement}
        >
          <Form.Item name="title" label="标题" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="requirement_type" label="类型">
            <Select options={requirementTypeOptions} />
          </Form.Item>
          <Form.Item name="summary" label="摘要">
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item name="detail" label="详情">
            <Input.TextArea rows={5} />
          </Form.Item>
          <Form.Item name="acceptance_criteria" label="验收标准">
            <Input.TextArea rows={4} />
          </Form.Item>
          <Form.Item name="status" label="状态">
            <Select options={requirementStatusOptions} />
          </Form.Item>
          <Form.Item name="priority" label="优先级">
            <InputNumber style={{ width: '100%' }} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="设置需求前置关系"
        open={Boolean(requirementDepTarget)}
        onCancel={onCloseRequirementDeps}
        onOk={onSaveRequirementDeps}
        confirmLoading={saveRequirementDepsPending}
        destroyOnClose
      >
        <Typography.Paragraph>{requirementDepTarget?.title}</Typography.Paragraph>
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          loading={requirementDepsLoading}
          value={requirementDepIds}
          onChange={onRequirementDepIdsChange}
          options={selectableRequirements
            .filter((item) => item.id !== requirementDepTarget?.id)
            .map((item) => ({ value: item.id, label: item.title }))}
        />
      </Modal>

      <Modal
        title="需求技术文档"
        open={Boolean(docTarget)}
        onCancel={onCloseDoc}
        width="min(1280px, calc(100vw - 48px))"
        style={{ top: 28 }}
        styles={{ body: technicalOverviewModalBodyStyle }}
        footer={<Button onClick={onCloseDoc}>关闭</Button>}
        destroyOnClose
      >
        <div style={technicalDocumentsLayoutStyle}>
          <section style={technicalDocumentsListPaneStyle}>
            <div style={technicalDocumentsListHeaderStyle}>
              <Typography.Text strong>文档列表</Typography.Text>
            </div>
            <div style={technicalDocumentsListBodyStyle}>
              <List
                loading={docLoading}
                dataSource={documents}
                locale={{ emptyText: '暂无技术文档' }}
                renderItem={(item) => {
                  const selected = item.id === selectedDocument?.id;
                  return (
                    <List.Item
                      key={item.id}
                      onClick={() => setSelectedDocumentId(item.id)}
                      style={{
                        cursor: 'pointer',
                        padding: '12px 16px',
                        background: selected ? '#eef4ff' : undefined,
                      }}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Typography.Text strong ellipsis>
                          {item.title || '技术文档'}
                        </Typography.Text>
                        <Space size={6} wrap>
                          <Tag color={selected ? 'blue' : 'default'}>
                            {requirementDocumentTypeLabel(item.doc_type)}
                          </Tag>
                          <Typography.Text type="secondary">
                            v{item.version}
                          </Typography.Text>
                        </Space>
                      </Space>
                    </List.Item>
                  );
                }}
              />
            </div>
          </section>
          <section style={technicalOverviewPreviewPaneStyle}>
            {selectedDocument ? (
              <>
                <div style={technicalOverviewPreviewHeaderStyle}>
                  <Typography.Text strong>{selectedDocument.title || '技术文档'}</Typography.Text>
                  <Space size={6} wrap>
                    <Tag>{requirementDocumentTypeLabel(selectedDocument.doc_type)}</Tag>
                    <Tag color="blue">{selectedDocument.format || 'markdown'}</Tag>
                  </Space>
                </div>
                <div style={technicalOverviewPreviewBodyStyle}>
                  <MarkdownPreview value={docLoading ? '加载中...' : selectedDocument.content} />
                </div>
              </>
            ) : (
              <Empty
                style={{ paddingTop: 120 }}
                description={docLoading ? '加载中...' : '暂无技术文档'}
              />
            )}
          </section>
        </div>
      </Modal>

      <Modal
        title="新建项目任务"
        open={workItemModalOpen}
        onCancel={onCloseWorkItemModal}
        onOk={() => workItemForm.submit()}
        confirmLoading={createWorkItemPending}
        destroyOnClose
      >
        <Form<WorkItemFormValues>
          form={workItemForm}
          layout="vertical"
          initialValues={{
            status: 'todo',
            priority: 0,
            sort_order: 0,
            task_runner_enabled_tool_ids: [],
            task_runner_skill_ids: [],
          }}
          onFinish={onCreateWorkItem}
        >
          <Form.Item
            name="requirement_id"
            label="所属需求"
            rules={[
              { required: true },
              {
                validator: async (_, value?: string) => {
                  if (!value) {
                    return;
                  }
                  const docs = await api.listRequirementDocuments(value);
                  if (!docs.some((doc) => doc.content?.trim())) {
                    throw new Error('创建项目任务前，请先填写该需求的技术文档内容');
                  }
                },
              },
            ]}
          >
            <Select options={selectableRequirements.map((item) => ({ value: item.id, label: item.title }))} />
          </Form.Item>
          <Form.Item name="title" label="标题" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={4} />
          </Form.Item>
          {executionOptionsErrorMessage ? (
            <Typography.Text type="danger">{executionOptionsErrorMessage}</Typography.Text>
          ) : null}
          <Form.Item
            name="task_runner_default_model_config_id"
            label="执行模型"
            rules={[{ required: true, message: '请选择执行模型' }]}
          >
            <Select
              showSearch
              loading={executionOptionsLoading}
              options={taskRunnerModelOptions}
              placeholder="选择 Task Runner 模型配置"
            />
          </Form.Item>
          <Form.Item
            name="task_runner_enabled_tool_ids"
            label="工具集"
            rules={[
              {
                validator: async (_, value?: string[]) => {
                  if (value?.length) {
                    return;
                  }
                  throw new Error('请选择工具集');
                },
              },
            ]}
          >
            <Select
              mode="multiple"
              showSearch
              loading={executionOptionsLoading}
              options={taskRunnerToolOptions}
              placeholder="选择可用工具"
            />
          </Form.Item>
          <Form.Item name="task_runner_skill_ids" label="Skills">
            <Select
              mode="multiple"
              showSearch
              loading={executionOptionsLoading}
              options={taskRunnerSkillOptions}
              placeholder="选择执行时加载的 Skill"
            />
          </Form.Item>
          <Form.Item name="status" label="状态">
            <Select options={workItemStatusOptions} />
          </Form.Item>
          <Row gutter={12}>
            <Col span={12}>
              <Form.Item name="priority" label="优先级">
                <InputNumber style={{ width: '100%' }} />
              </Form.Item>
            </Col>
            <Col span={12}>
              <Form.Item name="estimate_points" label="估算点数">
                <InputNumber style={{ width: '100%' }} min={0} />
              </Form.Item>
            </Col>
          </Row>
          <Form.Item name="tags_text" label="标签">
            <Input placeholder="frontend,api" />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="设置项目任务前置关系"
        open={Boolean(workItemDepTarget)}
        onCancel={onCloseWorkItemDeps}
        onOk={onSaveWorkItemDeps}
        confirmLoading={saveWorkItemDepsPending}
        destroyOnClose
      >
        <Typography.Paragraph>{workItemDepTarget?.title}</Typography.Paragraph>
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          loading={workItemDepsLoading}
          value={workItemDepIds}
          onChange={onWorkItemDepIdsChange}
          options={selectableWorkItems
            .filter((item) => item.id !== workItemDepTarget?.id)
            .map((item) => ({ value: item.id, label: item.title }))}
        />
      </Modal>

      <Drawer
        title="需求详情"
        open={Boolean(requirementDetailTarget)}
        onClose={onCloseRequirementDetail}
        width="min(1120px, calc(100vw - 48px))"
        styles={{ body: { padding: 0, background: '#f6f7f9' } }}
        destroyOnClose
      >
        {requirementDetailTarget ? <RequirementDetailPreview requirement={requirementDetailTarget} /> : null}
      </Drawer>

      <Drawer
        title="项目任务详情"
        open={Boolean(workItemDetailTarget)}
        onClose={onCloseWorkItemDetail}
        width="min(1120px, calc(100vw - 48px))"
        styles={{ body: { padding: 0, background: '#f6f7f9' } }}
        destroyOnClose
      >
        {workItemDetailTarget ? (
          <WorkItemDetailPreview
            workItem={workItemDetailTarget}
            modelLabelMap={taskRunnerModelLabelMap}
            toolLabelMap={taskRunnerToolLabelMap}
            skillLabelMap={taskRunnerSkillLabelMap}
            requirementTitle={
              requirements.find((item) => item.id === workItemDetailTarget.requirement_id)?.title ||
              workItemDetailTarget.requirement_id
            }
          />
        ) : null}
      </Drawer>
    </>
  );
}
