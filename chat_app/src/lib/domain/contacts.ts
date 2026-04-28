import type { ContactRecord } from '../../types';
import type { ContactResponse } from '../api/client/types';
import {
  asRecord,
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
    createdAt,
    updatedAt,
  };
};
