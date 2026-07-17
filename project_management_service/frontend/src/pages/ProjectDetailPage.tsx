// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Form, message } from 'antd';
import { useParams } from 'react-router-dom';

import { api } from '../api/client';
import type {
  CreateRequirementPayload,
  DependencyGraphNode,
  ProjectProfileRecord,
  ProjectRuntimeEnvironmentDeploymentResponse,
  ProjectRuntimeEnvironmentResponse,
  ProjectWorkItemRecord,
  RequirementRecord,
  UpsertProjectProfilePayload,
  UpdateProjectRuntimeEnvironmentVariablesPayload,
} from '../types';
import { buildProjectDetailColumns } from './projectDetail/columns';
import { ProjectDetailOverlays } from './projectDetail/ProjectDetailOverlays';
import { ProjectDetailTabs } from './projectDetail/ProjectDetailTabs';
import type {
  GraphRelationRow,
  ProfileMarkdownFieldName,
  WorkItemFormValues,
} from './projectDetail/types';
import { emptyRequirements, emptyWorkItems } from './projectDetail/types';
import {
  buildCreateWorkItemPayload,
  buildRequirementTree,
  isSelectableRequirement,
  isSelectableWorkItem,
} from './projectDetail/utils';

const emptyGraphNodes: DependencyGraphNode[] = [];

export function ProjectDetailPage() {
  const { projectId } = useParams<{ projectId: string }>();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [profileForm] = Form.useForm<UpsertProjectProfilePayload>();
  const [requirementForm] = Form.useForm<CreateRequirementPayload>();
  const [workItemForm] = Form.useForm<WorkItemFormValues>();
  const [requirementModalOpen, setRequirementModalOpen] = useState(false);
  const [workItemModalOpen, setWorkItemModalOpen] = useState(false);
  const [requirementDetailTarget, setRequirementDetailTarget] =
    useState<RequirementRecord | null>(null);
  const [workItemDetailTarget, setWorkItemDetailTarget] =
    useState<ProjectWorkItemRecord | null>(null);
  const [requirementDepTarget, setRequirementDepTarget] = useState<RequirementRecord | null>(null);
  const [workItemDepTarget, setWorkItemDepTarget] = useState<ProjectWorkItemRecord | null>(null);
  const [requirementDepIds, setRequirementDepIds] = useState<string[]>([]);
  const [workItemDepIds, setWorkItemDepIds] = useState<string[]>([]);
  const [docTarget, setDocTarget] = useState<RequirementRecord | null>(null);
  const [editingProfileField, setEditingProfileField] =
    useState<ProfileMarkdownFieldName | null>(null);
  const [showArchived, setShowArchived] = useState(false);

  const projectQuery = useQuery({
    queryKey: ['project', projectId],
    queryFn: () => api.getProject(projectId!),
    enabled: Boolean(projectId),
  });
  const profileQuery = useQuery({
    queryKey: ['project-profile', projectId],
    queryFn: () => api.getProjectProfile(projectId!),
    enabled: Boolean(projectId),
  });
  const requirementsQuery = useQuery({
    queryKey: ['requirements', projectId, showArchived],
    queryFn: () => api.listRequirements(projectId!, { include_archived: showArchived }),
    enabled: Boolean(projectId),
  });
  const workItemsQuery = useQuery({
    queryKey: ['work-items', projectId, showArchived],
    queryFn: () => api.listProjectWorkItems(projectId!, { include_archived: showArchived }),
    enabled: Boolean(projectId),
  });
  const graphQuery = useQuery({
    queryKey: ['project-graph', projectId, showArchived],
    queryFn: () => api.getProjectDependencyGraph(projectId!, { include_archived: showArchived }),
    enabled: Boolean(projectId),
  });
  const runtimeEnvironmentQuery = useQuery({
    queryKey: ['project-runtime-environment', projectId],
    queryFn: () => api.getProjectRuntimeEnvironment(projectId!),
    enabled: Boolean(projectId),
  });
  const runtimeEnvironmentDeploymentQuery = useQuery<ProjectRuntimeEnvironmentDeploymentResponse>({
    queryKey: ['project-runtime-environment-deployment', projectId],
    queryFn: () => api.getProjectRuntimeEnvironmentDeployment(projectId!),
    enabled: Boolean(
      projectId &&
        runtimeEnvironmentQuery.data?.environment.sandbox_provider === 'local_connector' &&
        runtimeEnvironmentQuery.data.images.some((image) =>
          ['running', 'starting', 'stopped'].includes(image.status),
        ),
    ),
    retry: false,
  });
  const requirementDepsQuery = useQuery({
    queryKey: ['requirement-deps', requirementDepTarget?.id],
    queryFn: () => api.listRequirementDependencies(requirementDepTarget!.id),
    enabled: Boolean(requirementDepTarget),
  });
  const workItemDepsQuery = useQuery({
    queryKey: ['work-item-deps', workItemDepTarget?.id],
    queryFn: () => api.listWorkItemDependencies(workItemDepTarget!.id),
    enabled: Boolean(workItemDepTarget),
  });
  const docQuery = useQuery({
    queryKey: ['requirement-documents', docTarget?.id],
    queryFn: () => api.listRequirementDocuments(docTarget!.id),
    enabled: Boolean(docTarget),
  });

  const requirements = requirementsQuery.data ?? emptyRequirements;
  const workItems = workItemsQuery.data ?? emptyWorkItems;
  const selectableRequirements = useMemo(
    () => requirements.filter(isSelectableRequirement),
    [requirements],
  );
  const selectableWorkItems = useMemo(() => workItems.filter(isSelectableWorkItem), [workItems]);
  const selectableRequirementIds = useMemo(
    () => new Set(selectableRequirements.map((item) => item.id)),
    [selectableRequirements],
  );
  const selectableWorkItemIds = useMemo(
    () => new Set(selectableWorkItems.map((item) => item.id)),
    [selectableWorkItems],
  );
  const requirementTree = useMemo(() => buildRequirementTree(requirements), [requirements]);
  const profileBackground = profileQuery.data?.background || undefined;
  const profileIntroduction = profileQuery.data?.introduction || undefined;

  const project = projectQuery.data;
  const graphNodes = graphQuery.data?.nodes ?? emptyGraphNodes;
  const graphNodeMap = useMemo(
    () => new Map(graphNodes.map((node) => [node.id, node])),
    [graphNodes],
  );
  const graphRelations = useMemo<GraphRelationRow[]>(
    () =>
      (graphQuery.data?.edges || []).map((edge, index) => ({
        key: `${edge.from}-${edge.to}-${edge.edge_type}-${index}`,
        edge,
        from: graphNodeMap.get(edge.from),
        to: graphNodeMap.get(edge.to),
      })),
    [graphNodeMap, graphQuery.data?.edges],
  );
  const blockingRelations = graphRelations.filter((item) => item.edge.edge_type === 'blocks');
  const containsRelations = graphRelations.filter((item) => item.edge.edge_type === 'contains');

  const cancelProfileFieldEdit = (field: ProfileMarkdownFieldName) => {
    profileForm.setFieldValue(field, profileQuery.data?.[field] || undefined);
    setEditingProfileField(null);
  };

  useEffect(() => {
    if (profileQuery.data) {
      profileForm.setFieldsValue({
        background: profileQuery.data.background || undefined,
        introduction: profileQuery.data.introduction || undefined,
      });
    }
  }, [profileForm, profileQuery.data]);

  useEffect(() => {
    if (requirementDepsQuery.data) {
      setRequirementDepIds(
        requirementDepsQuery.data
          .map((item) => item.prerequisite_requirement_id)
          .filter((id) => selectableRequirementIds.has(id)),
      );
    }
  }, [requirementDepsQuery.data, selectableRequirementIds]);

  useEffect(() => {
    if (workItemDepsQuery.data) {
      setWorkItemDepIds(
        workItemDepsQuery.data
          .map((item) => item.prerequisite_work_item_id)
          .filter((id) => selectableWorkItemIds.has(id)),
      );
    }
  }, [workItemDepsQuery.data, selectableWorkItemIds]);

  const invalidateProjectData = () => {
    queryClient.invalidateQueries({ queryKey: ['requirements', projectId] });
    queryClient.invalidateQueries({ queryKey: ['work-items', projectId] });
    queryClient.invalidateQueries({ queryKey: ['project-graph', projectId] });
  };

  const setRuntimeEnvironmentCache = (data: ProjectRuntimeEnvironmentResponse) => {
    queryClient.setQueryData(['project-runtime-environment', projectId], data);
  };

  const profileMutation = useMutation({
    mutationFn: (payload: UpsertProjectProfilePayload) =>
      api.upsertProjectProfile(projectId!, payload),
    onSuccess: (profile: ProjectProfileRecord) => {
      messageApi.success('项目详情已保存');
      setEditingProfileField(null);
      queryClient.setQueryData(['project-profile', projectId], profile);
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const createRequirementMutation = useMutation({
    mutationFn: (payload: CreateRequirementPayload) => api.createRequirement(projectId!, payload),
    onSuccess: () => {
      messageApi.success('需求已创建');
      setRequirementModalOpen(false);
      requirementForm.resetFields();
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const archiveRequirementMutation = useMutation({
    mutationFn: (id: string) => api.archiveRequirement(id),
    onSuccess: () => {
      messageApi.success('需求已归档');
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const saveRequirementDepsMutation = useMutation({
    mutationFn: () => api.setRequirementDependencies(requirementDepTarget!.id, requirementDepIds),
    onSuccess: () => {
      messageApi.success('需求前置关系已保存');
      setRequirementDepTarget(null);
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const createWorkItemMutation = useMutation({
    mutationFn: (values: WorkItemFormValues) =>
      api.createWorkItem(values.requirement_id, buildCreateWorkItemPayload(values)),
    onSuccess: () => {
      messageApi.success('项目任务已创建');
      setWorkItemModalOpen(false);
      workItemForm.resetFields();
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const archiveWorkItemMutation = useMutation({
    mutationFn: (id: string) => api.archiveWorkItem(id),
    onSuccess: () => {
      messageApi.success('项目任务已归档');
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const saveWorkItemDepsMutation = useMutation({
    mutationFn: () => api.setWorkItemDependencies(workItemDepTarget!.id, workItemDepIds),
    onSuccess: () => {
      messageApi.success('项目任务前置关系已保存');
      setWorkItemDepTarget(null);
      invalidateProjectData();
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const updateRuntimeEnvironmentSettingsMutation = useMutation({
    mutationFn: (sandboxEnabled: boolean) =>
      api.updateProjectRuntimeEnvironmentSettings(projectId!, {
        sandbox_enabled: sandboxEnabled,
      }),
    onSuccess: (data, sandboxEnabled) => {
      messageApi.success(sandboxEnabled ? '已启用项目沙箱初始化' : '已停用项目沙箱初始化');
      setRuntimeEnvironmentCache(data);
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const analyzeRuntimeEnvironmentMutation = useMutation({
    mutationFn: () => api.analyzeProjectRuntimeEnvironment(projectId!),
    onSuccess: (data) => {
      messageApi.success('运行环境初始化已完成');
      setRuntimeEnvironmentCache(data);
    },
    onError: (error) => {
      messageApi.error((error as Error).message);
      runtimeEnvironmentQuery.refetch();
    },
  });

  const updateRuntimeEnvironmentVariablesMutation = useMutation({
    mutationFn: (payload: UpdateProjectRuntimeEnvironmentVariablesPayload) =>
      api.updateProjectRuntimeEnvironmentVariables(projectId!, payload),
    onSuccess: (data) => {
      messageApi.success('运行环境变量已保存');
      setRuntimeEnvironmentCache(data);
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const startRuntimeEnvironmentMutation = useMutation({
    mutationFn: () => api.startProjectRuntimeEnvironment(projectId!),
    onSuccess: (data) => {
      messageApi.success('项目运行环境已作为一个 Docker Compose 项目启动');
      setRuntimeEnvironmentCache(data);
      queryClient.invalidateQueries({
        queryKey: ['project-runtime-environment-deployment', projectId],
      });
    },
    onError: (error) => {
      messageApi.error((error as Error).message);
      runtimeEnvironmentQuery.refetch();
    },
  });

  const stopRuntimeEnvironmentMutation = useMutation({
    mutationFn: () => api.stopProjectRuntimeEnvironment(projectId!),
    onSuccess: (data) => {
      messageApi.success('项目级 Docker Compose 环境已整体停止，数据卷已保留');
      setRuntimeEnvironmentCache(data);
      queryClient.invalidateQueries({
        queryKey: ['project-runtime-environment-deployment', projectId],
      });
    },
    onError: (error) => messageApi.error((error as Error).message),
  });

  const restartRuntimeEnvironmentMutation = useMutation({
    mutationFn: () => api.restartProjectRuntimeEnvironment(projectId!),
    onSuccess: (data) => {
      messageApi.success('项目级 Docker Compose 环境已整体重启');
      setRuntimeEnvironmentCache(data);
      queryClient.invalidateQueries({
        queryKey: ['project-runtime-environment-deployment', projectId],
      });
    },
    onError: (error) => {
      messageApi.error((error as Error).message);
      runtimeEnvironmentQuery.refetch();
    },
  });

  const { requirementColumns, workItemColumns } = buildProjectDetailColumns({
    requirements,
    onShowRequirementDetail: setRequirementDetailTarget,
    onShowRequirementDeps: setRequirementDepTarget,
    onShowRequirementDoc: setDocTarget,
    onArchiveRequirement: archiveRequirementMutation.mutate,
    onShowWorkItemDetail: setWorkItemDetailTarget,
    onShowWorkItemDeps: setWorkItemDepTarget,
    onArchiveWorkItem: archiveWorkItemMutation.mutate,
  });

  if (!projectId) {
    return null;
  }

  return (
    <div className="page">
      {contextHolder}
      <ProjectDetailTabs
        projectId={projectId}
        project={project}
        showArchived={showArchived}
        onShowArchivedChange={setShowArchived}
        onRefresh={() => {
          projectQuery.refetch();
          profileQuery.refetch();
          requirementsQuery.refetch();
          workItemsQuery.refetch();
          graphQuery.refetch();
          runtimeEnvironmentQuery.refetch();
        }}
        profileForm={profileForm}
        profileBackground={profileBackground}
        profileIntroduction={profileIntroduction}
        editingProfileField={editingProfileField}
        profileSaving={profileMutation.isPending}
        onEditProfileField={setEditingProfileField}
        onCancelProfileField={cancelProfileFieldEdit}
        onSaveProfile={(values) => profileMutation.mutate(values)}
        requirements={requirements}
        workItems={workItems}
        selectableRequirementCount={selectableRequirements.length}
        requirementTree={requirementTree}
        requirementColumns={requirementColumns}
        workItemColumns={workItemColumns}
        requirementsLoading={requirementsQuery.isLoading}
        workItemsLoading={workItemsQuery.isLoading}
        onOpenRequirementModal={() => setRequirementModalOpen(true)}
        onOpenWorkItemModal={() => setWorkItemModalOpen(true)}
        graphNodes={graphNodes}
        graphLoading={graphQuery.isLoading}
        blockingRelations={blockingRelations}
        containsRelations={containsRelations}
        runtimeEnvironment={runtimeEnvironmentQuery.data}
        runtimeEnvironmentDeployment={runtimeEnvironmentDeploymentQuery.data}
        runtimeEnvironmentLoading={runtimeEnvironmentQuery.isLoading}
        runtimeEnvironmentDeploymentLoading={runtimeEnvironmentDeploymentQuery.isFetching}
        runtimeEnvironmentErrorMessage={
          runtimeEnvironmentQuery.isError
            ? (runtimeEnvironmentQuery.error as Error).message
            : undefined
        }
        runtimeEnvironmentAnalyzing={analyzeRuntimeEnvironmentMutation.isPending}
        runtimeEnvironmentSettingsSaving={updateRuntimeEnvironmentSettingsMutation.isPending}
        runtimeEnvironmentVariablesSaving={updateRuntimeEnvironmentVariablesMutation.isPending}
        runtimeEnvironmentStarting={startRuntimeEnvironmentMutation.isPending}
        runtimeEnvironmentStopping={stopRuntimeEnvironmentMutation.isPending}
        runtimeEnvironmentRestarting={restartRuntimeEnvironmentMutation.isPending}
        onRefreshRuntimeEnvironment={() => runtimeEnvironmentQuery.refetch()}
        onAnalyzeRuntimeEnvironment={() => analyzeRuntimeEnvironmentMutation.mutate()}
        onRuntimeSandboxEnabledChange={(value) =>
          updateRuntimeEnvironmentSettingsMutation.mutate(value)
        }
        onSaveRuntimeEnvironmentVariables={async (payload) => {
          await updateRuntimeEnvironmentVariablesMutation.mutateAsync(payload);
        }}
        onStartRuntimeEnvironment={() => startRuntimeEnvironmentMutation.mutate()}
        onRefreshRuntimeEnvironmentDeployment={() => runtimeEnvironmentDeploymentQuery.refetch()}
        onStopRuntimeEnvironment={() => stopRuntimeEnvironmentMutation.mutate()}
        onRestartRuntimeEnvironment={() => restartRuntimeEnvironmentMutation.mutate()}
      />
      <ProjectDetailOverlays
        requirementModalOpen={requirementModalOpen}
        onCloseRequirementModal={() => setRequirementModalOpen(false)}
        requirementForm={requirementForm}
        createRequirementPending={createRequirementMutation.isPending}
        onCreateRequirement={(values) => createRequirementMutation.mutate(values)}
        requirementDepTarget={requirementDepTarget}
        onCloseRequirementDeps={() => setRequirementDepTarget(null)}
        onSaveRequirementDeps={() => saveRequirementDepsMutation.mutate()}
        saveRequirementDepsPending={saveRequirementDepsMutation.isPending}
        requirementDepsLoading={requirementDepsQuery.isLoading}
        requirementDepIds={requirementDepIds}
        onRequirementDepIdsChange={setRequirementDepIds}
        selectableRequirements={selectableRequirements}
        docTarget={docTarget}
        onCloseDoc={() => setDocTarget(null)}
        docLoading={docQuery.isLoading}
        docDocuments={docQuery.data}
        workItemModalOpen={workItemModalOpen}
        onCloseWorkItemModal={() => setWorkItemModalOpen(false)}
        workItemForm={workItemForm}
        createWorkItemPending={createWorkItemMutation.isPending}
        onCreateWorkItem={(values) => createWorkItemMutation.mutate(values)}
        workItemDepTarget={workItemDepTarget}
        onCloseWorkItemDeps={() => setWorkItemDepTarget(null)}
        onSaveWorkItemDeps={() => saveWorkItemDepsMutation.mutate()}
        saveWorkItemDepsPending={saveWorkItemDepsMutation.isPending}
        workItemDepsLoading={workItemDepsQuery.isLoading}
        workItemDepIds={workItemDepIds}
        onWorkItemDepIdsChange={setWorkItemDepIds}
        selectableWorkItems={selectableWorkItems}
        requirementDetailTarget={requirementDetailTarget}
        onCloseRequirementDetail={() => setRequirementDetailTarget(null)}
        workItemDetailTarget={workItemDetailTarget}
        onCloseWorkItemDetail={() => setWorkItemDetailTarget(null)}
        requirements={requirements}
      />
    </div>
  );
}
