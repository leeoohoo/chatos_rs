-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  name TEXT NOT NULL,
  root_path TEXT,
  git_url TEXT,
  source_type TEXT NOT NULL DEFAULT 'local',
  cloud_import_source TEXT NOT NULL DEFAULT 'none',
  import_status TEXT NOT NULL DEFAULT 'none',
  source_git_url TEXT,
  harness_space_identifier TEXT,
  harness_repo_identifier TEXT,
  harness_repo_path TEXT,
  harness_git_url TEXT,
  harness_git_ssh_url TEXT,
  harness_default_branch TEXT,
  harness_provision_status TEXT NOT NULL DEFAULT 'pending',
  harness_provision_error TEXT,
  harness_provisioned_at TEXT,
  import_error TEXT,
  import_started_at TEXT,
  import_finished_at TEXT,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_projects_owner_user_id
ON projects(owner_user_id);

CREATE INDEX IF NOT EXISTS idx_projects_status
ON projects(status);

CREATE TABLE IF NOT EXISTS project_runtime_environments (
  project_id TEXT PRIMARY KEY,
  status TEXT NOT NULL DEFAULT 'pending',
  sandbox_enabled INTEGER NOT NULL DEFAULT 1,
  sandbox_provider TEXT NOT NULL DEFAULT 'none',
  file_provider TEXT NOT NULL DEFAULT 'none',
  analysis_summary TEXT,
  not_runnable_reason TEXT,
  detected_stack_json TEXT NOT NULL DEFAULT '{}',
  required_services_json TEXT NOT NULL DEFAULT '[]',
  env_vars_json TEXT NOT NULL DEFAULT '{}',
  last_agent_run_id TEXT,
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS project_runtime_environment_images (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  environment_key TEXT NOT NULL,
  environment_type TEXT NOT NULL,
  display_name TEXT NOT NULL,
  image_id TEXT,
  image_ref TEXT,
  image_provider TEXT NOT NULL DEFAULT 'none',
  features_json TEXT NOT NULL DEFAULT '[]',
  ports_json TEXT NOT NULL DEFAULT '[]',
  env_vars_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'ready',
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(project_id, environment_key),
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_runtime_environment_images_project_id
ON project_runtime_environment_images(project_id);

CREATE TABLE IF NOT EXISTS project_profiles (
  project_id TEXT PRIMARY KEY,
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  background TEXT,
  introduction TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS requirements (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  parent_requirement_id TEXT,
  requirement_type TEXT NOT NULL DEFAULT 'requirement',
  title TEXT NOT NULL,
  summary TEXT,
  detail TEXT,
  business_value TEXT,
  acceptance_criteria TEXT,
  source TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'draft',
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  assignee_user_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY(parent_requirement_id) REFERENCES requirements(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_requirements_project_id
ON requirements(project_id);

CREATE INDEX IF NOT EXISTS idx_requirements_project_status
ON requirements(project_id, status);

CREATE INDEX IF NOT EXISTS idx_requirements_project_sort
ON requirements(project_id, priority DESC, updated_at DESC, id);

CREATE INDEX IF NOT EXISTS idx_requirements_project_status_sort
ON requirements(project_id, status, priority DESC, updated_at DESC, id);

CREATE TABLE IF NOT EXISTS requirement_dependencies (
  requirement_id TEXT NOT NULL,
  prerequisite_requirement_id TEXT NOT NULL,
  relation_type TEXT NOT NULL DEFAULT 'blocks',
  created_at TEXT NOT NULL,
  PRIMARY KEY(requirement_id, prerequisite_requirement_id),
  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE,
  FOREIGN KEY(prerequisite_requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_requirement_dependencies_requirement_id
ON requirement_dependencies(requirement_id);

CREATE INDEX IF NOT EXISTS idx_requirement_dependencies_prerequisite_id
ON requirement_dependencies(prerequisite_requirement_id);

CREATE TABLE IF NOT EXISTS requirement_documents (
  id TEXT PRIMARY KEY,
  requirement_id TEXT NOT NULL,
  doc_type TEXT NOT NULL DEFAULT 'technical_overview',
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  title TEXT NOT NULL,
  format TEXT NOT NULL DEFAULT 'markdown',
  content TEXT NOT NULL DEFAULT '',
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_requirement_documents_requirement_id
ON requirement_documents(requirement_id);

CREATE INDEX IF NOT EXISTS idx_requirement_documents_requirement_type_sort
ON requirement_documents(requirement_id, doc_type, updated_at DESC, id);

CREATE TABLE IF NOT EXISTS project_work_items (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  requirement_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'todo',
  priority INTEGER NOT NULL DEFAULT 0,
  assignee_user_id TEXT,
  estimate_points INTEGER,
  due_at TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  tags_json TEXT NOT NULL DEFAULT '[]',
  is_planning_task INTEGER NOT NULL DEFAULT 0,
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY(requirement_id) REFERENCES requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_id
ON project_work_items(project_id);

CREATE INDEX IF NOT EXISTS idx_project_work_items_requirement_id
ON project_work_items(requirement_id);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_status
ON project_work_items(project_id, status);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_sort
ON project_work_items(project_id, sort_order ASC, priority DESC, updated_at DESC, id);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_status_sort
ON project_work_items(project_id, status, sort_order ASC, priority DESC, updated_at DESC, id);

CREATE INDEX IF NOT EXISTS idx_project_work_items_requirement_sort
ON project_work_items(requirement_id, sort_order ASC, priority DESC, updated_at DESC, id);

CREATE TABLE IF NOT EXISTS project_work_item_dependencies (
  work_item_id TEXT NOT NULL,
  prerequisite_work_item_id TEXT NOT NULL,
  relation_type TEXT NOT NULL DEFAULT 'blocks',
  created_at TEXT NOT NULL,
  PRIMARY KEY(work_item_id, prerequisite_work_item_id),
  FOREIGN KEY(work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE,
  FOREIGN KEY(prerequisite_work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_work_item_dependencies_work_item_id
ON project_work_item_dependencies(work_item_id);

CREATE INDEX IF NOT EXISTS idx_project_work_item_dependencies_prerequisite_id
ON project_work_item_dependencies(prerequisite_work_item_id);

CREATE TABLE IF NOT EXISTS project_work_item_task_runner_links (
  id TEXT PRIMARY KEY,
  work_item_id TEXT NOT NULL,
  task_runner_task_id TEXT NOT NULL,
  task_runner_run_id TEXT,
  link_type TEXT NOT NULL DEFAULT 'execution',
  execution_group_id TEXT,
  is_current INTEGER NOT NULL DEFAULT 1,
  superseded_at TEXT,
  source_session_id TEXT,
  source_user_message_id TEXT,
  task_runner_status TEXT,
  last_callback_event TEXT,
  last_callback_at TEXT,
  last_error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_work_item_task_runner_links_work_item
ON project_work_item_task_runner_links(work_item_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_project_work_item_task_runner_links_task_id_unique
ON project_work_item_task_runner_links(task_runner_task_id);
