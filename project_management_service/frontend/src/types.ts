// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type UserRole = 'admin' | 'agent';
export type ProjectStatus = 'active' | 'archived';
export type ProjectSourceType = 'local' | 'local_connector' | 'cloud';
export type CloudImportSource = 'none' | 'empty' | 'git' | 'zip';
export type ProjectImportStatus = 'none' | 'pending' | 'importing' | 'ready' | 'failed';
export type ProjectRuntimeEnvironmentStatus =
  | 'disabled'
  | 'pending_configuration'
  | 'pending'
  | 'analyzing'
  | 'ready'
  | 'not_runnable'
  | 'failed';
export type RuntimeEnvironmentProvider =
  | 'none'
  | 'local_connector'
  | 'harness'
  | 'cloud_sandbox_manager';
export type RequirementStatus =
  | 'draft'
  | 'reviewing'
  | 'approved'
  | 'in_progress'
  | 'blocked'
  | 'failed'
  | 'done'
  | 'cancelled'
  | 'archived';
export type RequirementType = 'requirement' | 'change' | 'bug_fix';
export type ProjectWorkItemStatus =
  | 'todo'
  | 'ready'
  | 'in_progress'
  | 'blocked'
  | 'failed'
  | 'done'
  | 'cancelled'
  | 'archived';

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
}

export interface LoginPayload {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

export interface AgentAccountListItem {
  id: string;
  username: string;
  display_name: string;
  owner_user_id: string;
  owner_username: string;
  owner_display_name: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string | null;
}

export type ProjectManagementSkillLocale = 'zh-CN' | 'en-US';

export interface ProjectManagementSkillResponse {
  name: string;
  locale: ProjectManagementSkillLocale;
  content: string;
}

export interface ProjectRecord {
  id: string;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  name: string;
  root_path?: string | null;
  git_url?: string | null;
  source_type?: ProjectSourceType;
  cloud_import_source?: CloudImportSource;
  import_status?: ProjectImportStatus;
  source_git_url?: string | null;
  harness_space_identifier?: string | null;
  harness_repo_identifier?: string | null;
  harness_repo_path?: string | null;
  harness_git_url?: string | null;
  harness_git_ssh_url?: string | null;
  import_error?: string | null;
  import_started_at?: string | null;
  import_finished_at?: string | null;
  description?: string | null;
  status: ProjectStatus;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
}

export interface CreateProjectPayload {
  name: string;
  root_path?: string;
  git_url?: string;
  description?: string;
  sandbox_enabled?: boolean;
}

export type UpdateProjectPayload = Partial<CreateProjectPayload>;

export interface ProjectProfileRecord {
  project_id: string;
  background?: string | null;
  introduction?: string | null;
  created_at: string;
  updated_at: string;
}

export interface UpsertProjectProfilePayload {
  background?: string;
  introduction?: string;
}

export interface ProjectRuntimeEnvironmentRecord {
  project_id: string;
  status: ProjectRuntimeEnvironmentStatus;
  sandbox_enabled: boolean;
  sandbox_provider: RuntimeEnvironmentProvider;
  file_provider: RuntimeEnvironmentProvider;
  analysis_summary?: string | null;
  not_runnable_reason?: string | null;
  detected_stack: unknown;
  required_services: unknown;
  env_vars: unknown;
  last_agent_run_id?: string | null;
  last_error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProjectRuntimeEnvironmentImageRecord {
  id: string;
  project_id: string;
  environment_key: string;
  environment_type: string;
  display_name: string;
  image_id?: string | null;
  image_ref?: string | null;
  image_provider: RuntimeEnvironmentProvider;
  features: unknown;
  ports: unknown;
  env_vars: unknown;
  status: string;
  error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProjectRuntimeEnvironmentResponse {
  environment: ProjectRuntimeEnvironmentRecord;
  images: ProjectRuntimeEnvironmentImageRecord[];
}

export interface UpdateProjectRuntimeEnvironmentSettingsPayload {
  sandbox_enabled?: boolean;
}

export interface RequirementRecord {
  id: string;
  project_id: string;
  parent_requirement_id?: string | null;
  requirement_type: RequirementType;
  title: string;
  summary?: string | null;
  detail?: string | null;
  business_value?: string | null;
  acceptance_criteria?: string | null;
  source?: string | null;
  priority: number;
  status: RequirementStatus;
  owner_user_id?: string | null;
  assignee_user_id?: string | null;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
}

export interface CreateRequirementPayload {
  parent_requirement_id?: string;
  requirement_type?: RequirementType;
  title: string;
  summary?: string;
  detail?: string;
  business_value?: string;
  acceptance_criteria?: string;
  source?: string;
  priority?: number;
  status?: RequirementStatus;
  assignee_user_id?: string;
}

export type UpdateRequirementPayload = Partial<CreateRequirementPayload>;

export interface RequirementDependencyRecord {
  requirement_id: string;
  prerequisite_requirement_id: string;
  relation_type: string;
  created_at: string;
}

export interface RequirementDocumentRecord {
  id: string;
  requirement_id: string;
  doc_type: string;
  title: string;
  format: string;
  content: string;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface UpsertRequirementDocumentPayload {
  doc_type?: string;
  title?: string;
  format?: string;
  content: string;
}

export interface UpdateRequirementDocumentPayload {
  doc_type?: string;
  title?: string;
  format?: string;
  content?: string;
}

export interface ProjectWorkItemRecord {
  id: string;
  project_id: string;
  requirement_id: string;
  title: string;
  description?: string | null;
  status: ProjectWorkItemStatus;
  priority: number;
  assignee_user_id?: string | null;
  estimate_points?: number | null;
  due_at?: string | null;
  sort_order: number;
  tags: string[];
  is_planning_task: boolean;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
}

export interface CreateWorkItemPayload {
  title: string;
  description?: string;
  status?: ProjectWorkItemStatus;
  priority?: number;
  assignee_user_id?: string;
  estimate_points?: number;
  due_at?: string;
  sort_order?: number;
  tags?: string[];
  is_planning_task?: boolean;
}

export type UpdateWorkItemPayload = Partial<CreateWorkItemPayload> & {
  requirement_id?: string;
};

export interface WorkItemDependencyRecord {
  work_item_id: string;
  prerequisite_work_item_id: string;
  relation_type: string;
  created_at: string;
}

export interface ProjectWorkItemTaskRunnerLinkRecord {
  id: string;
  work_item_id: string;
  task_runner_task_id: string;
  task_runner_run_id?: string | null;
  link_type: string;
  execution_group_id?: string | null;
  is_current: boolean;
  superseded_at?: string | null;
  source_session_id?: string | null;
  source_user_message_id?: string | null;
  task_runner_status?: string | null;
  last_callback_event?: string | null;
  last_callback_at?: string | null;
  last_error_message?: string | null;
  created_at: string;
  updated_at: string;
}

export interface DependencyGraphNode {
  id: string;
  node_type: string;
  label: string;
  status: string;
  parent_id?: string | null;
  raw_id: string;
}

export interface DependencyGraphEdge {
  from: string;
  to: string;
  edge_type: string;
}

export interface DependencyGraphResponse {
  root_id?: string | null;
  nodes: DependencyGraphNode[];
  edges: DependencyGraphEdge[];
  blocked_by: DependencyGraphNode[];
  ready: boolean;
}
