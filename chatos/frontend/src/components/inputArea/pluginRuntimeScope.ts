// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PUBLIC_PROJECT_ID } from '../../lib/domain/contactSessions';

export const taskPluginPickerEnabled = (
  conversationId: string | null | undefined,
  projectId: string | null | undefined,
): boolean => {
  const normalizedProjectId = String(projectId || '').trim();
  return Boolean(
    conversationId
    && normalizedProjectId
    && normalizedProjectId !== '0'
    && normalizedProjectId !== PUBLIC_PROJECT_ID,
  );
};
