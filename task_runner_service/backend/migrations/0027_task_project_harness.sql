-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE task_projects ADD COLUMN source_type TEXT;
ALTER TABLE task_projects ADD COLUMN cloud_import_source TEXT;
ALTER TABLE task_projects ADD COLUMN import_status TEXT;
ALTER TABLE task_projects ADD COLUMN source_git_url TEXT;
ALTER TABLE task_projects ADD COLUMN harness_space_identifier TEXT;
ALTER TABLE task_projects ADD COLUMN harness_repo_identifier TEXT;
ALTER TABLE task_projects ADD COLUMN harness_repo_path TEXT;
ALTER TABLE task_projects ADD COLUMN harness_git_url TEXT;
ALTER TABLE task_projects ADD COLUMN harness_git_ssh_url TEXT;
ALTER TABLE task_projects ADD COLUMN harness_default_branch TEXT;
ALTER TABLE task_projects ADD COLUMN harness_provision_status TEXT;
ALTER TABLE task_projects ADD COLUMN harness_provision_error TEXT;
ALTER TABLE task_projects ADD COLUMN harness_provisioned_at TEXT;
