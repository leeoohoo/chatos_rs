// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ContactRecord } from '../../types';
import type { ContactResponse } from '../api/client/types';
import {
  asRecord,
  readBooleanFirst,
  normalizeDate,
  readString,
  readTrimmedString,
} from './normalizerUtils';

export const normalizeContact = (raw: ContactResponse | unknown): ContactRecord | null => {
  const record = asRecord(raw);
  if (!record) {
    return null;
  }
  const id = readTrimmedString(record, 'id');
  const agentId = readTrimmedString(record, 'agent_id');
  if (!id || !agentId) {
    return null;
  }
  const statusValue = readString(record, 'status');
  const status = statusValue
    ? statusValue.toLowerCase().trim()
    : 'active';
  const name = readTrimmedString(record, 'agent_name_snapshot')
    ? readTrimmedString(record, 'agent_name_snapshot')
    : '联系人';
  const createdAt = normalizeDate(record.created_at);
  const updatedAt = normalizeDate(record.updated_at);
  return {
    id,
    agentId,
    name,
    status,
    taskRunner: {
      enabled: readBooleanFirst(record, ['task_runner_enabled'], false),
      baseUrl: readTrimmedString(record, 'task_runner_base_url'),
      agentAccountId: readTrimmedString(record, 'task_runner_agent_account_id'),
      username: readTrimmedString(record, 'task_runner_username'),
      hasPassword: readBooleanFirst(record, ['task_runner_has_password'], false),
    },
    createdAt,
    updatedAt,
  };
};
