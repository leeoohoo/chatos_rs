// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { PUBLIC_PROJECT_ID } from '../../lib/domain/contactSessions';
import { taskPluginPickerEnabled } from './pluginRuntimeScope';

describe('Plugin runtime scope', () => {
  it('requires both a conversation and a concrete project', () => {
    expect(taskPluginPickerEnabled('conversation-1', 'project-1')).toBe(true);
    expect(taskPluginPickerEnabled(null, 'project-1')).toBe(false);
    expect(taskPluginPickerEnabled('conversation-1', null)).toBe(false);
    expect(taskPluginPickerEnabled('conversation-1', '0')).toBe(false);
    expect(taskPluginPickerEnabled('conversation-1', PUBLIC_PROJECT_ID)).toBe(false);
  });
});
