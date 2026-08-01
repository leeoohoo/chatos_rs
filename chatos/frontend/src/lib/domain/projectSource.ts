// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../types';

export type ProjectSourceKind = 'cloud' | 'local_connector';

const normalized = (value: string | null | undefined): string =>
  String(value || '').trim().toLowerCase();

export const resolveProjectSourceKind = (
  project: Pick<Project, 'sourceType' | 'rootPath'> | null | undefined,
): ProjectSourceKind => {
  const sourceType = normalized(project?.sourceType);
  if (sourceType === 'cloud') {
    return 'cloud';
  }
  if (sourceType === 'local' || sourceType === 'local_connector') {
    return 'local_connector';
  }

  if (normalized(project?.rootPath).startsWith('local://connector/')) {
    return 'local_connector';
  }

  return 'cloud';
};

export const isCloudProjectSource = (
  project: Pick<Project, 'sourceType' | 'rootPath'> | null | undefined,
): boolean => resolveProjectSourceKind(project) === 'cloud';
