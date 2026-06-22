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
