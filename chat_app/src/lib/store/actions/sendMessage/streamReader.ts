import { debugLog } from '@/lib/utils';
import { extractSseDataEvents } from './sse';
import type { HandleStreamEventResult } from './streamEventHandler';
import type { StreamEventPayload } from './types';

interface ConsumeChatStreamParams {
  reader: ReadableStreamDefaultReader<Uint8Array>;
  streamedTextRef: { value: string };
  flushPendingTextToStreamingMessage: () => void;
  handleParsedEvent: (parsed: StreamEventPayload) => HandleStreamEventResult;
}

export const consumeChatStream = async ({
  reader,
  streamedTextRef,
  flushPendingTextToStreamingMessage,
  handleParsedEvent,
}: ConsumeChatStreamParams): Promise<HandleStreamEventResult> => {
  const decoder = new TextDecoder();
  let buffer = '';
  let sawDone = false;
  let sawCancelled = false;
  let sawMeaningfulStreamData = false;
  let parseFailureCount = 0;

  try {
    while (true) {
      const { done, value } = await reader.read();

      if (value) {
        buffer += decoder.decode(value, { stream: !done });
      }

      if (done && buffer.trim() !== '') {
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

        let parsed: StreamEventPayload | string;
        try {
          parsed = JSON.parse(data) as StreamEventPayload | string;
          parseFailureCount = 0;
        } catch (parseError) {
          parseFailureCount += 1;
          if (parseFailureCount >= 5) {
            const detail = parseError instanceof Error ? parseError.message : String(parseError);
            throw new Error(`流式响应解析失败（已重试 5 次）: ${detail}`);
          }
          continue;
        }

        if (typeof parsed === 'string' && parsed === '[DONE]') {
          flushPendingTextToStreamingMessage();
          debugLog('✅ 收到完成信号');
          sawDone = true;
          break;
        }
        if (typeof parsed === 'string') {
          continue;
        }

        const eventResult = handleParsedEvent(parsed);
        if (eventResult.sawMeaningfulStreamData) {
          sawMeaningfulStreamData = true;
        }
        if (eventResult.sawCancelled) {
          sawCancelled = true;
        }
        if (eventResult.sawDone) {
          sawDone = true;
          break;
        }
      }

      if (done) {
        flushPendingTextToStreamingMessage();
        debugLog('✅ 流式响应完成');
        if (!sawDone) {
          if (sawCancelled) {
            debugLog('⚠️ 未收到 done/complete，但已收到 cancelled，按取消完成处理');
            sawDone = true;
            break;
          }
          const hasBufferedText =
            typeof streamedTextRef.value === 'string' && streamedTextRef.value.trim().length > 0;
          if (sawMeaningfulStreamData || hasBufferedText) {
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

    return {
      sawDone,
      sawCancelled,
      sawMeaningfulStreamData,
    };
  } finally {
    flushPendingTextToStreamingMessage();
    reader.releaseLock();
  }
};
