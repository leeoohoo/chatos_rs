// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { SessionMessageResponse } from './session';

export interface ProjectResponse {
  id: string;
  name: string;
  root_path?: string;
  rootPath?: string;
  display_root_path?: string | null;
  displayRootPath?: string | null;
  git_url?: string | null;
  gitUrl?: string | null;
  source_type?: string | null;
  sourceType?: string | null;
  cloud_import_source?: string | null;
  cloudImportSource?: string | null;
  import_status?: string | null;
  importStatus?: string | null;
  source_git_url?: string | null;
  sourceGitUrl?: string | null;
  harness_space_identifier?: string | null;
  harnessSpaceIdentifier?: string | null;
  harness_repo_identifier?: string | null;
  harnessRepoIdentifier?: string | null;
  harness_repo_path?: string | null;
  harnessRepoPath?: string | null;
  harness_git_url?: string | null;
  harnessGitUrl?: string | null;
  harness_git_ssh_url?: string | null;
  harnessGitSshUrl?: string | null;
  import_error?: string | null;
  importError?: string | null;
  import_started_at?: string | null;
  importStartedAt?: string | null;
  import_finished_at?: string | null;
  importFinishedAt?: string | null;
  description?: string | null;
  user_id?: string | null;
  userId?: string | null;
  latest_session_id?: string | null;
  latestSessionId?: string | null;
  last_message_at?: string | null;
  lastMessageAt?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface ProjectRunTargetResponse {
  id: string;
  label?: string;
  kind?: string;
  language?: string | null;
  cwd?: string;
  command?: string;
  source?: string;
  confidence?: number;
  is_default?: boolean;
  isDefault?: boolean;
  entrypoint?: string | null;
  entry_point?: string | null;
  manifest_path?: string | null;
  manifestPath?: string | null;
  required_toolchains?: string[];
  requiredToolchains?: string[];
}

export interface ProjectRunCatalogResponse {
  project_id?: string;
  projectId?: string;
  status?: string;
  default_target_id?: string | null;
  defaultTargetId?: string | null;
  targets?: ProjectRunTargetResponse[];
  error_message?: string | null;
  errorMessage?: string | null;
  analyzed_at?: string | null;
  analyzedAt?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectRunExecuteResponse {
  success?: boolean;
  status?: string;
  run_id?: string;
  runId?: string;
  terminal_id?: string;
  terminalId?: string;
  target_id?: string;
  targetId?: string;
  command?: string;
  cwd?: string;
  message?: string;
  error?: string;
  env_overrides?: Record<string, string>;
  envOverrides?: Record<string, string>;
}

export interface ProjectRequirementExecutionTaskResponse {
  project_task_id?: string;
  projectTaskId?: string;
  requirement_id?: string;
  requirementId?: string;
  task_runner_task_id?: string;
  taskRunnerTaskId?: string;
  task_runner_run_id?: string | null;
  taskRunnerRunId?: string | null;
  task_runner_status?: string;
  taskRunnerStatus?: string;
}

export interface ProjectRequirementExecuteResponse {
  success?: boolean;
  project_id?: string;
  projectId?: string;
  requirement_id?: string;
  requirementId?: string;
  contact_id?: string;
  contactId?: string;
  conversation_id?: string;
  conversationId?: string;
  message_id?: string;
  messageId?: string;
  message?: SessionMessageResponse | null;
  created_tasks?: ProjectRequirementExecutionTaskResponse[];
  createdTasks?: ProjectRequirementExecutionTaskResponse[];
  plan_mode_enabled?: boolean;
  planModeEnabled?: boolean;
}

export interface ProjectRequirementStopResponse {
  success?: boolean;
  project_id?: string;
  projectId?: string;
  requirement_id?: string;
  requirementId?: string;
  contact_id?: string;
  contactId?: string;
  cancelled_tasks?: unknown[];
  cancelledTasks?: unknown[];
  skipped_tasks?: unknown[];
  skippedTasks?: unknown[];
  reset_work_item_ids?: string[];
  resetWorkItemIds?: string[];
}

export interface ProjectRunStateResponse {
  project_id?: string;
  projectId?: string;
  running?: boolean;
  busy?: boolean;
  status?: string;
  terminal_id?: string | null;
  terminalId?: string | null;
  terminal_name?: string | null;
  terminalName?: string | null;
  cwd?: string | null;
  terminal?: import('./terminal').TerminalResponse | null;
  instances?: Array<{
    terminal_id?: string | null;
    terminalId?: string | null;
    terminal_name?: string | null;
    terminalName?: string | null;
    cwd?: string | null;
    status?: string;
    busy?: boolean;
    running?: boolean;
    terminal?: import('./terminal').TerminalResponse | null;
  }>;
}

export interface ProjectRunToolchainOptionResponse {
  id: string;
  kind?: string;
  label?: string;
  version?: string | null;
  path?: string;
  source?: string;
  is_default?: boolean;
  isDefault?: boolean;
}

export interface ProjectRunConfigFileSummaryResponse {
  kind?: string;
  label?: string;
  path?: string;
  preview?: string | null;
  source?: string;
}

export interface ProjectRunValidationIssueResponse {
  kind?: string;
  message?: string;
  target_id?: string | null;
  targetId?: string | null;
  target_label?: string | null;
  targetLabel?: string | null;
  path?: string | null;
  hint?: string | null;
}

export interface ProjectRunCustomToolchainResponse {
  kind?: string;
  label?: string;
  path?: string;
}

export interface ProjectRunEnvironmentResponse {
  project_id?: string;
  projectId?: string;
  user_id?: string | null;
  userId?: string | null;
  options_by_kind?: Record<string, ProjectRunToolchainOptionResponse[]>;
  optionsByKind?: Record<string, ProjectRunToolchainOptionResponse[]>;
  config_files?: ProjectRunConfigFileSummaryResponse[];
  configFiles?: ProjectRunConfigFileSummaryResponse[];
  validation_issues?: ProjectRunValidationIssueResponse[];
  validationIssues?: ProjectRunValidationIssueResponse[];
  selected_toolchains?: Record<string, string>;
  selectedToolchains?: Record<string, string>;
  custom_toolchains?: Record<string, ProjectRunCustomToolchainResponse>;
  customToolchains?: Record<string, ProjectRunCustomToolchainResponse>;
  env_vars?: Record<string, string>;
  envVars?: Record<string, string>;
  terminal_ui_enabled?: boolean;
  terminalUiEnabled?: boolean;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectContactLinkResponse {
  contact_id?: string;
  contactId?: string;
  agent_id?: string;
  agentId?: string;
  agent_name_snapshot?: string | null;
  agentNameSnapshot?: string | null;
  latest_session_id?: string | null;
  latestSessionId?: string | null;
  last_bound_at?: string | null;
  lastBoundAt?: string | null;
  last_message_at?: string | null;
  lastMessageAt?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectContactLockResponse {
  locked?: boolean;
  error?: string;
  detail?: string;
}

export type ProjectRequirementStatus =
  | 'draft'
  | 'reviewing'
  | 'approved'
  | 'in_progress'
  | 'blocked'
  | 'failed'
  | 'done'
  | 'cancelled'
  | 'archived';

export type ProjectRequirementType = 'requirement' | 'change' | 'bug_fix';

export type ProjectWorkItemStatus =
  | 'todo'
  | 'ready'
  | 'in_progress'
  | 'blocked'
  | 'failed'
  | 'done'
  | 'cancelled'
  | 'archived';

export interface ProjectRequirementResponse {
  id: string;
  project_id?: string;
  projectId?: string;
  parent_requirement_id?: string | null;
  parentRequirementId?: string | null;
  requirement_type?: ProjectRequirementType;
  requirementType?: ProjectRequirementType;
  title: string;
  summary?: string | null;
  detail?: string | null;
  business_value?: string | null;
  businessValue?: string | null;
  acceptance_criteria?: string | null;
  acceptanceCriteria?: string | null;
  source?: string | null;
  priority?: number;
  status?: ProjectRequirementStatus;
  creator_user_id?: string | null;
  creatorUserId?: string | null;
  creator_username?: string | null;
  creatorUsername?: string | null;
  creator_display_name?: string | null;
  creatorDisplayName?: string | null;
  owner_user_id?: string | null;
  ownerUserId?: string | null;
  owner_username?: string | null;
  ownerUsername?: string | null;
  owner_display_name?: string | null;
  ownerDisplayName?: string | null;
  assignee_user_id?: string | null;
  assigneeUserId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  archived_at?: string | null;
  archivedAt?: string | null;
}

export interface ProjectRequirementDocumentResponse {
  id: string;
  requirement_id?: string;
  requirementId?: string;
  doc_type?: string;
  docType?: string;
  title?: string;
  format?: string;
  content?: string;
  version?: number;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface ProjectWorkItemResponse {
  id: string;
  project_id?: string;
  projectId?: string;
  requirement_id?: string;
  requirementId?: string;
  title: string;
  description?: string | null;
  status?: ProjectWorkItemStatus;
  priority?: number;
  assignee_user_id?: string | null;
  assigneeUserId?: string | null;
  estimate_points?: number | null;
  estimatePoints?: number | null;
  due_at?: string | null;
  dueAt?: string | null;
  sort_order?: number;
  sortOrder?: number;
  tags?: string[];
  is_planning_task?: boolean;
  isPlanningTask?: boolean;
  creator_user_id?: string | null;
  creatorUserId?: string | null;
  creator_username?: string | null;
  creatorUsername?: string | null;
  creator_display_name?: string | null;
  creatorDisplayName?: string | null;
  owner_user_id?: string | null;
  ownerUserId?: string | null;
  owner_username?: string | null;
  ownerUsername?: string | null;
  owner_display_name?: string | null;
  ownerDisplayName?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  archived_at?: string | null;
  archivedAt?: string | null;
}

export interface ProjectDependencyGraphNodeResponse {
  id: string;
  node_type?: string;
  nodeType?: string;
  label?: string;
  status?: string;
  parent_id?: string | null;
  parentId?: string | null;
  raw_id?: string;
  rawId?: string;
}

export interface ProjectDependencyGraphEdgeResponse {
  from: string;
  to: string;
  edge_type?: string;
  edgeType?: string;
}

export interface ProjectDependencyGraphResponse {
  root_id?: string | null;
  rootId?: string | null;
  nodes?: ProjectDependencyGraphNodeResponse[];
  edges?: ProjectDependencyGraphEdgeResponse[];
  blocked_by?: ProjectDependencyGraphNodeResponse[];
  blockedBy?: ProjectDependencyGraphNodeResponse[];
  ready?: boolean;
}

export interface ProjectWorkItemCountsResponse {
  total?: number;
  open?: number;
  done?: number;
  blocked?: number;
  by_status?: Record<string, number>;
  byStatus?: Record<string, number>;
}

export interface ProjectPlanResponse {
  project_id?: string;
  projectId?: string;
  requirements?: ProjectRequirementResponse[];
  work_items?: ProjectWorkItemResponse[];
  workItems?: ProjectWorkItemResponse[];
  work_item_counts?: ProjectWorkItemCountsResponse;
  workItemCounts?: ProjectWorkItemCountsResponse;
  dependency_graph?: ProjectDependencyGraphResponse;
  dependencyGraph?: ProjectDependencyGraphResponse;
}

export interface ProjectPlanOptions {
  includeArchived?: boolean;
  includeWorkItems?: boolean;
}

export interface ProjectRequirementWorkItemsOptions {
  includeArchived?: boolean;
  includeDependencyGraph?: boolean;
}

export interface ProjectRequirementWorkItemsResponse {
  work_items?: ProjectWorkItemResponse[];
  workItems?: ProjectWorkItemResponse[];
  dependency_graph?: ProjectDependencyGraphResponse;
  dependencyGraph?: ProjectDependencyGraphResponse;
}
