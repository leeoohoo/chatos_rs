// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveLocalProjectCreationEnabled } from './useLocalProjectCreationSetting';

describe('resolveLocalProjectCreationEnabled', () => {
  it('defaults closed and accepts the managed configuration value', () => {
    expect(resolveLocalProjectCreationEnabled(undefined)).toBe(false);
    expect(resolveLocalProjectCreationEnabled({
      effective: { LOCAL_PROJECT_CREATION_ENABLED: true },
    })).toBe(true);
    expect(resolveLocalProjectCreationEnabled({
      settings: { LOCAL_PROJECT_CREATION_ENABLED: 'off' },
    })).toBe(false);
  });
});
