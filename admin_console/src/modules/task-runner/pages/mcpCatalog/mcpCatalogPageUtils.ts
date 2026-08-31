// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { McpServerToolProfileInfo } from '../../types';
import type { TranslateFn } from '../../i18n/I18nProvider';

export const MCP_CARD_STYLE = {
  width: '100%',
  padding: 16,
  borderRadius: 6,
  background: '#fff',
  border: '1px solid #f0f0f0',
};

export const TOOL_PROFILE_COLORS: Record<string, string> = {
  admin_full: 'volcano',
  agent_default: 'blue',
  chatos_async_planner: 'geekblue',
};

export function profileLabel(
  profile: McpServerToolProfileInfo,
  t: TranslateFn,
): string {
  if (profile.key === 'admin_full') {
    return t('mcpCatalog.profile.adminFull');
  }
  if (profile.key === 'agent_default') {
    return t('mcpCatalog.profile.agentDefault');
  }
  if (profile.key === 'chatos_async_planner') {
    return t('mcpCatalog.profile.chatosAsyncPlanner');
  }
  return profile.label;
}

export function profileDescription(
  profile: McpServerToolProfileInfo,
  t: TranslateFn,
): string {
  if (profile.key === 'admin_full') {
    return t('mcpCatalog.profile.adminFullDescription');
  }
  if (profile.key === 'agent_default') {
    return t('mcpCatalog.profile.agentDefaultDescription');
  }
  if (profile.key === 'chatos_async_planner') {
    return t('mcpCatalog.profile.chatosAsyncPlannerDescription');
  }
  return profile.description;
}
