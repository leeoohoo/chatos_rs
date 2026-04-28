import { describe, expect, it } from 'vitest';

import {
  extractTaskBoardUpdatedEvent,
  extractTaskReviewPanelFromToolStream,
  extractUiPromptPanelFromToolStream,
} from './toolStreamPanels';

describe('domain/toolStreamPanels', () => {
  it('normalizes task review panels from tool stream payloads', () => {
    const panel = extractTaskReviewPanelFromToolStream({
      content: JSON.stringify({
        event: 'task_create_review_required',
        data: {
          review_id: 'review_1',
          conversation_id: 'session_from_stream',
          conversation_turn_id: 'turn_from_stream',
          timeout_ms: 30000,
          draft_tasks: [
            {
              title: '  Fix runner  ',
              description: '  inspect logs  ',
              priority: 'high',
              status: 'doing',
              tags: ['runner', 'runner', 'ops'],
            },
          ],
        },
      }),
    }, 'fallback_session', 'fallback_turn');

    expect(panel).toMatchObject({
      reviewId: 'review_1',
      sessionId: 'session_from_stream',
      conversationTurnId: 'turn_from_stream',
      timeoutMs: 30000,
      submitting: false,
      error: null,
    });
    expect(panel?.drafts).toHaveLength(1);
    expect(panel?.drafts[0]).toMatchObject({
      title: 'Fix runner',
      details: 'inspect logs',
      priority: 'high',
      status: 'doing',
      tags: ['runner', 'ops'],
    });
    expect(panel?.drafts[0]?.id.startsWith('draft1_')).toBe(true);
  });

  it('normalizes ui prompt panels from mixed payload shapes', () => {
    const panel = extractUiPromptPanelFromToolStream({
      tool_call_id: 'tool_123',
      content: JSON.stringify({
        event: 'ui_prompt_required',
        data: {
          prompt_id: 'prompt_1',
          kind: 'mixed',
          title: 'Need input',
          message: 'Provide values',
          allow_cancel: false,
          payload: {
            fields: [
              { key: 'token', label: 'Token', required: true, secret: true },
            ],
            choice: {
              multiple: true,
              options: [
                { value: 'a', label: 'Option A' },
                { value: 'b', label: 'Option B' },
              ],
              min_selections: 1,
              max_selections: 2,
            },
          },
        },
      }),
    }, 'fallback_session', 'fallback_turn');

    expect(panel).toMatchObject({
      promptId: 'prompt_1',
      sessionId: 'fallback_session',
      conversationTurnId: 'fallback_turn',
      toolCallId: 'tool_123',
      kind: 'mixed',
      title: 'Need input',
      message: 'Provide values',
      allowCancel: false,
      submitting: false,
      error: null,
    });
    expect(panel?.payload.fields).toEqual([
      {
        key: 'token',
        label: 'Token',
        description: '',
        placeholder: '',
        default: '',
        required: true,
        multiline: false,
        secret: true,
      },
    ]);
    expect(panel?.payload.choice).toMatchObject({
      multiple: true,
      min_selections: 1,
      max_selections: 2,
    });
  });

  it('extracts task board refresh events', () => {
    expect(extractTaskBoardUpdatedEvent({
      content: JSON.stringify({
        event: 'task_board_updated',
        data: {
          conversation_id: 'session_1',
          conversation_turn_id: 'turn_1',
          task_board: 'updated board',
        },
      }),
    })).toEqual({
      sessionId: 'session_1',
      conversationTurnId: 'turn_1',
      taskBoard: 'updated board',
    });
  });
});
