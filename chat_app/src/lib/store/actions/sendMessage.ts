import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import type { TaskReviewDraft, TaskReviewPanelState } from '../types';
import { debugLog } from '@/lib/utils';

const TASK_CREATE_REVIEW_REQUIRED_EVENT = 'task_create_review_required';

const createInternalId = (prefix: string) => {
  const randomPart =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID().replace(/-/g, '')
      : Date.now().toString() + '_' + Math.random().toString(36).slice(2, 10);
  return prefix + '_' + randomPart;
};

const normalizeTaskPriority = (value: unknown): TaskReviewDraft['priority'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'high') return 'high';
  if (normalized === 'low') return 'low';
  return 'medium';
};

const normalizeTaskStatus = (value: unknown): TaskReviewDraft['status'] => {
  const normalized = String(value ?? '').trim().toLowerCase();
  if (normalized === 'doing') return 'doing';
  if (normalized === 'blocked') return 'blocked';
  if (normalized === 'done') return 'done';
  return 'todo';
};

const parseTaskTags = (value: unknown): string[] => {
  const source = Array.isArray(value)
    ? value
    : typeof value === 'string'
      ? value.split(',')
      : [];

  const seen = new Set<string>();
  const tags: string[] = [];
  source.forEach((item) => {
    const tag = String(item ?? '').trim();
    if (!tag || seen.has(tag)) {
      return;
    }
    seen.add(tag);
    tags.push(tag);
  });
  return tags;
};

const toTaskReviewDraft = (raw: any, index: number): TaskReviewDraft => {
  const title = String(raw?.title ?? '').trim();
  const details = String(raw?.details ?? raw?.description ?? '').trim();
  const dueRaw = raw?.due_at ?? raw?.dueAt;
  const dueAt = typeof dueRaw === 'string' ? dueRaw.trim() : '';

  return {
    id: typeof raw?.id === 'string' && raw.id.trim() ? raw.id : createInternalId('draft' + (index + 1)),
    title,
    details,
    priority: normalizeTaskPriority(raw?.priority),
    status: normalizeTaskStatus(raw?.status),
    tags: parseTaskTags(raw?.tags),
    dueAt: dueAt || null,
  };
};

const extractTaskReviewPanelFromToolStream = (
  streamPayload: any,
  fallbackSessionId: string,
  fallbackTurnId: string
): TaskReviewPanelState | null => {
  const rawContent = typeof streamPayload?.content === 'string' ? streamPayload.content.trim() : '';
  if (!rawContent) {
    return null;
  }

  let parsedChunk: any = null;
  try {
    parsedChunk = JSON.parse(rawContent);
  } catch (_) {
    return null;
  }

  if (parsedChunk?.event !== TASK_CREATE_REVIEW_REQUIRED_EVENT) {
    return null;
  }

  const payload = parsedChunk?.data ?? {};
  const reviewId = typeof payload?.review_id === 'string' ? payload.review_id.trim() : '';
  if (!reviewId) {
    return null;
  }

  const payloadSessionId = typeof payload?.session_id === 'string' ? payload.session_id.trim() : '';
  const sessionId = payloadSessionId || fallbackSessionId;

  const payloadTurnId = typeof payload?.conversation_turn_id === 'string'
    ? payload.conversation_turn_id.trim()
    : '';
  const conversationTurnId = payloadTurnId || fallbackTurnId;

  const rawDraftTasks = Array.isArray(payload?.draft_tasks) ? payload.draft_tasks : [];
  const drafts = rawDraftTasks.map((task: any, index: number) => toTaskReviewDraft(task, index));

  return {
    reviewId,
    sessionId,
    conversationTurnId,
    drafts,
    timeoutMs: typeof payload?.timeout_ms === 'number' ? payload.timeout_ms : undefined,
    submitting: false,
    error: null,
  };
};

const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

// 工厂函数：创建 sendMessage 处理器，注入依赖以便于在 store 外部维护
export function createSendMessageHandler({
  set,
  get,
  client,
  getUserIdParam,
}: {
  set: (fn: (state: any) => void) => void;
  get: () => any;
  client: ApiClient;
  getUserIdParam: () => string;
}) {
  return async function sendMessage(content: string, attachments: any[] = []) {
    let tempUserId: string | null = null;
    let tempAssistantId: string | null = null;
    const {
      currentSessionId,
      selectedModelId,
      aiModelConfigs,
      chatConfig,
      sessionChatState,
      activeSystemContext,
      selectedAgentId,
      agents,
    } = get();

    if (!currentSessionId) {
      throw new Error('No active session');
    }

    // 检查是否已经在发送消息，防止重复发送
    const chatState = sessionChatState[currentSessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
    if (chatState.isLoading || chatState.isStreaming) {
      debugLog('Message sending already in progress, ignoring duplicate request');
      return;
    }

    // 需要选择模型或智能体之一
    let selectedModel: any = null;
    let selectedAgent: any = null;
    if (selectedAgentId) {
      selectedAgent = agents.find((a: any) => a.id === selectedAgentId);
      if (!selectedAgent || selectedAgent.enabled === false) {
        throw new Error('选择的智能体不可用');
      }
    } else if (selectedModelId) {
      selectedModel = aiModelConfigs.find((model: any) => model.id === selectedModelId);
      if (!selectedModel || !selectedModel.enabled) {
        throw new Error('选择的模型不可用');
      }
    } else {
      throw new Error('请先选择一个模型或智能体');
    }

    const conversationTurnId = createInternalId('turn');

    try {
      const activeModelConfig = selectedAgent
        ? aiModelConfigs.find((model: any) => model.id === selectedAgent.ai_model_config_id)
        : selectedModel;
      const supportsImages = activeModelConfig?.supports_images === true;
      const supportsReasoning = activeModelConfig?.supports_reasoning === true || !!activeModelConfig?.thinking_level;
      const reasoningEnabled = supportsReasoning && (chatConfig?.reasoningEnabled === true || !!activeModelConfig?.thinking_level);
      const safeAttachments = Array.isArray(attachments)
        ? (supportsImages ? attachments : attachments.filter((f: any) => !(f && typeof f.type === 'string' && f.type.startsWith('image/'))))
        : [];

      // 预处理附件：生成前端展示对象和发送给后端的精简对象
      const readFileAsDataUrl = (file: File) => new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = reject;
        reader.readAsDataURL(file);
      });
      const readFileAsText = (file: File) => new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = reject;
        reader.readAsText(file);
      });

      const makePreviewAttachment = async (file: File) => {
        const isImage = file.type.startsWith('image/');
        const isAudio = file.type.startsWith('audio/');
        const url = isImage || isAudio ? await readFileAsDataUrl(file) : URL.createObjectURL(file);
        return {
          id: `att_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
          messageId: 'temp',
          type: isImage ? 'image' : (isAudio ? 'audio' : 'file'),
          name: file.name,
          url,
          size: file.size,
          mimeType: file.type,
          createdAt: new Date(),
        };
      };

      const makeApiAttachment = async (file: File) => {
        const isImage = file.type.startsWith('image/');
        const isText = file.type.startsWith('text/') || file.type === 'application/json';
        const isPdf = file.type === 'application/pdf' || /\.pdf$/i.test(file.name);
        const isDocx = file.type === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' || /\.docx$/i.test(file.name);
        const MAX_EMBED = 5 * 1024 * 1024; // 5MB 上限，超出不内联内容

        if (isImage) {
          const dataUrl = await readFileAsDataUrl(file);
          return { name: file.name, mimeType: file.type, size: file.size, type: 'image', dataUrl };
        }
        if (isText) {
          const text = await readFileAsText(file);
          return { name: file.name, mimeType: file.type, size: file.size, type: 'file', text };
        }
        if ((isPdf || isDocx) && file.size <= MAX_EMBED) {
          // 小体积 docx/pdf 以内联 base64，由后端负责抽取正文
          const dataUrl = await readFileAsDataUrl(file);
          return { name: file.name, mimeType: isPdf ? 'application/pdf' : 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: file.size, type: 'file', dataUrl };
        }
        // 其他或超限：仅元数据
        return { name: file.name, mimeType: file.type, size: file.size, type: 'file' };
      };

      const previewAttachments = await Promise.all((safeAttachments || []).map(makePreviewAttachment));
      const apiAttachments = await Promise.all((safeAttachments || []).map(makeApiAttachment));

      // 创建用户消息（仅前端展示，不立即保存数据库）
      const userMessageTime = new Date();
      const userMessage: Message = {
        id: `temp_user_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`,
        sessionId: currentSessionId,
        role: 'user',
        content,
        status: 'completed',
        createdAt: userMessageTime,
        metadata: {
          conversation_turn_id: conversationTurnId,
          ...(previewAttachments.length > 0 ? { attachments: previewAttachments as any } : {}),
          model: selectedAgent ? `[Agent] ${selectedAgent.name}` : selectedModel.model_name,
          ...(selectedModel
            ? {
                modelConfig: {
                  id: selectedModel.id,
                  name: selectedModel.name,
                  base_url: selectedModel.base_url,
                  model_name: selectedModel.model_name,
                },
              }
            : {}),
          historyProcess: {
            hasProcess: false,
            toolCallCount: 0,
            thinkingCount: 0,
            processMessageCount: 0,
            userMessageId: '',
            finalAssistantMessageId: null,
            expanded: false,
            loaded: true,
            loading: false,
          },
        },
      };
      tempUserId = userMessage.id;
      if (userMessage.metadata?.historyProcess) {
        userMessage.metadata.historyProcess.userMessageId = userMessage.id;
      }

      set((state: any) => {
        state.messages.push(userMessage);

        if (!state.sessionTurnProcessState) {
          state.sessionTurnProcessState = {};
        }
        if (!state.sessionTurnProcessState[currentSessionId]) {
          state.sessionTurnProcessState[currentSessionId] = {};
        }
        state.sessionTurnProcessState[currentSessionId][userMessage.id] = {
          expanded: false,
          loaded: true,
          loading: false,
        };

        const prev = state.sessionChatState[currentSessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
        state.sessionChatState[currentSessionId] = { ...prev, isLoading: true, isStreaming: true };
        if (state.currentSessionId === currentSessionId) {
          state.isLoading = true;
          state.isStreaming = true;
        }
      });

      // 创建临时的助手消息用于UI显示，但不保存到数据库
      const assistantMessageTime = new Date(userMessageTime.getTime() + 1);
      const tempAssistantMessage = {
        id: `temp_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        sessionId: currentSessionId,
        role: 'assistant' as const,
        content: '',
        status: 'streaming' as const,
        createdAt: assistantMessageTime,
        metadata: {
          conversation_turn_id: conversationTurnId,
          model: selectedAgent ? `[Agent] ${selectedAgent.name}` : selectedModel.model_name,
          ...(selectedModel
            ? {
                modelConfig: {
                  id: selectedModel.id,
                  name: selectedModel.name,
                  base_url: selectedModel.base_url,
                  model_name: selectedModel.model_name,
                },
              }
            : {}),
          historyFinalForUserMessageId: userMessage.id,
          historyProcessExpanded: false,
          toolCalls: [], // 初始化工具调用数组
          contentSegments: [{ content: '', type: 'text' as const }], // 初始化内容分段
          currentSegmentIndex: 0, // 当前正在写入的分段索引
        },
      };
      tempAssistantId = tempAssistantMessage.id;

      set((state: any) => {
        state.messages.push(tempAssistantMessage);

        const linkedUserMessage = state.messages.find((m: any) => m.id === userMessage.id && m.role === 'user');
        if (linkedUserMessage?.metadata?.historyProcess) {
          linkedUserMessage.metadata.historyProcess.finalAssistantMessageId = tempAssistantMessage.id;
        }

        const prev = state.sessionChatState[currentSessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
        state.sessionChatState[currentSessionId] = {
          ...prev,
          isLoading: true,
          isStreaming: true,
          streamingMessageId: tempAssistantMessage.id,
        };
        if (!state.sessionStreamingMessageDrafts) {
          state.sessionStreamingMessageDrafts = {};
        }
        state.sessionStreamingMessageDrafts[currentSessionId] = cloneStreamingMessageDraft(tempAssistantMessage);
        if (state.currentSessionId === currentSessionId) {
          state.streamingMessageId = tempAssistantMessage.id;
        }
      });

      // 准备聊天请求数据（根据选择的目标：模型或智能体）
      const chatRequest = selectedAgent
        ? {
            session_id: currentSessionId,
            turn_id: conversationTurnId,
            message: content,
            // 仅在选择智能体时携带智能体信息，不包含模型配置
            agent_id: selectedAgent.id,
            system_context: activeSystemContext?.content || chatConfig.systemPrompt || '',
            attachments: apiAttachments || [],
            reasoning_enabled: reasoningEnabled,
          }
        : {
            session_id: currentSessionId,
            turn_id: conversationTurnId,
            message: content,
            // 仅在选择模型时携带模型配置
            model_config: {
              model: selectedModel.model_name,
              provider: selectedModel.provider,
              base_url: selectedModel.base_url,
              api_key: selectedModel.api_key || '',
              temperature: chatConfig.temperature,
              thinking_level: selectedModel.thinking_level,
              supports_images: selectedModel.supports_images === true,
              supports_reasoning: selectedModel.supports_reasoning === true,
            },
            system_context: activeSystemContext?.content || chatConfig.systemPrompt || '',
            attachments: apiAttachments || [],
            reasoning_enabled: reasoningEnabled,
          };

      debugLog('🚀 开始调用后端流式聊天API:', chatRequest);

      // 使用后端API进行流式聊天（模型或智能体）
      const response = selectedAgent
        ? await client.streamAgentChat(
            currentSessionId,
            content,
            selectedAgent.id,
            getUserIdParam(),
            apiAttachments,
            reasoningEnabled,
            { useResponses: activeModelConfig?.supports_responses === true, turnId: conversationTurnId }
          )
        : await client.streamChat(
            currentSessionId,
            content,
            selectedModel,
            getUserIdParam(),
            apiAttachments,
            reasoningEnabled,
            { turnId: conversationTurnId }
          );

      if (!response) {
        throw new Error('No response received');
      }

      const reader = response.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let sawDone = false;
      let streamedTextBuffer = '';

      const extractSseDataEvents = (source: string) => {
        const events: string[] = [];
        let cursor = 0;

        while (cursor < source.length) {
          const crlfIdx = source.indexOf('\r\n\r\n', cursor);
          const lfIdx = source.indexOf('\n\n', cursor);

          if (crlfIdx === -1 && lfIdx === -1) {
            break;
          }

          let boundary = -1;
          let separatorLength = 0;
          if (crlfIdx !== -1 && (lfIdx === -1 || crlfIdx < lfIdx)) {
            boundary = crlfIdx;
            separatorLength = 4;
          } else {
            boundary = lfIdx;
            separatorLength = 2;
          }

          const rawEvent = source.slice(cursor, boundary);
          cursor = boundary + separatorLength;

          const dataLines = rawEvent
            .split(/\r?\n/)
            .map((line) => line.trimStart())
            .filter((line) => line.startsWith('data:'))
            .map((line) => line.slice(5).trimStart());

          if (dataLines.length > 0) {
            events.push(dataLines.join('\n').trim());
          }
        }

        return { events, rest: source.slice(cursor) };
      };

      const ensureStreamingMessage = (state: any) => {
        let message = state.messages.find((m: any) => m.id === tempAssistantMessage.id);
        if (!message) {
          const savedDraft = state.sessionStreamingMessageDrafts?.[currentSessionId];
          const fallbackMessage = savedDraft
            ? cloneStreamingMessageDraft(savedDraft)
            : {
                ...tempAssistantMessage,
                role: 'assistant' as const,
                status: 'streaming' as const,
                content: streamedTextBuffer,
                metadata: {
                  ...(tempAssistantMessage.metadata || {}),
                  toolCalls: [],
                  contentSegments: [{ content: streamedTextBuffer, type: 'text' as const }],
                  currentSegmentIndex: 0,
                },
              };

          if (state.currentSessionId === currentSessionId) {
            state.messages.push(fallbackMessage);
          }
          message = fallbackMessage;
        }
        return message;
      };

      const persistStreamingMessageDraft = (state: any, message: any) => {
        if (!message) {
          return;
        }
        if (!state.sessionStreamingMessageDrafts) {
          state.sessionStreamingMessageDrafts = {};
        }
        state.sessionStreamingMessageDrafts[currentSessionId] = cloneStreamingMessageDraft(message);
      };

      const updateTurnHistoryProcess = (state: any, updater: (current: any) => Partial<any>) => {
        if (!tempUserId) {
          return;
        }

        const userMessage = state.messages.find((m: any) => m.id === tempUserId && m.role === 'user');
        if (!userMessage) {
          return;
        }

        if (!userMessage.metadata) {
          userMessage.metadata = {} as any;
        }

        const current = userMessage.metadata.historyProcess || {
          hasProcess: false,
          toolCallCount: 0,
          thinkingCount: 0,
          processMessageCount: 0,
          userMessageId: tempUserId,
          finalAssistantMessageId: tempAssistantMessage.id,
          expanded: false,
          loaded: true,
          loading: false,
        };

        const patch = updater(current) || {};
        const next = {
          ...current,
          ...patch,
          userMessageId: tempUserId,
          finalAssistantMessageId: tempAssistantMessage.id,
        };

        const toolCallCount = Number(next.toolCallCount || 0);
        const thinkingCount = Number(next.thinkingCount || 0);
        const processMessageCount = Number(next.processMessageCount || 0);
        next.hasProcess = Boolean(next.hasProcess || toolCallCount > 0 || thinkingCount > 0 || processMessageCount > 0);

        userMessage.metadata.historyProcess = next;

        const assistantMessage = state.messages.find((m: any) => m.id === tempAssistantMessage.id && m.role === 'assistant');
        if (assistantMessage?.metadata) {
          assistantMessage.metadata.historyProcessExpanded = next.expanded === true;
        }
      };

      const appendTextToStreamingMessage = (contentStr: string) => {
        if (!contentStr) return;
        streamedTextBuffer += contentStr;

        set((state: any) => {
          const message = ensureStreamingMessage(state);
          if (message && message.metadata) {
            const currentIndex = message.metadata.currentSegmentIndex || 0;
            const segments = message.metadata.contentSegments || [];

            if (segments[currentIndex] && segments[currentIndex].type === 'text') {
              segments[currentIndex].content += contentStr;
            } else {
              segments.push({ content: contentStr, type: 'text' as const });
              message.metadata.currentSegmentIndex = segments.length - 1;
            }

            message.metadata.contentSegments = segments;
            message.content = segments
              .filter((s: any) => s.type === 'text')
              .map((s: any) => s.content)
              .join('');
            (message as any).updatedAt = new Date();
          }
          persistStreamingMessageDraft(state, message);
        });
      };

      const applyCompleteContent = (finalContent: string) => {
        if (!finalContent) return;
        streamedTextBuffer = finalContent;

        set((state: any) => {
          const message = ensureStreamingMessage(state);
          if (!message || !message.metadata) return;

          const segments = message.metadata.contentSegments || [];
          let textIndex = -1;
          for (let i = segments.length - 1; i >= 0; i--) {
            if (segments[i].type === 'text') {
              textIndex = i;
              break;
            }
          }

          if (textIndex === -1) {
            segments.push({ content: finalContent, type: 'text' as const });
            textIndex = segments.length - 1;
          } else {
            segments[textIndex].content = finalContent;
            for (let i = 0; i < segments.length; i++) {
              if (i !== textIndex && segments[i].type === 'text') {
                segments[i].content = '';
              }
            }
          }

          message.metadata.contentSegments = segments;
          message.metadata.currentSegmentIndex = textIndex;
          message.content = finalContent;
          (message as any).updatedAt = new Date();
          persistStreamingMessageDraft(state, message);
        });
      };

      try {
        while (true) {
          const { done, value } = await reader.read();

          if (value) {
            buffer += decoder.decode(value, { stream: !done });
          }

          if (done && buffer.trim() !== '') {
            // 连接关闭时主动补一个事件分隔，避免尾包没有空行时被丢弃
            buffer = `${buffer}\n\n`;
          }

          const parsedEvents = extractSseDataEvents(buffer);
          buffer = parsedEvents.rest;

          for (const data of parsedEvents.events) {
            if (data === '') continue;

            if (data === '[DONE]') {
                debugLog('✅ 收到完成信号');
                sawDone = true;
                break;
              }

            try {
                const parsed = JSON.parse(data);

                // 兼容后端以字符串形式发送的 [DONE]
                if (typeof parsed === 'string' && parsed === '[DONE]') {
                  debugLog('✅ 收到完成信号');
                  sawDone = true;
                  break;
                }

                // 处理后端发送的数据格式
                if (parsed.type === 'chunk') {
                  // 后端发送格式: {type: 'chunk', content: '...', accumulated: '...'}
                  if (parsed.content) {
                    const contentStr =
                      typeof parsed.content === 'string'
                        ? parsed.content
                        : typeof parsed === 'string'
                        ? parsed
                        : parsed.content || '';
                    appendTextToStreamingMessage(contentStr);
                  }

                } else if (parsed.type === 'thinking') {
                  // 新增类型：模型的思考过程（与正文分离，可折叠显示，灰色字体）
                  if (parsed.content) {
                    set((state: any) => {
                      const message = ensureStreamingMessage(state);
                      if (message && message.metadata) {
                        const contentStr =
                          typeof parsed.content === 'string'
                            ? parsed.content
                            : typeof parsed === 'string'
                            ? parsed
                            : parsed.content || '';
                        const segments = message.metadata.contentSegments || [];
                        const lastIdx = segments.length - 1;
                        let createdThinkingSegment = false;

                        if (lastIdx >= 0 && segments[lastIdx].type === 'thinking') {
                          // Continue appending to the current thinking segment
                          (segments[lastIdx] as any).content += contentStr;
                          message.metadata.currentSegmentIndex = lastIdx;
                        } else {
                          // Start a new thinking segment
                          segments.push({ content: contentStr, type: 'thinking' as const });
                          message.metadata.currentSegmentIndex = segments.length - 1;
                          createdThinkingSegment = true;
                        }

                        // Keep only text segments in message.content
                        message.content = segments
                          .filter((s: any) => s.type === 'text')
                          .map((s: any) => s.content)
                          .join('');

                        updateTurnHistoryProcess(state, (current: any) => ({
                          hasProcess: true,
                          thinkingCount: Number(current?.thinkingCount || 0) + (createdThinkingSegment ? 1 : 0),
                          processMessageCount: Number(current?.processMessageCount || 0) + (createdThinkingSegment ? 1 : 0),
                        }));

                        (message as any).updatedAt = new Date();
                      }
                      persistStreamingMessageDraft(state, message);
                    });
                  }
                } else if (parsed.type === 'context_summarized' || parsed.type === 'context_summarized_start' || parsed.type === 'context_summarized_stream' || parsed.type === 'context_summarized_end') {
                  // 将“摘要”作为当前助手消息的一个流式分段（thinking 样式），严格按事件顺序渲染并具备打字机效果
                  const data = parsed.data || {};
                  const header = '【上下文摘要】\\n';
                  set((state: any) => {
                    const message = ensureStreamingMessage(state);
                    if (!message || !message.metadata) return;
                    const segments = message.metadata.contentSegments || [];

                    // 定位/创建“摘要”专用分段（使用 thinking 样式以便流式渲染）
                    const ensureSummarySegment = () => {
                      const lastIdx = segments.length - 1;
                      if (lastIdx >= 0 && segments[lastIdx].type === 'thinking' && String((segments[lastIdx] as any).content || '').startsWith(header)) {
                        return lastIdx;
                      }
                      segments.push({ content: header, type: 'thinking' as const });
                      message.metadata.contentSegments = segments;
                      message.metadata.currentSegmentIndex = segments.length - 1;
                      return segments.length - 1;
                    };

                    if (parsed.type === 'context_summarized_start') {
                      ensureSummarySegment();
                    } else if (parsed.type === 'context_summarized_stream') {
                      const idx = ensureSummarySegment();
                      const chunkContent = data.content || data.chunk || data.data || '';
                      (segments[idx] as any).content += String(chunkContent);
                      message.metadata.currentSegmentIndex = idx;
                    } else if (parsed.type === 'context_summarized_end' || parsed.type === 'context_summarized') {
                      const idx = ensureSummarySegment();
                      const full = typeof data.full_summary === 'string' && data.full_summary.length > 0 ? data.full_summary : (data.summary_preview || '');
                      (segments[idx] as any).content = header + String(full);
                      message.metadata.contentSegments = segments;
                      message.metadata.currentSegmentIndex = idx;
                    }

                    updateTurnHistoryProcess(state, (current: any) => ({
                      hasProcess: true,
                      thinkingCount: Number(current?.thinkingCount || 0) + (parsed.type === 'context_summarized_start' ? 1 : 0),
                      processMessageCount: Number(current?.processMessageCount || 0) + (parsed.type === 'context_summarized_start' ? 1 : 0),
                    }));

                    (message as any).updatedAt = new Date();
                    persistStreamingMessageDraft(state, message);
                  });
                } else if (parsed.type === 'content') {
                  // 兼容旧格式: {type: 'content', content: '...'}
                  const contentStr =
                    typeof parsed.content === 'string'
                      ? parsed.content
                      : typeof parsed === 'string'
                      ? parsed
                      : parsed.content || '';
                  appendTextToStreamingMessage(contentStr);

                } else if (parsed.type === 'tools_start') {
                  // 处理工具调用事件
                  debugLog('🔧 收到工具调用:', parsed.data);
                  debugLog('🔧 工具调用数据类型:', typeof parsed.data, '是否为数组:', Array.isArray(parsed.data));

                  // 数据转换函数：将后端格式转换为前端期望的格式
                  const convertToolCallData = (tc: any) => {
                    debugLog('🔧 [DEBUG] 原始工具调用数据:', tc);
                    debugLog('🔧 [DEBUG] tc.function:', tc.function);
                    debugLog('🔧 [DEBUG] tc.function?.name:', tc.function?.name);
                    debugLog('🔧 [DEBUG] tc.name:', tc.name);

                    const toolCall = {
                      id: tc.id || tc.tool_call_id || `tool_${Date.now()}_${Math.random()}`, // 确保有ID
                      messageId: tempAssistantMessage.id, // 添加前端需要的messageId
                      name: tc.function?.name || tc.name || 'unknown_tool', // 兼容不同的name字段位置
                      arguments: tc.function?.arguments || tc.arguments || '{}', // 兼容不同的arguments字段位置
                      result: tc.result || '', // 初始化result字段
                      finalResult: tc.finalResult || tc.final_result || tc.result || '',
                      streamLog: tc.streamLog || tc.stream_log || '',
                      completed: tc.completed === true,
                      error: tc.error || undefined, // 可选的error字段
                      createdAt: tc.createdAt || tc.created_at || new Date(), // 添加前端需要的createdAt，支持多种时间格式
                    };

                    debugLog('🔧 [DEBUG] 转换后的工具调用:', toolCall);
                    return toolCall;
                  };

                  // 修复：从 parsed.data.tool_calls 中提取工具调用数组
                  debugLog('🔧 [DEBUG] tools_start 原始数据:', parsed.data);
                  const rawToolCalls = parsed.data.tool_calls || parsed.data;
                  const toolCallsArray = Array.isArray(rawToolCalls) ? rawToolCalls : [rawToolCalls];
                  debugLog('🔧 [DEBUG] 提取的工具调用数组:', toolCallsArray);

                  set((state: any) => {
                    const messageIndex = state.messages.findIndex((m: any) => m.id === tempAssistantMessage.id);
                    debugLog('🔧 查找消息索引:', messageIndex, '消息ID:', tempAssistantMessage.id);
                    const message = ensureStreamingMessage(state);
                    if (!message) {
                      debugLog('🔧 ❌ 未找到对应的消息');
                      return;
                    }

                    debugLog('🔧 找到消息，当前metadata:', message.metadata);
                    if (!message.metadata) {
                      message.metadata = {} as any;
                    }
                    if (!message.metadata.toolCalls) {
                      message.metadata.toolCalls = [] as any[];
                    }

                    const segments = message.metadata.contentSegments || [];

                    // 处理所有工具调用
                    debugLog('🔧 处理工具调用数组，长度:', toolCallsArray.length);
                    toolCallsArray.forEach((tc: any) => {
                      const toolCall = convertToolCallData(tc);
                      debugLog('🔧 添加转换后的工具调用:', toolCall);
                      message.metadata!.toolCalls!.push(toolCall);

                      // 添加工具调用分段
                      segments.push({
                        content: '',
                        type: 'tool_call' as const,
                        toolCallId: toolCall.id,
                      });
                    });

                    // 为工具调用后的内容创建新的文本分段
                    segments.push({ content: '', type: 'text' as const });
                    message.metadata!.currentSegmentIndex = segments.length - 1;

                    updateTurnHistoryProcess(state, (current: any) => ({
                      hasProcess: true,
                      toolCallCount: Number(current?.toolCallCount || 0) + toolCallsArray.length,
                      processMessageCount: Number(current?.processMessageCount || 0) + toolCallsArray.length,
                    }));

                    debugLog('[tools_start] updated toolCalls:', message.metadata.toolCalls);
                    (message as any).updatedAt = new Date();
                    persistStreamingMessageDraft(state, message);
                  });
                } else if (parsed.type === 'tools_end') {
                  // 处理工具结果事件
                  debugLog('🔧 收到工具结果:', parsed.data);
                  debugLog('🔧 工具结果数据类型:', typeof parsed.data);

                  // 兼容多种后端结构：{tool_results:[...]}, {results:[...]}, [...] 或单对象
                  const rawResults = parsed.data?.tool_results || parsed.data?.results || parsed.data;
                  const resultsArray = Array.isArray(rawResults)
                    ? rawResults
                    : (rawResults ? [rawResults] : []);

                  set((state: any) => {
                    const message = ensureStreamingMessage(state);
                    if (!message || !message.metadata || !message.metadata.toolCalls) {
                      return;
                    }

                    // 更新对应工具调用的结果
                    resultsArray.forEach((result: any) => {
                      // 统一字段名称处理：支持 tool_call_id、id、toolCallId 等不同命名
                      const toolCallId = result.tool_call_id || result.id || result.toolCallId;

                      if (!toolCallId) {
                        console.warn('⚠️ 工具结果缺少工具调用ID:', result);
                        return;
                      }

                      debugLog('🔍 查找工具调用:', toolCallId, '在消息中:', message.metadata?.toolCalls?.map((tc: any) => tc.id));
                      const toolCall = message.metadata!.toolCalls!.find((tc: any) => tc.id === toolCallId);

                      if (toolCall) {
                        debugLog('✅ 找到工具调用，更新最终结果:', toolCall.id);

                        // 根据后端数据格式处理最终结果
                        // 支持多种结果字段名称：result、content、output
                        const resultContent = result.result || result.content || result.output || '';

                        // 检查执行状态
                        if (result.success === false || result.is_error === true) {
                          // 工具执行失败
                          toolCall.error = result.error || resultContent || '工具执行失败';
                          toolCall.completed = true;
                          debugLog('❌ 工具执行失败:', {
                            id: toolCall.id,
                            name: result.name || toolCall.name,
                            error: toolCall.error,
                            success: result.success,
                            is_error: result.is_error,
                          });
                        } else {
                          // 工具执行成功，记录最终结果（不覆盖 streamLog）
                          if (typeof resultContent === 'string' && resultContent.length > 0) {
                            toolCall.finalResult = resultContent;
                            toolCall.result = resultContent;
                          } else if (!toolCall.result || toolCall.result.trim() === '') {
                            toolCall.result = resultContent;
                          }

                          toolCall.completed = true;

                          // 清除可能存在的错误状态
                          if (toolCall.error) {
                            delete toolCall.error;
                          }

                          debugLog('✅ 工具执行成功，最终结果已更新:', {
                            id: toolCall.id,
                            name: result.name || toolCall.name,
                            resultLength: (toolCall.result || '').length,
                            streamLogLength: (toolCall.streamLog || '').length,
                            success: result.success,
                            is_stream: result.is_stream,
                          });
                        }
                      } else {
                        debugLog('❌ 未找到对应的工具调用:', toolCallId);
                        debugLog('📋 当前可用的工具调用ID:', message.metadata?.toolCalls?.map((tc: any) => tc.id));
                      }
                    });

                    // 强制触发消息更新以确保自动滚动
                    // 通过更新消息的 updatedAt 时间戳来触发 React 重新渲染
                    (message as any).updatedAt = new Date();
                    persistStreamingMessageDraft(state, message);
                  });
                } else if (parsed.type === 'tools_stream') {
                  // 处理工具流式返回内容
                  debugLog('🔧 收到工具流式数据:', parsed.data);
                  const data = parsed.data;
                  const reviewPanel = extractTaskReviewPanelFromToolStream(
                    data,
                    currentSessionId,
                    conversationTurnId
                  );
                  if (reviewPanel) {
                    debugLog('📝 收到任务确认事件，打开任务编辑面板:', reviewPanel);
                    set((state: any) => {
                      const sessionId = reviewPanel.sessionId;
                      const panels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
                        ? state.taskReviewPanelsBySession[sessionId]
                        : [];
                      const index = panels.findIndex((item: any) => item.reviewId === reviewPanel.reviewId);
                      if (index >= 0) {
                        panels[index] = reviewPanel;
                      } else {
                        panels.push(reviewPanel);
                      }
                      state.taskReviewPanelsBySession[sessionId] = panels;
                      if (state.currentSessionId === sessionId) {
                        state.taskReviewPanel = panels[0] || reviewPanel;
                      }

                      const message = ensureStreamingMessage(state);
                      if (!message) {
                        return;
                      }
                      if (message.metadata && message.metadata.toolCalls) {
                        const toolCallId = data?.toolCallId || data?.tool_call_id || data?.id;
                        if (toolCallId) {
                          const toolCall = message.metadata.toolCalls.find((tc: any) => tc.id === toolCallId);
                          if (toolCall) {
                            toolCall.result = 'Waiting for task confirmation...';
                            toolCall.completed = false;
                          }
                        }
                      }
                      (message as any).updatedAt = new Date();
                      persistStreamingMessageDraft(state, message);
                    });
                    continue;
                  }

                  set((state: any) => {
                    const message = ensureStreamingMessage(state);
                    if (!message || !message.metadata || !message.metadata.toolCalls) {
                      return;
                    }

                    // 统一字段名称处理：支持 toolCallId、tool_call_id、id 等不同命名
                    const toolCallId = data.toolCallId || data.tool_call_id || data.id;

                    if (!toolCallId) {
                      console.warn('⚠️ 工具流式数据缺少工具调用ID:', data);
                      return;
                    }

                    debugLog('🔍 查找工具调用进行流式更新:', toolCallId);
                    const toolCall = message.metadata.toolCalls.find((tc: any) => tc.id === toolCallId);

                    if (toolCall) {
                      // 根据后端实际发送的数据格式处理
                      // 后端发送: {tool_call_id, name, success, is_error, content, is_stream: true}
                      const rawChunkContent = data.content || data.chunk || data.data || '';
                      const chunkContent = typeof rawChunkContent === 'string'
                        ? rawChunkContent
                        : JSON.stringify(rawChunkContent);
                      const isDeltaStream = data.is_stream === true;

                      // 检查是否有错误
                      if (data.is_error || !data.success) {
                        // 如果是错误，标记工具调用失败
                        toolCall.error = chunkContent || '工具执行出错';
                        toolCall.completed = true;
                        debugLog('❌ 工具流式执行出错:', {
                          id: toolCall.id,
                          error: toolCall.error,
                          success: data.success,
                          is_error: data.is_error,
                        });
                      } else {
                        if (isDeltaStream) {
                          // 保留完整流式日志，便于右侧过程面板展示
                          toolCall.streamLog = (toolCall.streamLog || '') + chunkContent;
                          // 累积增量输出，提供运行中的实时视觉反馈
                          toolCall.result = (toolCall.result || '') + chunkContent;
                        } else {
                          // 非增量事件通常表示工具已经给出完整结果，直接覆盖即可
                          if (typeof chunkContent === 'string' && chunkContent.length > 0) {
                            toolCall.finalResult = chunkContent;
                          }
                          toolCall.result = chunkContent;
                          toolCall.completed = true;
                        }
                        debugLog('🔧 工具流式数据已更新:', {
                          id: toolCall.id,
                          name: data.name,
                          chunkLength: chunkContent.length,
                          totalLength: toolCall.result.length,
                          streamLogLength: (toolCall.streamLog || '').length,
                          success: data.success,
                          is_stream: isDeltaStream,
                        });
                      }

                      // 强制触发UI更新
                      (message as any).updatedAt = new Date();
                      persistStreamingMessageDraft(state, message);
                    }
                  });
                } else if (parsed.type === 'error') {
                  throw new Error(parsed.message || parsed.data?.message || 'Stream error');
                } else if (parsed.type === 'cancelled') {
                  // 标记当前消息中的工具调用为已取消，避免一直处于等待中
                  set((state: any) => {
                    const message = ensureStreamingMessage(state);
                    if (message && message.metadata && message.metadata.toolCalls) {
                      message.metadata.toolCalls.forEach((tc: any) => {
                        if (!tc.error) {
                          const hasResult = tc.result !== undefined && tc.result !== null && String(tc.result).trim() !== '';
                          if (!hasResult) {
                            tc.result = tc.result || '';
                          }
                          tc.error = '已取消';
                        }
                        tc.completed = true;
                      });
                      (message as any).updatedAt = new Date();
                      persistStreamingMessageDraft(state, message);
                    }
                  });
                  debugLog('⚠️ 流式会话已被取消');
                  sawDone = true;
                  break;
                } else if (parsed.type === 'done') {
                  debugLog('✅ 收到完成信号');
                  sawDone = true;
                  break;
                } else if (parsed.type === 'complete') {
                  const finalContent = parsed?.result?.content;
                  if (typeof finalContent === 'string' && finalContent.length > 0) {
                    applyCompleteContent(finalContent);
                  }
                  sawDone = true;
                  break;
                }
            } catch (parseError) {
                const preview = data.length > 400 ? `${data.slice(0, 400)}...` : data;
                console.warn('解析流式数据失败:', parseError, 'dataPreview:', preview);
              }
            }

          if (done) {
            debugLog('✅ 流式响应完成');
            break;
          }

          if (sawDone) {
            break;
          }
        }
      } finally {
        reader.releaseLock();

        // 更新状态，结束流式传输
        set((state: any) => {
          const currentDraft = state.sessionStreamingMessageDrafts?.[currentSessionId];
          if (currentDraft) {
            const finalizedDraft = cloneStreamingMessageDraft(currentDraft);
            const finalizedStatus = sawDone ? 'completed' : ((finalizedDraft as any)?.status || 'streaming');
            (finalizedDraft as any).status = finalizedStatus;
            const existingIndex = state.messages.findIndex((m: any) => m.id === tempAssistantMessage.id);
            const shouldWriteToCurrentMessages = existingIndex !== -1 || state.currentSessionId === currentSessionId;
            if (existingIndex !== -1) {
              state.messages[existingIndex] = {
                ...state.messages[existingIndex],
                ...finalizedDraft,
              };
            } else if (shouldWriteToCurrentMessages) {
              state.messages.push(finalizedDraft);
            }
          }
          if (state.sessionStreamingMessageDrafts) {
            state.sessionStreamingMessageDrafts[currentSessionId] = null;
          }

          const prev = state.sessionChatState[currentSessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
          state.sessionChatState[currentSessionId] = { ...prev, isLoading: false, isStreaming: false, streamingMessageId: null };
          if (state.currentSessionId === currentSessionId) {
            state.isLoading = false;
            state.isStreaming = false;
            state.streamingMessageId = null;
          }
        });
      }

      debugLog('✅ 消息发送完成');
    } catch (error) {
      console.error('❌ 发送消息失败:', error);

      // 移除临时消息并显示错误
      set((state: any) => {
        if (state.sessionStreamingMessageDrafts) {
          state.sessionStreamingMessageDrafts[currentSessionId] = null;
        }
        if (state.currentSessionId === currentSessionId) {
          if (tempAssistantId) {
            const assistantIndex = state.messages.findIndex((m: any) => m.id === tempAssistantId);
            if (assistantIndex !== -1) {
              state.messages.splice(assistantIndex, 1);
            }
          }
          if (tempUserId) {
            const userIndex = state.messages.findIndex((m: any) => m.id === tempUserId);
            if (userIndex !== -1) {
              state.messages.splice(userIndex, 1);
            }
          }
        }
        const prev = state.sessionChatState[currentSessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
        state.sessionChatState[currentSessionId] = { ...prev, isLoading: false, isStreaming: false, streamingMessageId: null };
        if (state.currentSessionId === currentSessionId) {
          state.isLoading = false;
          state.isStreaming = false;
          state.streamingMessageId = null;
          state.error = error instanceof Error ? error.message : 'Failed to send message';
        }
      });

      throw error;
    }
  };
}
