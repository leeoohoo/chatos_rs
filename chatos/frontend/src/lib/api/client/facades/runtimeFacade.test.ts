// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { runtimeFacade } from './runtimeFacade';

describe('runtimeFacade cloud orchestration', () => {
  it('passes the full tool scope through the cloud tools endpoint', async () => {
    const request = vi.fn().mockResolvedValue({ data: [] });
    const context = { getRequestFn: () => request };

    await runtimeFacade.getAgentTools.call(context as never, {
      conversationId: 'conv-123',
      mcpEnabled: false,
      enabledMcpIds: ['builtin_notepad', 'builtin_task_manager'],
      projectId: 'proj-1',
      projectRoot: '/tmp/workspace',
      contactAgentId: 'agent-9',
      skillsEnabled: true,
      selectedSkillIds: ['skill-a', 'skill-b'],
    });

    expect(request.mock.calls[0][0]).toContain('/agent/tools?');
    expect(request.mock.calls[0][0]).toContain('conversation_id=conv-123');
    expect(request.mock.calls[0][0]).toContain('selected_skill_ids=skill-a%2Cskill-b');
  });

  it('posts guidance and stop commands to the cloud Agent runtime', async () => {
    const request = vi.fn().mockResolvedValue({ accepted: true });
    const context = { getRequestFn: () => request };

    await runtimeFacade.sendRuntimeGuidance.call(
      context as never,
      'conv-123',
      'turn-1',
      'please keep going',
      [],
    );
    await runtimeFacade.stopChat.call(context as never, 'conv-123', 'turn-1');

    expect(request).toHaveBeenNthCalledWith(1, '/agent/chat/guidance', {
      method: 'POST',
      body: JSON.stringify({
        conversation_id: 'conv-123',
        turn_id: 'turn-1',
        content: 'please keep going',
        attachments: [],
      }),
    });
    expect(request).toHaveBeenNthCalledWith(2, '/agent/chat/stop', {
      method: 'POST',
      body: JSON.stringify({ conversation_id: 'conv-123', turn_id: 'turn-1' }),
    });
  });

  it('routes Ask User, Task Manager, and summaries through cloud APIs', async () => {
    const request = vi.fn().mockResolvedValue({ prompts: [], tasks: [], items: [] });
    const context = { getRequestFn: () => request };

    await runtimeFacade.listAskUserPrompts.call(context as never, 'conv-1', { limit: 10 });
    await runtimeFacade.getTaskManagerTasks.call(context as never, 'conv-1', { includeDone: true });
    await runtimeFacade.getConversationSummaries.call(context as never, 'conv-1', { limit: 20 });

    expect(request.mock.calls[0][0]).toContain('/ask-user-prompts?conversation_id=conv-1');
    expect(request.mock.calls[1][0]).toContain('/task-manager/tasks?conversation_id=conv-1');
    expect(request.mock.calls[2][0]).toBe('/conversations/conv-1/summaries?limit=20');
  });
});
