// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { GitActionResult } from '../../../types';

export const actionOutputMessage = (result: GitActionResult, fallback: string): string => (
  result.stdout || result.stderr || fallback
);

export const actionErrorMessage = (result: GitActionResult, fallback: string): string => (
  result.stderr || result.stdout || fallback
);

export const normalizeNonEmptyPaths = (paths: string[]): string[] => (
  paths.map((path) => path.trim()).filter(Boolean)
);
