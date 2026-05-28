import { describe, expect, it, vi } from 'vitest';

import { runtimeFacade } from './runtimeFacade';

describe('runtimeFacade.getAgentV3Tools', () => {
  it('passes conversation id through to the tools query', async () => {
    const request = vi.fn().mockResolvedValue({ data: [] });
    const context = {
      getRequestFn: () => request,
    };

    await runtimeFacade.getAgentV3Tools.call(context as never, {
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
    expect(request.mock.calls[0][0]).toContain('/agent_v3/tools?');
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
