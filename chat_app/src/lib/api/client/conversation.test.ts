import { describe, expect, it, vi } from 'vitest';

import { getMcpServers } from './conversation';

describe('conversation api client helpers', () => {
  it('loads enabled mcp servers from config service', async () => {
    const request = vi.fn().mockResolvedValue([
      {
        id: 'cfg-1',
        name: 'remote-shell',
        command: 'npx remote-shell',
        enabled: true,
      },
      {
        id: 'cfg-2',
        name: 'disabled-shell',
        command: 'npx disabled-shell',
        enabled: false,
      },
    ]);

    const response = await getMcpServers(request as never, '  conv-1  ');

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith('/mcp-configs');
    expect(response).toEqual({
      data: {
        mcp_servers: [
          {
            name: 'remote-shell',
            url: 'npx remote-shell',
          },
        ],
      },
    });
  });
});
