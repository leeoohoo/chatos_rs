// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveWorkspaceResourceVisibility } from './resourceVisibility';

describe('workspace resource visibility', () => {
  it('keeps remote resources visible on the cloud surface', () => {
    expect(resolveWorkspaceResourceVisibility({
      terminalUiEnabled: true,
      terminalUiResolved: true,
    })).toEqual({
      showTerminalSection: true,
      showRemoteSection: true,
    });
  });

  it('respects the terminal UI setting without depending on the desktop bridge', () => {
    expect(resolveWorkspaceResourceVisibility({
      terminalUiEnabled: true,
      terminalUiResolved: true,
    })).toEqual({
      showTerminalSection: true,
      showRemoteSection: true,
    });
    expect(resolveWorkspaceResourceVisibility({
      terminalUiEnabled: false,
      terminalUiResolved: true,
    })).toEqual({
      showTerminalSection: false,
      showRemoteSection: true,
    });
  });
});
