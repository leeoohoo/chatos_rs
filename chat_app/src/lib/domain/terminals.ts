// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Terminal, TerminalLog } from '../../types';
import type { TerminalLogResponse, TerminalResponse } from '../api/client/types';
import {
  asRecord,
  normalizeDate,
  readValue,
} from './normalizerUtils';

export const normalizeTerminal = (raw: TerminalResponse | unknown): Terminal => {
  const record = asRecord(raw);
  const createdAtSource = readValue(record, 'created_at') ?? readValue(record, 'createdAt') ?? Date.now();
  const updatedAtSource = readValue(record, 'updated_at')
    ?? readValue(record, 'updatedAt')
    ?? createdAtSource;
  const lastActiveAtSource = readValue(record, 'last_active_at')
    ?? readValue(record, 'lastActiveAt')
    ?? updatedAtSource;

  return {
    id: (readValue(record, 'id') ?? '') as Terminal['id'],
    name: (readValue(record, 'name') ?? '') as Terminal['name'],
    cwd: (readValue(record, 'cwd') ?? '') as Terminal['cwd'],
    kind: (readValue(record, 'kind') ?? null) as Terminal['kind'],
    userId: (readValue(record, 'user_id') ?? readValue(record, 'userId') ?? null) as Terminal['userId'],
    projectId: (readValue(record, 'project_id') ?? readValue(record, 'projectId') ?? null) as Terminal['projectId'],
    status: (readValue(record, 'status') ?? 'running') as Terminal['status'],
    busy: (readValue(record, 'busy') ?? false) as Terminal['busy'],
    createdAt: normalizeDate(createdAtSource),
    updatedAt: normalizeDate(updatedAtSource),
    lastActiveAt: normalizeDate(lastActiveAtSource),
  };
};

export const normalizeTerminalLog = (raw: TerminalLogResponse | unknown): TerminalLog => {
  const record = asRecord(raw);

  return {
    id: (readValue(record, 'id') ?? '') as TerminalLog['id'],
    terminalId: (readValue(record, 'terminal_id') ?? readValue(record, 'terminalId') ?? '') as TerminalLog['terminalId'],
    logType: (readValue(record, 'log_type') ?? readValue(record, 'logType') ?? readValue(record, 'type') ?? '') as TerminalLog['logType'],
    content: (readValue(record, 'content') ?? '') as TerminalLog['content'],
    createdAt: (readValue(record, 'created_at') ?? readValue(record, 'createdAt') ?? '') as TerminalLog['createdAt'],
  };
};
