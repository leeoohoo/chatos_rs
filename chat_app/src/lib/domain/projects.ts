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
    gitUrl: (readValue(record, 'git_url') ?? readValue(record, 'gitUrl') ?? null) as Project['gitUrl'],
    description: (readValue(record, 'description') ?? null) as Project['description'],
    userId: (readValue(record, 'user_id') ?? readValue(record, 'userId') ?? null) as Project['userId'],
    latestSessionId: (readValue(record, 'latest_session_id') ?? readValue(record, 'latestSessionId') ?? null) as Project['latestSessionId'],
    lastMessageAt: (readValue(record, 'last_message_at') ?? readValue(record, 'lastMessageAt') ?? null) as Project['lastMessageAt'],
    createdAt: normalizeDate(createdAtSource),
    updatedAt: normalizeDate(updatedAtSource),
  };
};
