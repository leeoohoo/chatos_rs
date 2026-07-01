// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  FsMoveResponse,
} from '../../lib/api/client/types';

export const readProjectTreeErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

export const readProjectTreeMovedPath = (result: FsMoveResponse): string => {
  if (typeof result.to_path === 'string') {
    return result.to_path.trim();
  }
  if (typeof result.toPath === 'string') {
    return result.toPath.trim();
  }
  return '';
};
