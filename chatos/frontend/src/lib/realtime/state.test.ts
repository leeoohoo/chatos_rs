// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  applyRealtimeTopicAckSnapshot,
  clearRealtimeTopicAckSnapshot,
  hasRealtimeTopicAckSnapshot,
  waitForRealtimeTopicAckSnapshot,
} from './state';

const topic = { scope: 'conversation' as const, id: 'session-1' };

afterEach(() => {
  clearRealtimeTopicAckSnapshot();
  vi.useRealTimers();
});

describe('realtime topic acknowledgement state', () => {
  it('waits until the conversation subscription is acknowledged', async () => {
    vi.useFakeTimers();
    const waiting = waitForRealtimeTopicAckSnapshot(topic, 5_000);

    applyRealtimeTopicAckSnapshot('subscribe', [topic]);

    await expect(waiting).resolves.toBe(true);
    expect(hasRealtimeTopicAckSnapshot(topic)).toBe(true);
  });

  it('clears acknowledged topics after unsubscribe', () => {
    applyRealtimeTopicAckSnapshot('subscribe', [topic]);
    applyRealtimeTopicAckSnapshot('unsubscribe', [topic]);

    expect(hasRealtimeTopicAckSnapshot(topic)).toBe(false);
  });
});
