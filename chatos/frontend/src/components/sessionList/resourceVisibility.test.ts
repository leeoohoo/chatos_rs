// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { resolveWorkspaceResourceVisibility } from './resourceVisibility';

describe('workspace resource visibility', () => {
  it('hides terminal and remote resources in the browser cloud surface', () => {
    expect(resolveWorkspaceResourceVisibility({
      desktopBridgeAvailable: false,
      terminalUiEnabled: true,
      terminalUiResolved: true,
    })).toEqual({
      showTerminalSection: false,
      showRemoteSection: false,
    });
  });

  it('shows desktop resources while respecting the terminal UI setting', () => {
    expect(resolveWorkspaceResourceVisibility({
      desktopBridgeAvailable: true,
      terminalUiEnabled: true,
      terminalUiResolved: true,
    })).toEqual({
      showTerminalSection: true,
      showRemoteSection: true,
    });
    expect(resolveWorkspaceResourceVisibility({
      desktopBridgeAvailable: true,
      terminalUiEnabled: false,
      terminalUiResolved: true,
    })).toEqual({
      showTerminalSection: false,
      showRemoteSection: true,
    });
  });
});
