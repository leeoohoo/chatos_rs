import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';
import { prepareAttachmentsForStreaming } from './sendMessage/attachments';
import { createInternalId } from './sendMessage/internalId';
import {
  createDraftAssistantMessage,
  createDraftUserMessage,
} from './sendMessage/messageFactory';
import {
  buildChatRequestLogPayload,
  buildStreamChatRuntimeOptions,
  resolveModelCapabilities,
} from './sendMessage/requestPayload';
import { extractSseDataEvents } from './sendMessage/sse';
import { cloneStreamingMessageDraft } from './sendMessage/streamText';
import { createStreamingMessageStateHelpers } from './sendMessage/streamingState';
import {
  extractTaskReviewPanelFromToolStream,
  extractUiPromptPanelFromToolStream,
} from './sendMessage/toolPanels';
import {
  markToolCallAsWaitingForPanel,
  upsertTaskReviewPanelState,
  upsertUiPromptPanelState,
} from './sendMessage/toolPanelState';
import {
  applyToolEndResultsToMessage,
  applyToolStartToMessage,
  applyToolStreamDataToMessage,
  extractToolCallsFromStartPayload,
  extractToolResultsFromEndPayload,
} from './sendMessage/toolEvents';
import {
  formatAssistantFailureContent,
  resolveReadableErrorMessage,
  resolveStreamErrorPayload,
} from './sendMessage/errorParsing';
import {
  resolveRuntimeConfig,
  resolveSelectedModelOrThrow,
} from './sendMessage/runtime';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from '../helpers/sessionRuntime';
import type { SendMessageRuntimeOptions } from '../types';

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
  const defaultSessionChatState = {
    isLoading: false,
    isStreaming: false,
    isStopping: false,
    streamingMessageId: null as string | null,
    activeTurnId: null as string | null,
  };

  return async function sendMessage(
    content: string,
    attachments: any[] = [],
    runtimeOptions: SendMessageRuntimeOptions = {},
  ) {
    let tempUserId: string | null = null;
    let tempAssistantId: string | null = null;
    const {
      currentSessionId,
      currentSession,
      selectedModelId,
      selectedAgentId,
      aiModelConfigs,
      chatConfig,
      sessionChatState,
      activeSystemContext,
      sessionAiSelectionBySession,
    } = get();

    if (!currentSessionId) {
      throw new Error('No active session');
    }

    // 检查是否已经在发送消息，防止重复发送
    const chatState = sessionChatState[currentSessionId] || defaultSessionChatState;
    if (chatState.isLoading || chatState.isStreaming || chatState.isStopping) {
      debugLog('Message sending already in progress, ignoring duplicate request');
      return;
    }

    const sessionAiSelection = sessionAiSelectionBySession?.[currentSessionId];
    const effectiveSelectedModelId = sessionAiSelection?.selectedModelId ?? selectedModelId;
    const sessionRuntime = readSessionRuntimeFromMetadata(currentSession?.metadata);
    const fallbackContactAgentId = (
      typeof runtimeOptions?.contactAgentId === 'string'
        ? runtimeOptions.contactAgentId.trim()
        : ''
    ) || (
      typeof sessionAiSelection?.selectedAgentId === 'string'
        ? sessionAiSelection.selectedAgentId.trim()
        : ''
    ) || (
      typeof selectedAgentId === 'string'
        ? selectedAgentId.trim()
        : ''
    ) || null;
    const runtimeOptionsWithContactFallback: SendMessageRuntimeOptions = {
      ...runtimeOptions,
      contactAgentId: fallbackContactAgentId,
    };
    const {
      effectiveContactAgentId,
      effectiveProjectId,
      effectiveProjectRoot,
      effectiveWorkspaceRoot,
      effectiveExecutionRoot,
      effectiveMcpEnabled,
      effectiveEnabledMcpIds,
    } = resolveRuntimeConfig(sessionRuntime, runtimeOptionsWithContactFallback);
    const selectedModel = resolveSelectedModelOrThrow(
      effectiveSelectedModelId,
      aiModelConfigs,
    );

    const runtimeMetadata = mergeSessionRuntimeIntoMetadata(currentSession?.metadata, {
      selectedModelId: selectedModel?.id || null,
      contactAgentId: effectiveContactAgentId,
      projectId: effectiveProjectId,
      projectRoot: effectiveProjectRoot,
      workspaceRoot: effectiveWorkspaceRoot,
      mcpEnabled: effectiveMcpEnabled,
      enabledMcpIds: effectiveEnabledMcpIds,
    });
    set((state: any) => {
      const sessionIndex = state.sessions.findIndex((session: any) => session.id === currentSessionId);
      if (sessionIndex >= 0) {
        state.sessions[sessionIndex].metadata = runtimeMetadata;
      }
      if (state.currentSession?.id === currentSessionId) {
        state.currentSession.metadata = runtimeMetadata;
      }
    });
    void client.updateSession(currentSessionId, { metadata: runtimeMetadata }).catch(() => {});

    const conversationTurnId = createInternalId('turn');
    const streamedTextRef = { value: '' };
    let tempAssistantMessage: any = {
      id: '',
      sessionId: currentSessionId,
      role: 'assistant' as const,
      content: '',
      status: 'streaming' as const,
      createdAt: new Date(),
      metadata: {},
    };
    try {
      const {
        supportsImages,
        reasoningEnabled,
      } = resolveModelCapabilities(selectedModel, chatConfig);
      const { previewAttachments, apiAttachments } = await prepareAttachmentsForStreaming(
        attachments,
        supportsImages,
      );

      // 创建用户消息（仅前端展示，不立即保存数据库）
      const userMessageTime = new Date();
      const userMessage: Message = createDraftUserMessage({
        sessionId: currentSessionId,
        content,
        conversationTurnId,
        selectedModel,
        previewAttachments,
        createdAt: userMessageTime,
      });
      tempUserId = userMessage.id;
      const turnProcessKey = conversationTurnId || userMessage.id;
      if (userMessage.metadata?.historyProcess) {
        userMessage.metadata.historyProcess.userMessageId = userMessage.id;
        userMessage.metadata.historyProcess.turnId = turnProcessKey;
      }

      set((state: any) => {
        state.messages.push(userMessage);

        if (!state.sessionTurnProcessState) {
          state.sessionTurnProcessState = {};
        }
        if (!state.sessionTurnProcessState[currentSessionId]) {
          state.sessionTurnProcessState[currentSessionId] = {};
        }
        state.sessionTurnProcessState[currentSessionId][turnProcessKey] = {
          expanded: false,
          loaded: false,
          loading: false,
        };

        const prev = state.sessionChatState[currentSessionId] || defaultSessionChatState;
        state.sessionChatState[currentSessionId] = {
          ...prev,
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          activeTurnId: conversationTurnId,
        };
        if (!state.sessionRuntimeGuidanceState) {
          state.sessionRuntimeGuidanceState = {};
        }
        state.sessionRuntimeGuidanceState[currentSessionId] = {
          pendingCount: 0,
          appliedCount: 0,
          lastGuidanceAt: null,
          lastAppliedAt: null,
        };
        if (state.currentSessionId === currentSessionId) {
          state.isLoading = true;
          state.isStreaming = true;
        }
      });

      // 创建临时的助手消息用于UI显示，但不保存到数据库
      tempAssistantMessage = createDraftAssistantMessage({
        sessionId: currentSessionId,
        conversationTurnId,
        selectedModel,
        userMessage,
        userMessageTime,
      });
      tempAssistantId = tempAssistantMessage.id;

      set((state: any) => {
        state.messages.push(tempAssistantMessage);

        const linkedUserMessage = state.messages.find((m: any) => m.id === userMessage.id && m.role === 'user');
        if (linkedUserMessage?.metadata?.historyProcess) {
          linkedUserMessage.metadata.historyProcess.finalAssistantMessageId = tempAssistantMessage.id;
        }

        const prev = state.sessionChatState[currentSessionId] || defaultSessionChatState;
        state.sessionChatState[currentSessionId] = {
          ...prev,
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingMessageId: tempAssistantMessage.id,
          activeTurnId: conversationTurnId,
        };
        if (!state.sessionStreamingMessageDrafts) {
          state.sessionStreamingMessageDrafts = {};
        }
        state.sessionStreamingMessageDrafts[currentSessionId] = cloneStreamingMessageDraft(tempAssistantMessage);
        if (state.currentSessionId === currentSessionId) {
          state.streamingMessageId = tempAssistantMessage.id;
        }
      });

      const chatRequest = buildChatRequestLogPayload({
        sessionId: currentSessionId,
        turnId: conversationTurnId,
        content,
        selectedModel,
        chatConfig,
        systemContext: activeSystemContext?.content || chatConfig.systemPrompt || '',
        attachments: apiAttachments || [],
        reasoningEnabled,
        contactAgentId: effectiveContactAgentId,
        projectId: effectiveProjectId,
        projectRoot: effectiveExecutionRoot,
        mcpEnabled: effectiveMcpEnabled,
        enabledMcpIds: effectiveEnabledMcpIds,
      });

      debugLog('🚀 开始调用后端流式聊天API:', chatRequest);

      const response = await client.streamChat(
        currentSessionId,
        content,
        selectedModel,
        getUserIdParam(),
        apiAttachments,
        reasoningEnabled,
        buildStreamChatRuntimeOptions({
          turnId: conversationTurnId,
          contactAgentId: effectiveContactAgentId,
          projectId: effectiveProjectId,
          projectRoot: effectiveExecutionRoot,
          mcpEnabled: effectiveMcpEnabled,
          enabledMcpIds: effectiveEnabledMcpIds,
        }),
      );

      if (!response) {
        throw new Error('No response received');
      }

      const reader = response.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let sawDone = false;
      let sawCancelled = false;
      let parseFailureCount = 0;
      let sawMeaningfulStreamData = false;
      const {
        ensureStreamingMessage,
        persistStreamingMessageDraft,
        updateTurnHistoryProcess,
        appendTextToStreamingMessage,
        flushPendingTextToStreamingMessage,
        appendThinkingToStreamingMessage,
        applyCompleteContent,
      } = createStreamingMessageStateHelpers({
        set,
        currentSessionId,
        tempAssistantMessage,
        tempUserId,
        conversationTurnId,
        streamedTextRef,
      });

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
                flushPendingTextToStreamingMessage();
                debugLog('✅ 收到完成信号');
                sawDone = true;
                break;
              }

            let parsed: any;
            try {
              parsed = JSON.parse(data);
              parseFailureCount = 0;
            } catch (parseError) {
              parseFailureCount += 1;
              if (parseFailureCount >= 5) {
                const detail = parseError instanceof Error ? parseError.message : String(parseError);
                throw new Error(`流式响应解析失败（已重试 5 次）: ${detail}`);
              }
              continue;
            }

            // 兼容后端以字符串形式发送的 [DONE]
            if (typeof parsed === 'string' && parsed === '[DONE]') {
              flushPendingTextToStreamingMessage();
              debugLog('✅ 收到完成信号');
              sawDone = true;
              break;
            }

            const isTextDeltaEvent = parsed.type === 'chunk' || parsed.type === 'content';
            if (!isTextDeltaEvent) {
              flushPendingTextToStreamingMessage();
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
                    if (typeof contentStr === 'string' && contentStr.trim().length > 0) {
                      sawMeaningfulStreamData = true;
                    }
                  }

            } else if (parsed.type === 'thinking') {
                  // 新增类型：模型的思考过程（与正文分离，可折叠显示，灰色字体）
                  if (parsed.content) {
                    const contentStr =
                      typeof parsed.content === 'string'
                        ? parsed.content
                        : typeof parsed === 'string'
                        ? parsed
                        : parsed.content || '';
                    appendThinkingToStreamingMessage(contentStr);
                    sawMeaningfulStreamData = true;
                  }
            } else if (parsed.type === 'content') {
                  // 兼容旧格式: {type: 'content', content: '...'}
                  const contentStr =
                    typeof parsed.content === 'string'
                      ? parsed.content
                      : typeof parsed === 'string'
                      ? parsed
                      : parsed.content || '';
                  appendTextToStreamingMessage(contentStr);
                  if (typeof contentStr === 'string' && contentStr.trim().length > 0) {
                    sawMeaningfulStreamData = true;
                  }

            } else if (parsed.type === 'tools_start') {
              debugLog('🔧 收到工具调用:', parsed.data);
              const toolCallsArray = extractToolCallsFromStartPayload(parsed.data);

              set((state: any) => {
                const message = ensureStreamingMessage(state);
                if (!message) {
                  return;
                }

                const addedCount = applyToolStartToMessage(
                  message,
                  toolCallsArray,
                  tempAssistantMessage.id,
                );

                updateTurnHistoryProcess(state, (current: any) => ({
                  hasProcess: true,
                  toolCallCount: Number(current?.toolCallCount || 0) + addedCount,
                  processMessageCount: Number(current?.processMessageCount || 0) + addedCount,
                }));

                (message as any).updatedAt = new Date();
                persistStreamingMessageDraft(state, message);
              });
              sawMeaningfulStreamData = true;
            } else if (parsed.type === 'tools_end') {
              debugLog('🔧 收到工具结果:', parsed.data);
              const resultsArray = extractToolResultsFromEndPayload(parsed.data);

              set((state: any) => {
                const message = ensureStreamingMessage(state);
                if (!message) {
                  return;
                }

                applyToolEndResultsToMessage(message, resultsArray);
                (message as any).updatedAt = new Date();
                persistStreamingMessageDraft(state, message);
              });
              sawMeaningfulStreamData = true;
            } else if (parsed.type === 'tools_stream') {
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
                  upsertTaskReviewPanelState(state, reviewPanel);

                  const message = ensureStreamingMessage(state);
                  if (!message) {
                    return;
                  }
                  markToolCallAsWaitingForPanel(message, data, 'Waiting for task confirmation...');
                  (message as any).updatedAt = new Date();
                  persistStreamingMessageDraft(state, message);
                });
                continue;
              }

              const uiPromptPanel = extractUiPromptPanelFromToolStream(
                data,
                currentSessionId,
                conversationTurnId
              );
              if (uiPromptPanel) {
                debugLog('🧩 收到 UI Prompt 事件，打开交互面板:', uiPromptPanel);
                set((state: any) => {
                  upsertUiPromptPanelState(state, uiPromptPanel);

                  const message = ensureStreamingMessage(state);
                  if (!message) {
                    return;
                  }
                  markToolCallAsWaitingForPanel(message, data, 'Waiting for UI prompt response...');
                  (message as any).updatedAt = new Date();
                  persistStreamingMessageDraft(state, message);
                });
                continue;
              }

              set((state: any) => {
                const message = ensureStreamingMessage(state);
                if (!message) {
                  return;
                }

                const updated = applyToolStreamDataToMessage(message, data);
                if (!updated) {
                  return;
                }

                (message as any).updatedAt = new Date();
                persistStreamingMessageDraft(state, message);
              });
              sawMeaningfulStreamData = true;
            } else if (parsed.type === 'runtime_guidance_queued') {
              const data = (parsed && typeof parsed === 'object') ? parsed.data : null;
              set((state: any) => {
                if (!state.sessionRuntimeGuidanceState) {
                  state.sessionRuntimeGuidanceState = {};
                }
                const prev = state.sessionRuntimeGuidanceState[currentSessionId] || {
                  pendingCount: 0,
                  appliedCount: 0,
                  lastGuidanceAt: null,
                  lastAppliedAt: null,
                };
                const pendingFromPayload = Number(
                  (data && Number.isFinite(Number(data.pending_count)))
                    ? Number(data.pending_count)
                    : Number.NaN
                );
                state.sessionRuntimeGuidanceState[currentSessionId] = {
                  ...prev,
                  pendingCount: Number.isFinite(pendingFromPayload)
                    ? Math.max(0, pendingFromPayload)
                    : Math.max(0, Number(prev.pendingCount || 0) + 1),
                  lastGuidanceAt: typeof parsed.timestamp === 'string' ? parsed.timestamp : prev.lastGuidanceAt,
                };
              });
            } else if (parsed.type === 'runtime_guidance_applied') {
              const data = (parsed && typeof parsed === 'object') ? parsed.data : null;
              set((state: any) => {
                if (!state.sessionRuntimeGuidanceState) {
                  state.sessionRuntimeGuidanceState = {};
                }
                const prev = state.sessionRuntimeGuidanceState[currentSessionId] || {
                  pendingCount: 0,
                  appliedCount: 0,
                  lastGuidanceAt: null,
                  lastAppliedAt: null,
                };
                const pendingFromPayload = Number(
                  (data && Number.isFinite(Number(data.pending_count)))
                    ? Number(data.pending_count)
                    : Number.NaN
                );
                const nextPending = Number.isFinite(pendingFromPayload)
                  ? Math.max(0, pendingFromPayload)
                  : Math.max(0, Number(prev.pendingCount || 0) - 1);
                state.sessionRuntimeGuidanceState[currentSessionId] = {
                  ...prev,
                  pendingCount: nextPending,
                  appliedCount: Math.max(0, Number(prev.appliedCount || 0) + 1),
                  lastAppliedAt: (
                    typeof data?.applied_at === 'string'
                      ? data.applied_at
                      : (typeof parsed.timestamp === 'string' ? parsed.timestamp : prev.lastAppliedAt)
                  ),
                };
              });
            } else if (parsed.type === 'error') {
              const streamError = resolveStreamErrorPayload(parsed);
              throw new Error(
                typeof streamError.code === 'string' && streamError.code.trim().length > 0
                  ? `[${streamError.code}] ${streamError.message}`
                  : streamError.message
              );
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
                  // 仅标记“已收到取消事件”，继续等待后端 done / complete，
                  // 避免前端在 stop 请求返回后立即结束。
                  debugLog('⚠️ 收到取消事件，等待后端完成信号...');
                  sawCancelled = true;
                  continue;
            } else if (parsed.type === 'done') {
                  flushPendingTextToStreamingMessage();
                  debugLog('✅ 收到完成信号');
                  sawDone = true;
                  break;
            } else if (parsed.type === 'complete') {
                  flushPendingTextToStreamingMessage();
                  const finalContent = parsed?.result?.content;
                  if (typeof finalContent === 'string' && finalContent.length > 0) {
                    applyCompleteContent(finalContent);
                  }
                  sawDone = true;
                  break;
            }
          }

          if (done) {
            flushPendingTextToStreamingMessage();
            debugLog('✅ 流式响应完成');
            if (!sawDone) {
              if (sawCancelled) {
                // 某些后端实现只会发送 cancelled，然后直接关闭连接，不再额外发送 done。
                // 此时按正常取消完成处理，避免误判为中断错误。
                debugLog('⚠️ 未收到 done/complete，但已收到 cancelled，按取消完成处理');
                sawDone = true;
                break;
              }
              const hasBufferedText =
                typeof streamedTextRef.value === 'string' && streamedTextRef.value.trim().length > 0;
              if (sawMeaningfulStreamData || hasBufferedText) {
                // Some providers/gateways close stream without explicit done marker.
                debugLog('⚠️ 未收到 done/complete 事件，按已接收流数据正常结束');
                sawDone = true;
              } else {
                throw new Error('流式响应在完成前中断，请稍后重试');
              }
            }
            break;
          }

          if (sawDone) {
            break;
          }
        }
      } finally {
        flushPendingTextToStreamingMessage();
        reader.releaseLock();

        // 更新状态，结束流式传输
        set((state: any) => {
          const currentDraft = state.sessionStreamingMessageDrafts?.[currentSessionId];
          let backgroundFinalizedDraft: any = null;
          if (currentDraft) {
            const finalizedDraft = cloneStreamingMessageDraft(currentDraft);
            const finalizedStatus = sawDone ? 'completed' : 'error';
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
            } else {
              // 会话不在前台时，保留最终草稿，避免切回后短时间内看不到消息
              backgroundFinalizedDraft = finalizedDraft;
            }
          }
          if (state.sessionStreamingMessageDrafts) {
            state.sessionStreamingMessageDrafts[currentSessionId] = backgroundFinalizedDraft;
          }

          const prev = state.sessionChatState[currentSessionId] || defaultSessionChatState;
          state.sessionChatState[currentSessionId] = {
            ...prev,
            isLoading: false,
            isStreaming: false,
            isStopping: false,
            streamingMessageId: null,
            activeTurnId: null,
          };
          if (!state.sessionRuntimeGuidanceState) {
            state.sessionRuntimeGuidanceState = {};
          }
          const runtimeGuidanceState = state.sessionRuntimeGuidanceState[currentSessionId];
          if (runtimeGuidanceState) {
            runtimeGuidanceState.pendingCount = 0;
          }
          if (state.currentSessionId === currentSessionId) {
            state.isLoading = false;
            state.isStreaming = false;
            state.streamingMessageId = null;
          }
        });
      }

      debugLog('✅ 消息发送完成');
    } catch (error) {
      const readableError = resolveReadableErrorMessage(error);
      console.error('❌ 发送消息失败:', readableError, error);

      set((state: any) => {
        const existingAssistantIndex = tempAssistantId
          ? state.messages.findIndex((m: any) => m.id === tempAssistantId)
          : -1;
        const currentDraft = state.sessionStreamingMessageDrafts?.[currentSessionId];
        const baseAssistant = existingAssistantIndex !== -1
          ? state.messages[existingAssistantIndex]
          : (currentDraft ? cloneStreamingMessageDraft(currentDraft) : {
              ...tempAssistantMessage,
              content: streamedTextRef.value,
              metadata: {
                ...(tempAssistantMessage.metadata || {}),
                contentSegments: [{ content: streamedTextRef.value, type: 'text' as const }],
                currentSegmentIndex: 0,
              },
            });
        const failureContent = formatAssistantFailureContent(
          readableError,
          typeof baseAssistant?.content === 'string' ? baseAssistant.content : streamedTextRef.value,
        );
        const nextMetadata = {
          ...(baseAssistant?.metadata || {}),
          contentSegments: [{ content: failureContent, type: 'text' as const }],
          currentSegmentIndex: 0,
          requestError: readableError,
        };
        const failureAssistantMessage = {
          ...baseAssistant,
          role: 'assistant' as const,
          status: 'error' as const,
          content: failureContent,
          metadata: nextMetadata,
          updatedAt: new Date(),
        };

        if (existingAssistantIndex !== -1) {
          state.messages[existingAssistantIndex] = failureAssistantMessage;
        } else if (state.currentSessionId === currentSessionId) {
          state.messages.push(failureAssistantMessage);
        }

        if (state.sessionStreamingMessageDrafts) {
          state.sessionStreamingMessageDrafts[currentSessionId] = (
            existingAssistantIndex !== -1 || state.currentSessionId === currentSessionId
          )
            ? null
            : cloneStreamingMessageDraft(failureAssistantMessage);
        }

        const prev = state.sessionChatState[currentSessionId] || defaultSessionChatState;
        state.sessionChatState[currentSessionId] = {
          ...prev,
          isLoading: false,
          isStreaming: false,
          isStopping: false,
          streamingMessageId: null,
          activeTurnId: null,
        };
        if (!state.sessionRuntimeGuidanceState) {
          state.sessionRuntimeGuidanceState = {};
        }
        const runtimeGuidanceState = state.sessionRuntimeGuidanceState[currentSessionId];
        if (runtimeGuidanceState) {
          runtimeGuidanceState.pendingCount = 0;
        }
        if (state.currentSessionId === currentSessionId) {
          state.isLoading = false;
          state.isStreaming = false;
          state.streamingMessageId = null;
          state.error = readableError;
        }
      });

      throw new Error(readableError);
    }
  };
}
