// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { CaretDownOutlined, CaretRightOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Col, Descriptions, Form, Row, Space, Statistic, Switch, Table, Tabs, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { FormInstance } from 'antd/es/form';
import type { MouseEvent } from 'react';
import { Link } from 'react-router-dom';

import type {
  DependencyGraphNode,
  ProjectRecord,
  ProjectRuntimeEnvironmentDeploymentResponse,
  ProjectRuntimeEnvironmentResponse,
  UpdateProjectRuntimeEnvironmentVariablesPayload,
  ProjectWorkItemRecord,
  RequirementRecord,
  UpsertProjectProfilePayload,
} from '../../types';
import { ProfileMarkdownField, graphStatusTag, projectStatusTag, renderGraphNode } from './renderers';
import { RuntimeEnvironmentPanel } from './RuntimeEnvironmentPanel';
import { profileFormStyle, profileToolbarStyle } from './styles';
import type { GraphRelationRow, ProfileMarkdownFieldName, RequirementTableRecord } from './types';

interface ProjectDetailTabsProps {
  projectId: string;
  project?: ProjectRecord;
  showArchived: boolean;
  onShowArchivedChange: (value: boolean) => void;
  onRefresh: () => void;
  profileForm: FormInstance<UpsertProjectProfilePayload>;
  profileBackground?: string;
  profileIntroduction?: string;
  editingProfileField: ProfileMarkdownFieldName | null;
  profileSaving: boolean;
  onEditProfileField: (field: ProfileMarkdownFieldName) => void;
  onCancelProfileField: (field: ProfileMarkdownFieldName) => void;
  onSaveProfile: (values: UpsertProjectProfilePayload) => void;
  requirements: RequirementRecord[];
  workItems: ProjectWorkItemRecord[];
  selectableRequirementCount: number;
  requirementTree: RequirementTableRecord[];
  requirementColumns: ColumnsType<RequirementTableRecord>;
  workItemColumns: ColumnsType<ProjectWorkItemRecord>;
  requirementsLoading: boolean;
  workItemsLoading: boolean;
  onOpenRequirementModal: () => void;
  onOpenWorkItemModal: () => void;
  graphNodes: DependencyGraphNode[];
  graphLoading: boolean;
  blockingRelations: GraphRelationRow[];
  containsRelations: GraphRelationRow[];
  runtimeEnvironment?: ProjectRuntimeEnvironmentResponse;
  runtimeEnvironmentDeployment?: ProjectRuntimeEnvironmentDeploymentResponse;
  runtimeEnvironmentLoading: boolean;
  runtimeEnvironmentDeploymentLoading: boolean;
  runtimeEnvironmentErrorMessage?: string;
  runtimeEnvironmentAnalyzing: boolean;
  runtimeEnvironmentSettingsSaving: boolean;
  runtimeEnvironmentVariablesSaving: boolean;
  runtimeEnvironmentStarting: boolean;
  runtimeEnvironmentStopping: boolean;
  runtimeEnvironmentRestarting: boolean;
  onRefreshRuntimeEnvironment: () => void;
  onAnalyzeRuntimeEnvironment: () => void;
  onRuntimeSandboxEnabledChange: (value: boolean) => void;
  onSaveRuntimeEnvironmentVariables: (
    payload: UpdateProjectRuntimeEnvironmentVariablesPayload,
  ) => Promise<void>;
  onStartRuntimeEnvironment: () => void;
  onRefreshRuntimeEnvironmentDeployment: () => void;
  onStopRuntimeEnvironment: () => void;
  onRestartRuntimeEnvironment: () => void;
}

const renderRequirementExpandIcon = ({
  expanded,
  onExpand,
  record,
}: {
  expanded: boolean;
  onExpand: (record: RequirementTableRecord, event: MouseEvent<HTMLElement>) => void;
  record: RequirementTableRecord;
}) => {
  if (!record.children?.length) {
    return <span className="requirement-tree-expander requirement-tree-expander-placeholder" />;
  }
  return (
    <button
      type="button"
      aria-label={expanded ? '收起子需求' : '展开子需求'}
      className="requirement-tree-expander"
      onClick={(event) => {
        event.stopPropagation();
        onExpand(record, event);
      }}
    >
      {expanded ? <CaretDownOutlined /> : <CaretRightOutlined />}
    </button>
  );
};

export function ProjectDetailTabs({
  projectId,
  project,
  showArchived,
  onShowArchivedChange,
  onRefresh,
  profileForm,
  profileBackground,
  profileIntroduction,
  editingProfileField,
  profileSaving,
  onEditProfileField,
  onCancelProfileField,
  onSaveProfile,
  requirements,
  workItems,
  selectableRequirementCount,
  requirementTree,
  requirementColumns,
  workItemColumns,
  requirementsLoading,
  workItemsLoading,
  onOpenRequirementModal,
  onOpenWorkItemModal,
  graphNodes,
  graphLoading,
  blockingRelations,
  containsRelations,
  runtimeEnvironment,
  runtimeEnvironmentDeployment,
  runtimeEnvironmentLoading,
  runtimeEnvironmentDeploymentLoading,
  runtimeEnvironmentErrorMessage,
  runtimeEnvironmentAnalyzing,
  runtimeEnvironmentSettingsSaving,
  runtimeEnvironmentVariablesSaving,
  runtimeEnvironmentStarting,
  runtimeEnvironmentStopping,
  runtimeEnvironmentRestarting,
  onRefreshRuntimeEnvironment,
  onAnalyzeRuntimeEnvironment,
  onRuntimeSandboxEnabledChange,
  onSaveRuntimeEnvironmentVariables,
  onStartRuntimeEnvironment,
  onRefreshRuntimeEnvironmentDeployment,
  onStopRuntimeEnvironment,
  onRestartRuntimeEnvironment,
}: ProjectDetailTabsProps) {
  return (
    <>
      <div className="page-header">
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>
            {project?.name || '项目详情'}
          </Typography.Title>
          <Typography.Text type="secondary">
            <Link to="/projects">项目</Link>
            <span> / {projectId}</span>
          </Typography.Text>
        </div>
        <Space>
          <Space size={8}>
            <Typography.Text type="secondary">显示已归档</Typography.Text>
            <Switch size="small" checked={showArchived} onChange={onShowArchivedChange} />
          </Space>
          <Button icon={<ReloadOutlined />} onClick={onRefresh}>
            刷新
          </Button>
        </Space>
      </div>

      <Tabs
        items={[
          {
            key: 'overview',
            label: '概览',
            children: (
              <Space direction="vertical" size="large" style={{ width: '100%' }}>
                <Descriptions bordered column={2} size="small">
                  <Descriptions.Item label="项目名">{project?.name || '-'}</Descriptions.Item>
                  <Descriptions.Item label="状态">
                    {project ? projectStatusTag(project.status) : '-'}
                  </Descriptions.Item>
                  <Descriptions.Item label="项目来源">
                    {projectSourceTag(project?.source_type)}
                  </Descriptions.Item>
                  <Descriptions.Item label="云端导入">
                    {projectImportStatusTag(project?.import_status)}
                  </Descriptions.Item>
                  <Descriptions.Item label="根目录">{project?.root_path || '-'}</Descriptions.Item>
                  <Descriptions.Item label="Git">{project?.git_url || '-'}</Descriptions.Item>
                  <Descriptions.Item label="短描述" span={2}>
                    {project?.description || '-'}
                  </Descriptions.Item>
                </Descriptions>
                <Row gutter={16}>
                  <Col xs={24} md={6}>
                    <Card>
                      <Statistic title="需求数" value={requirements.length} />
                    </Card>
                  </Col>
                  <Col xs={24} md={6}>
                    <Card>
                      <Statistic title="项目任务数" value={workItems.length} />
                    </Card>
                  </Col>
                  <Col xs={24} md={6}>
                    <Card>
                      <Statistic
                        title="阻塞任务"
                        value={workItems.filter((item) => item.status === 'blocked').length}
                      />
                    </Card>
                  </Col>
                  <Col xs={24} md={6}>
                    <Card>
                      <Statistic
                        title="失败任务"
                        value={workItems.filter((item) => item.status === 'failed').length}
                      />
                    </Card>
                  </Col>
                </Row>
              </Space>
            ),
          },
          {
            key: 'runtime-environment',
            label: '运行环境',
            children: (
              <RuntimeEnvironmentPanel
                response={runtimeEnvironment}
                deployment={runtimeEnvironmentDeployment}
                loading={runtimeEnvironmentLoading}
                deploymentLoading={runtimeEnvironmentDeploymentLoading}
                errorMessage={runtimeEnvironmentErrorMessage}
                analyzing={runtimeEnvironmentAnalyzing}
                settingsSaving={runtimeEnvironmentSettingsSaving}
                variablesSaving={runtimeEnvironmentVariablesSaving}
                environmentStarting={runtimeEnvironmentStarting}
                environmentStopping={runtimeEnvironmentStopping}
                environmentRestarting={runtimeEnvironmentRestarting}
                onRefresh={onRefreshRuntimeEnvironment}
                onAnalyze={onAnalyzeRuntimeEnvironment}
                onSandboxEnabledChange={onRuntimeSandboxEnabledChange}
                onSaveEnvironmentVariables={onSaveRuntimeEnvironmentVariables}
                onStartEnvironment={onStartRuntimeEnvironment}
                onRefreshDeployment={onRefreshRuntimeEnvironmentDeployment}
                onStopEnvironment={onStopRuntimeEnvironment}
                onRestartEnvironment={onRestartRuntimeEnvironment}
              />
            ),
          },
          {
            key: 'profile',
            label: '项目详情',
            children: (
              <Form<UpsertProjectProfilePayload>
                form={profileForm}
                layout="vertical"
                onFinish={onSaveProfile}
                style={profileFormStyle}
              >
                <div style={profileToolbarStyle}>
                  <Typography.Title level={4} style={{ margin: 0 }}>
                    项目文档
                  </Typography.Title>
                </div>
                <ProfileMarkdownField
                  title="项目背景"
                  name="background"
                  value={profileBackground}
                  editing={editingProfileField === 'background'}
                  saving={profileSaving}
                  onEdit={() => onEditProfileField('background')}
                  onCancel={() => onCancelProfileField('background')}
                />
                <ProfileMarkdownField
                  title="项目介绍"
                  name="introduction"
                  value={profileIntroduction}
                  editing={editingProfileField === 'introduction'}
                  saving={profileSaving}
                  onEdit={() => onEditProfileField('introduction')}
                  onCancel={() => onCancelProfileField('introduction')}
                />
              </Form>
            ),
          },
          {
            key: 'requirements',
            label: '需求',
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <div className="toolbar">
                  <Button type="primary" icon={<PlusOutlined />} onClick={onOpenRequirementModal}>
                    新建需求
                  </Button>
                </div>
                <Table<RequirementTableRecord>
                  rowKey="id"
                  loading={requirementsLoading}
                  columns={requirementColumns}
                  dataSource={requirementTree}
                  expandable={{ indentSize: 0, expandIcon: renderRequirementExpandIcon }}
                  pagination={{ pageSize: 8, showSizeChanger: true }}
                  scroll={{ x: 1220 }}
                />
              </Space>
            ),
          },
          {
            key: 'work-items',
            label: '项目任务',
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <div className="toolbar">
                  <Button
                    type="primary"
                    icon={<PlusOutlined />}
                    disabled={selectableRequirementCount === 0}
                    onClick={onOpenWorkItemModal}
                  >
                    新建项目任务
                  </Button>
                </div>
                <Table<ProjectWorkItemRecord>
                  rowKey="id"
                  loading={workItemsLoading}
                  columns={workItemColumns}
                  dataSource={workItems}
                  pagination={{ pageSize: 8, showSizeChanger: true }}
                  scroll={{ x: 1640 }}
                />
              </Space>
            ),
          },
          {
            key: 'graph',
            label: '依赖图',
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <Row gutter={16}>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic
                        title="需求"
                        value={graphNodes.filter((node) => node.node_type === 'requirement').length}
                      />
                    </Card>
                  </Col>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic
                        title="项目任务"
                        value={graphNodes.filter((node) => node.node_type === 'work_item').length}
                      />
                    </Card>
                  </Col>
                  <Col xs={24} md={8}>
                    <Card>
                      <Statistic title="阻塞关系" value={blockingRelations.length} />
                    </Card>
                  </Col>
                </Row>
                <Table<GraphRelationRow>
                  rowKey="key"
                  size="small"
                  loading={graphLoading}
                  dataSource={blockingRelations}
                  pagination={false}
                  title={() => '阻塞关系'}
                  locale={{ emptyText: '暂无阻塞关系' }}
                  columns={[
                    {
                      title: '前置项',
                      render: (_, record) => renderGraphNode(record.from, record.edge.from),
                    },
                    {
                      title: '被阻塞项',
                      render: (_, record) => renderGraphNode(record.to, record.edge.to),
                    },
                    {
                      title: '关系',
                      width: 120,
                      render: () => <Tag color="error">阻塞</Tag>,
                    },
                  ]}
                />
                <Table<GraphRelationRow>
                  rowKey="key"
                  size="small"
                  loading={graphLoading}
                  dataSource={containsRelations}
                  pagination={{ pageSize: 8 }}
                  title={() => '需求拆分'}
                  locale={{ emptyText: '暂无项目任务' }}
                  columns={[
                    {
                      title: '需求',
                      render: (_, record) => renderGraphNode(record.from, record.edge.from),
                    },
                    {
                      title: '项目任务',
                      render: (_, record) => renderGraphNode(record.to, record.edge.to),
                    },
                    {
                      title: '关系',
                      width: 120,
                      render: () => <Tag color="blue">拆分</Tag>,
                    },
                  ]}
                />
                <Table<DependencyGraphNode>
                  rowKey="id"
                  size="small"
                  loading={graphLoading}
                  dataSource={graphNodes}
                  pagination={{ pageSize: 8 }}
                  title={() => '对象清单'}
                  columns={[
                    {
                      title: '对象',
                      render: (_, record) => renderGraphNode(record, record.id),
                    },
                    {
                      title: '状态',
                      width: 140,
                      render: (_, record) => graphStatusTag(record),
                    },
                  ]}
                />
              </Space>
            ),
          },
        ]}
      />
    </>
  );
}

function projectSourceTag(source?: ProjectRecord['source_type']) {
  if (source === 'cloud') {
    return <Tag color="geekblue">云端项目</Tag>;
  }
  if (source === 'local_connector') {
    return <Tag color="cyan">本地连接器</Tag>;
  }
  if (source === 'local') {
    return <Tag>本地项目</Tag>;
  }
  return <Tag>未知</Tag>;
}

function projectImportStatusTag(status?: ProjectRecord['import_status']) {
  if (!status || status === 'none') {
    return <Tag>无</Tag>;
  }
  const color =
    status === 'ready'
      ? 'success'
      : status === 'failed'
        ? 'error'
        : status === 'pending' || status === 'importing'
          ? 'processing'
          : 'default';
  const label =
    status === 'ready'
      ? '已就绪'
      : status === 'failed'
        ? '失败'
        : status === 'pending'
          ? '等待导入'
          : status === 'importing'
            ? '导入中'
            : status;
  return <Tag color={color}>{label}</Tag>;
}
