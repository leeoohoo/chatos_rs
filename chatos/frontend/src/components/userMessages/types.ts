// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../types';

export interface UserMessageTaskState {
  hasTask: boolean;
  running: boolean;
  label: string | null;
  runningCount: number;
}

export interface UserMessageTurn {
  turnId: string;
  userMessage: Message;
  finalAssistantMessage: Message | null;
  hasProcess: boolean;
  toolCallCount: number;
  thinkingCount: number;
  processMessageCount: number;
  taskState: UserMessageTaskState;
}
