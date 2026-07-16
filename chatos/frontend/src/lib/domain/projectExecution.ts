// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../types';

export type ProjectExecutionPlane = 'cloud' | 'local_connector';

const normalized = (value: string | null | undefined): string =>
  String(value || '').trim().toLowerCase();

export const resolveProjectExecutionPlane = (
  project: Pick<Project, 'executionPlane' | 'sourceType' | 'rootPath'>,
): ProjectExecutionPlane => {
  const explicit = normalized(project.executionPlane);
  if (explicit === 'cloud') {
    return 'cloud';
  }
  if (explicit === 'local_connector') {
    return 'local_connector';
  }

  const sourceType = normalized(project.sourceType);
  if (sourceType === 'cloud') {
    return 'cloud';
  }
  if (sourceType === 'local' || sourceType === 'local_connector') {
    return 'local_connector';
  }

  if (normalized(project.rootPath).startsWith('local://connector/')) {
    return 'local_connector';
  }

  return 'cloud';
};

export const isCloudProject = (
  project: Pick<Project, 'executionPlane' | 'sourceType' | 'rootPath'>,
): boolean => resolveProjectExecutionPlane(project) === 'cloud';
