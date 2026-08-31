// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { AgentPromptVendor } from '../../types';

export const AGENT_PROMPT_VENDORS: AgentPromptVendor[] = ['glm', 'deepseek', 'gpt', 'kimi'];

export function agentPromptVendorLabel(vendor: AgentPromptVendor): string {
  return {
    glm: 'GLM',
    deepseek: 'DeepSeek',
    gpt: 'GPT / OpenAI',
    kimi: 'Kimi / Moonshot',
  }[vendor];
}
