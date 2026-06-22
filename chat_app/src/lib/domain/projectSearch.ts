import type { FsContentSearchEntryResponse } from '../api/client/types';
import type { ProjectSearchHit } from '../../types';
import {
  asRecord,
  readNumberFirst,
  readString,
  readStringFirst,
} from './normalizerUtils';

export const normalizeProjectSearchHit = (
  raw: FsContentSearchEntryResponse | unknown,
): ProjectSearchHit => {
  const record = asRecord(raw);
  const path = readString(record, 'path');
  return {
    path,
    relativePath: readStringFirst(record, ['relative_path', 'relativePath'], path),
    line: readNumberFirst(record, ['line'], 1),
    column: readNumberFirst(record, ['column'], 1),
    text: readString(record, 'text'),
  };
};

export const buildProjectSearchHitId = (hit: ProjectSearchHit): string => (
  `${hit.path}:${hit.line}:${hit.column}`
);
