// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../types';
import type { ProjectResponse } from '../api/client/types';
import {
  asRecord,
  normalizeDate,
  readValue,
} from './normalizerUtils';

export const normalizeProject = (raw: ProjectResponse | unknown): Project => {
  const record = asRecord(raw);
  const createdAtSource = readValue(record, 'created_at') ?? readValue(record, 'createdAt') ?? Date.now();
  const updatedAtSource = readValue(record, 'updated_at')
    ?? readValue(record, 'updatedAt')
    ?? createdAtSource;

  return {
    id: (readValue(record, 'id') ?? '') as Project['id'],
    name: (readValue(record, 'name') ?? '') as Project['name'],
    rootPath: (readValue(record, 'root_path') ?? readValue(record, 'rootPath') ?? '') as Project['rootPath'],
    displayRootPath: (readValue(record, 'display_root_path') ?? readValue(record, 'displayRootPath') ?? null) as Project['displayRootPath'],
    gitUrl: (readValue(record, 'git_url') ?? readValue(record, 'gitUrl') ?? null) as Project['gitUrl'],
    sourceType: (readValue(record, 'source_type') ?? readValue(record, 'sourceType') ?? 'cloud') as Project['sourceType'],
    executionPlane: (readValue(record, 'execution_plane') ?? readValue(record, 'executionPlane') ?? null) as Project['executionPlane'],
    cloudImportSource: (readValue(record, 'cloud_import_source') ?? readValue(record, 'cloudImportSource') ?? null) as Project['cloudImportSource'],
    importStatus: (readValue(record, 'import_status') ?? readValue(record, 'importStatus') ?? null) as Project['importStatus'],
    sourceGitUrl: (readValue(record, 'source_git_url') ?? readValue(record, 'sourceGitUrl') ?? null) as Project['sourceGitUrl'],
    harnessSpaceIdentifier: (readValue(record, 'harness_space_identifier') ?? readValue(record, 'harnessSpaceIdentifier') ?? null) as Project['harnessSpaceIdentifier'],
    harnessRepoIdentifier: (readValue(record, 'harness_repo_identifier') ?? readValue(record, 'harnessRepoIdentifier') ?? null) as Project['harnessRepoIdentifier'],
    harnessRepoPath: (readValue(record, 'harness_repo_path') ?? readValue(record, 'harnessRepoPath') ?? null) as Project['harnessRepoPath'],
    harnessGitUrl: (readValue(record, 'harness_git_url') ?? readValue(record, 'harnessGitUrl') ?? null) as Project['harnessGitUrl'],
    harnessGitSshUrl: (readValue(record, 'harness_git_ssh_url') ?? readValue(record, 'harnessGitSshUrl') ?? null) as Project['harnessGitSshUrl'],
    importError: (readValue(record, 'import_error') ?? readValue(record, 'importError') ?? null) as Project['importError'],
    importStartedAt: (readValue(record, 'import_started_at') ?? readValue(record, 'importStartedAt') ?? null) as Project['importStartedAt'],
    importFinishedAt: (readValue(record, 'import_finished_at') ?? readValue(record, 'importFinishedAt') ?? null) as Project['importFinishedAt'],
    description: (readValue(record, 'description') ?? null) as Project['description'],
    userId: (readValue(record, 'user_id') ?? readValue(record, 'userId') ?? null) as Project['userId'],
    latestSessionId: (readValue(record, 'latest_session_id') ?? readValue(record, 'latestSessionId') ?? null) as Project['latestSessionId'],
    lastMessageAt: (readValue(record, 'last_message_at') ?? readValue(record, 'lastMessageAt') ?? null) as Project['lastMessageAt'],
    createdAt: normalizeDate(createdAtSource),
    updatedAt: normalizeDate(updatedAtSource),
  };
};
