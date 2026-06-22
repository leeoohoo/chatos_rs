import { describe, expect, it, vi } from 'vitest';

import { getConversationUserMessageTurns } from './sessions';

describe('workspace session api helpers', () => {
  it('builds the user-message turns query with paging params', async () => {
    const request = vi.fn().mockResolvedValue({
      items: [],
      has_more: false,
      next_before: null,
    });

    await getConversationUserMessageTurns(request as never, 'conv-1', {
      limit: 10,
      before: 'turn-9',
    });

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith(
      '/conversations/conv-1/user-message-turns?limit=10&before=turn-9',
    );
  });
});
