import { describe, expect, it } from 'vitest';

import { buildChatRequestLogPayload } from './requestPayload';

describe('sendMessage request payload helpers', () => {
  it('summarizes chat request debug logs without embedding full payload content', () => {
    const payload = buildChatRequestLogPayload({
      sessionId: 'session-1',
      turnId: 'turn-1',
      content: 'message '.repeat(80),
      selectedModel: {
        id: 'model-1',
        model_name: 'gpt-test',
        provider: 'openai',
        base_url: 'https://example.test',
        supports_images: true,
        supports_reasoning: true,
      } as never,
      chatConfig: {
        temperature: 0.7,
      } as never,
      systemContext: 'system context '.repeat(80),
      attachments: [{
        name: 'large.txt',
        mimeType: 'text/plain',
        size: 1024,
        type: 'file',
        dataUrl: 'data:text/plain;base64,secret',
        text: 'secret text',
      }],
      reasoningEnabled: true,
      contactAgentId: 'contact-1',
      remoteConnectionId: null,
      projectId: 'project-1',
      projectRoot: '/workspace/project',
      workspaceRoot: '/workspace',
      planMode: true,
    });

    expect(payload.message_preview.length).toBeLessThan(payload.message_chars);
    expect(payload.system_context_preview.length).toBeLessThan(payload.system_context_chars);
    expect(payload.attachment_count).toBe(1);
    expect(payload.attachment_bytes).toBe(1024);
    expect(payload.attachments[0]).toEqual({
      name: 'large.txt',
      mimeType: 'text/plain',
      size: 1024,
      type: 'file',
    });
  });
});
