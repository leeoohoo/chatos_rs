// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface WorkspaceResourceVisibility {
  showTerminalSection: boolean;
  showRemoteSection: boolean;
}

export const resolveWorkspaceResourceVisibility = (input: {
  terminalUiEnabled: boolean;
  terminalUiResolved: boolean;
}): WorkspaceResourceVisibility => ({
  showTerminalSection: input.terminalUiResolved
    && input.terminalUiEnabled,
  showRemoteSection: true,
});
