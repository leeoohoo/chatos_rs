// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export const createInternalId = (prefix: string) => {
  const randomPart =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID().replace(/-/g, '')
      : Date.now().toString() + '_' + Math.random().toString(36).slice(2, 10);
  return prefix + '_' + randomPart;
};
