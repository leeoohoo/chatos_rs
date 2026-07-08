// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { runtimeFacade } from './runtimeFacade';

describe('runtimeFacade.getAgentTools', () => {
  it('passes conversation id through to the tools query', async () => {
    const request = vi.fn().mockResolvedValue({ data: [] });
    const context = {
      getRequestFn: () => request,
    };

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

    expect(request).toHaveBeenCalledTimes(1);
    expect(request.mock.calls[0][0]).toContain('/agent/tools?');
    expect(request.mock.calls[0][0]).toContain('conversation_id=conv-123');
    expect(request.mock.calls[0][0]).toContain('mcp_enabled=false');
    expect(request.mock.calls[0][0]).toContain('enabled_mcp_ids=builtin_notepad%2Cbuiltin_task_manager');
    expect(request.mock.calls[0][0]).toContain('project_id=proj-1');
    expect(request.mock.calls[0][0]).toContain('project_root=%2Ftmp%2Fworkspace');
    expect(request.mock.calls[0][0]).toContain('contact_agent_id=agent-9');
    expect(request.mock.calls[0][0]).toContain('skills_enabled=true');
    expect(request.mock.calls[0][0]).toContain('selected_skill_ids=skill-a%2Cskill-b');
  });
});

describe('runtimeFacade.sendRuntimeGuidance', () => {
  it('posts guidance to the active turn endpoint', async () => {
    const request = vi.fn().mockResolvedValue({ accepted: true });
    const context = {
      getRequestFn: () => request,
    };

    await runtimeFacade.sendRuntimeGuidance.call(
      context as never,
      'conv-123',
      'turn-1',
      'please keep going',
      [{ name: 'note.txt', mimeType: 'text/plain', size: 12, type: 'file', text: 'hello' }],
    );

    expect(request).toHaveBeenCalledTimes(1);
    expect(request.mock.calls[0][0]).toBe('/agent/chat/guidance');
    expect(request.mock.calls[0][1]).toEqual({
      method: 'POST',
      body: JSON.stringify({
        conversation_id: 'conv-123',
        turn_id: 'turn-1',
        content: 'please keep going',
        attachments: [
          { name: 'note.txt', mimeType: 'text/plain', size: 12, type: 'file', text: 'hello' },
        ],
      }),
    });
  });
});
