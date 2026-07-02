// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  buildChatRequestLogPayload,
  resolveEffectivePlanMode,
} from './requestPayload';

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

  it('keeps plan mode off when the saved session setting is disabled', () => {
    expect(resolveEffectivePlanMode({
      projectId: 'project-1',
      planModeEnabled: false,
    })).toBe(false);
  });

  it('requires a concrete project and saved session setting to enable plan mode', () => {
    expect(resolveEffectivePlanMode({
      projectId: 'project-1',
      planModeEnabled: true,
    })).toBe(true);

    expect(resolveEffectivePlanMode({
      projectId: '0',
      planModeEnabled: true,
    })).toBe(false);
  });
});
