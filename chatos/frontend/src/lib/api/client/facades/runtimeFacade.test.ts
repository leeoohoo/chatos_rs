// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { runtimeFacade } from './runtimeFacade';

describe('runtimeFacade.sendChatCommand', () => {
  it('routes local sessions to the desktop runtime without creating a cloud stream request', async () => {
    const sendChatCommand = vi.fn().mockResolvedValue({ accepted: true });
    const getStreamApiContext = vi.fn(() => {
      throw new Error('cloud stream should not be used');
    });
    const context = {
      getLocalRuntimeClient: () => ({ sendChatCommand }),
      getStreamApiContext,
    };
    const model = { id: 'model-1', provider: 'openai', model_name: 'gpt-test' };

    await runtimeFacade.sendChatCommand.call(
      context as never,
      'lc_session_123',
      'hello',
      model,
      'user-1',
      [],
      false,
      { turnId: 'turn-1' },
    );

    expect(sendChatCommand).toHaveBeenCalledWith(
      'lc_session_123',
      'hello',
      model,
      [],
      false,
      { turnId: 'turn-1' },
    );
    expect(getStreamApiContext).not.toHaveBeenCalled();
  });
});

describe('runtimeFacade.getAgentTools', () => {
  it('loads local session tools from the desktop runtime', async () => {
    const getAgentTools = vi.fn().mockResolvedValue({ tools: [] });
    const context = {
      getLocalRuntimeClient: () => ({ getAgentTools }),
      getRequestFn: () => {
        throw new Error('cloud tools endpoint should not be used');
      },
    };

    await runtimeFacade.getAgentTools.call(context as never, {
      conversationId: 'lc_session_123',
    });

    expect(getAgentTools).toHaveBeenCalledWith('lc_session_123');
  });

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

describe('runtimeFacade local Ask User routing', () => {
  it('routes local prompt reads and mutations to the desktop runtime', async () => {
    const listAskUserPrompts = vi.fn().mockResolvedValue({ prompts: [] });
    const submitAskUserPrompt = vi.fn().mockResolvedValue({ success: true });
    const cancelAskUserPrompt = vi.fn().mockResolvedValue({ success: true });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud Ask User endpoint should not be used');
    });
    const context = {
      getLocalRuntimeClient: () => ({
        listAskUserPrompts,
        submitAskUserPrompt,
        cancelAskUserPrompt,
      }),
      getRequestFn: () => cloudRequest,
    };
    const payload = { conversation_id: 'lc_session_ask', selection: 'yes' };

    await runtimeFacade.listAskUserPrompts.call(
      context as never,
      'lc_session_ask',
      { includePending: true, limit: 10 },
    );
    await runtimeFacade.submitAskUserPrompt.call(context as never, 'up_1', payload);
    await runtimeFacade.cancelAskUserPrompt.call(context as never, 'up_2', payload);

    expect(listAskUserPrompts).toHaveBeenCalledWith(
      'lc_session_ask',
      { includePending: true, limit: 10 },
    );
    expect(submitAskUserPrompt).toHaveBeenCalledWith('up_1', payload);
    expect(cancelAskUserPrompt).toHaveBeenCalledWith('up_2', payload);
    expect(cloudRequest).not.toHaveBeenCalled();
  });
});

describe('runtimeFacade local Task Manager routing', () => {
  it('routes task reads and mutations to SQLite runtime APIs', async () => {
    const getTaskManagerTasks = vi.fn().mockResolvedValue([]);
    const updateTaskManagerTask = vi.fn().mockResolvedValue({ id: 'task-1' });
    const completeTaskManagerTask = vi.fn().mockResolvedValue({ id: 'task-1' });
    const deleteTaskManagerTask = vi.fn().mockResolvedValue({ success: true });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud task endpoint should not be used');
    });
    const context = {
      getLocalRuntimeClient: () => ({
        getTaskManagerTasks,
        updateTaskManagerTask,
        completeTaskManagerTask,
        deleteTaskManagerTask,
      }),
      getRequestFn: () => cloudRequest,
    };

    await runtimeFacade.getTaskManagerTasks.call(context as never, 'lc_session_task', {});
    await runtimeFacade.updateTaskManagerTask.call(
      context as never, 'lc_session_task', 'task-1', { status: 'doing' },
    );
    await runtimeFacade.completeTaskManagerTask.call(
      context as never, 'lc_session_task', 'task-1', { outcome_summary: 'done' },
    );
    await runtimeFacade.deleteTaskManagerTask.call(
      context as never, 'lc_session_task', 'task-1',
    );

    expect(getTaskManagerTasks).toHaveBeenCalledWith('lc_session_task', {});
    expect(updateTaskManagerTask).toHaveBeenCalledWith(
      'lc_session_task', 'task-1', { status: 'doing' },
    );
    expect(completeTaskManagerTask).toHaveBeenCalledWith(
      'lc_session_task', 'task-1', { outcome_summary: 'done' },
    );
    expect(deleteTaskManagerTask).toHaveBeenCalledWith('lc_session_task', 'task-1');
    expect(cloudRequest).not.toHaveBeenCalled();
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

  it('routes local-session guidance to the desktop runtime', async () => {
    const sendRuntimeGuidance = vi.fn().mockResolvedValue({ accepted: true });
    const request = vi.fn(() => {
      throw new Error('cloud guidance endpoint should not be used');
    });
    const context = {
      getLocalRuntimeClient: () => ({ sendRuntimeGuidance }),
      getRequestFn: () => request,
    };

    await runtimeFacade.sendRuntimeGuidance.call(
      context as never,
      'lc_session_123',
      'turn-1',
      'continue',
      [],
    );
    expect(sendRuntimeGuidance).toHaveBeenCalledWith(
      'lc_session_123',
      'turn-1',
      'continue',
      [],
    );
    expect(request).not.toHaveBeenCalled();
  });
});

describe('runtimeFacade.stopChat', () => {
  it('routes local stop requests to the desktop runtime', async () => {
    const stopChat = vi.fn().mockResolvedValue({ success: true });
    const context = {
      getLocalRuntimeClient: () => ({ stopChat }),
      getRequestFn: () => {
        throw new Error('cloud stop endpoint should not be used');
      },
    };

    await runtimeFacade.stopChat.call(context as never, 'lc_session_123', 'turn-1');

    expect(stopChat).toHaveBeenCalledWith('lc_session_123', 'turn-1');
  });
});

describe('runtimeFacade local memory routing', () => {
  it('routes local summary reads and review generation to the desktop runtime', async () => {
    const getConversationSummaries = vi.fn().mockResolvedValue({
      items: [],
      total: 0,
      has_summary: false,
    });
    const runConversationReviewRepair = vi.fn().mockResolvedValue({ success: true });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud memory endpoint should not be used');
    });
    const context = {
      getLocalRuntimeClient: () => ({
        getConversationSummaries,
        runConversationReviewRepair,
      }),
      getRequestFn: () => cloudRequest,
    };

    await runtimeFacade.getConversationSummaries.call(
      context as never,
      'lc_session_memory',
      { limit: 20, offset: 0 },
    );
    await runtimeFacade.runConversationReviewRepair.call(
      context as never,
      'lc_session_memory',
    );

    expect(getConversationSummaries).toHaveBeenCalledWith(
      'lc_session_memory',
      { limit: 20, offset: 0 },
    );
    expect(runConversationReviewRepair).toHaveBeenCalledWith('lc_session_memory');
    expect(cloudRequest).not.toHaveBeenCalled();
  });

  it('routes local summary mutations and status reads to the desktop runtime', async () => {
    const deleteConversationSummary = vi.fn().mockResolvedValue({ success: true });
    const clearConversationSummaries = vi.fn().mockResolvedValue({ success: true });
    const getConversationReviewRepairStatus = vi.fn().mockResolvedValue({ success: true });
    const getConversationMemoryRecalls = vi.fn().mockResolvedValue([]);
    const deleteConversationMemoryRecall = vi.fn().mockResolvedValue({
      success: true,
      deleted_recalls: 2,
    });
    const context = {
      getLocalRuntimeClient: () => ({
        deleteConversationSummary,
        clearConversationSummaries,
        getConversationReviewRepairStatus,
        getConversationMemoryRecalls,
        deleteConversationMemoryRecall,
      }),
    };

    await runtimeFacade.deleteConversationSummary.call(
      context as never,
      'lc_session_memory',
      'lc_summary_1',
    );
    await runtimeFacade.clearConversationSummaries.call(
      context as never,
      'lc_session_memory',
    );
    await runtimeFacade.getConversationReviewRepairStatus.call(
      context as never,
      'lc_session_memory',
    );
    await runtimeFacade.getConversationMemoryRecalls.call(
      context as never,
      'lc_session_memory',
      { limit: 8 },
    );
    await runtimeFacade.deleteConversationMemoryRecall.call(
      context as never,
      'lc_session_memory',
      'lc_recall_1',
    );

    expect(deleteConversationSummary).toHaveBeenCalledWith(
      'lc_session_memory',
      'lc_summary_1',
    );
    expect(clearConversationSummaries).toHaveBeenCalledWith('lc_session_memory');
    expect(getConversationReviewRepairStatus).toHaveBeenCalledWith('lc_session_memory');
    expect(getConversationMemoryRecalls).toHaveBeenCalledWith(
      'lc_session_memory',
      { limit: 8 },
    );
    expect(deleteConversationMemoryRecall).toHaveBeenCalledWith(
      'lc_session_memory',
      'lc_recall_1',
    );
  });
});
