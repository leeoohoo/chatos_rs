import { MessageManager } from './messageManager';
import AiRequestHandler from './aiRequestHandler';
import type { AiModelConfig, Message, MessageRole } from '../../types';
import { debugLog } from '@/lib/utils';

// 使用与aiClient相同的CallbackType
type CallbackType = 'chunk' | 'tool_call' | 'error' | 'complete' | 'tool_stream_chunk' | 'tool_result' | 'conversation_complete' | 'summary_chunk';

/**
 * 工具结果处理器
 * 负责处理单次工具执行结果，生成总结并替换内容
 */
export class ToolResultProcessor {
    private messageManager: MessageManager;
    private modelConfig: AiModelConfig;
    private conversationId: string;
    private callback: (type: CallbackType, data?: any) => void;
    private sessionId: string;
    private configUrl: string;

    constructor(messageManager: MessageManager, modelConfig: AiModelConfig, conversationId: string, callback: (type: CallbackType, data?: any) => void, sessionId?: string, configUrl: string = '/api') {
        this.messageManager = messageManager;
        this.modelConfig = modelConfig;
        this.conversationId = conversationId;
        this.callback = callback;
        this.sessionId = sessionId || conversationId;
        this.configUrl = configUrl;
    }

    /**
     * 处理单次工具执行结果
     * @param executeResult 工具执行结果
     * @param sessionId 会话ID
     * @returns 处理后的工具消息
     */
    async processToolResult(executeResult: any, sessionId: string): Promise<any> {
        // 直接使用累积的工具结果，executeResult 就是单个工具消息对象
        const result = executeResult;
        debugger
        
        // 检查是否需要生成总结
        const contentLength = (result.content || '').length;
        const shouldSummarize = contentLength > 1000; // 超过1000字符时生成总结
        
        let finalContent = result.content || '';
        
        if (shouldSummarize) {
            try {
                const summary = await this.generateContentSummary(result.content, result.tool_name);
                finalContent = summary;
                debugLog(`[Tool Result Summary] 为工具 ${result.tool_name} 生成总结，原长度: ${contentLength}, 总结长度: ${summary.length}`);
            } catch (error: any) {
                // 检查是否是用户中断错误
                if (error.message === 'Stream aborted by user' || error.name === 'AbortError') {
                    debugLog('[Tool Result Summary] 总结生成被用户中断');
                    return;
                }
                console.error(`[Tool Result Summary] 生成总结失败:`, error);
                // 使用原内容
            }
        }

        // 总结完成后，保存最终的工具消息到数据库
        await this.messageManager.saveToolMessage({
            sessionId: sessionId,
            role: 'tool',
            content: result.content || '', // 保存原始内容
            summary: shouldSummarize ? finalContent : undefined, // 保存总结内容
            status: 'completed',
            createdAt: new Date(),
            metadata: {
                 tool_call_id: result.tool_call_id,
                 tool_name: result.tool_name,
                 is_summarized: shouldSummarize,
                 original_length: contentLength
             }
        });

        // 返回用于添加到消息列表的工具消息
        return {
            role: 'tool',
            tool_call_id: result.tool_call_id,
            content: finalContent,
            metadata: {
                tool_name: result.tool_name,
                timestamp: new Date().toISOString(),
                content_length: contentLength,
                is_summarized: shouldSummarize
            }
        };
    }

    /**
     * 生成内容总结
     * @param content 原始内容
     * @param toolName 工具名称
     * @returns 总结内容
     */
    private async generateContentSummary(content: string, toolName: string): Promise<string> {
        try {
            // 创建总结请求的消息
             const summaryMessages: Message[] = [
                 {
                     id: Date.now().toString() + '_system',
                     sessionId: this.conversationId,
                     role: 'system' as MessageRole,
                     content: '请帮我总结一下这个内容，对其内容进行精简，将主要信息提取出来。请用简洁明了的语言概括核心要点。',
                     rawContent: '请帮我总结一下这个内容，对其内容进行精简，将主要信息提取出来。请用简洁明了的语言概括核心要点。',
                     status: 'completed',
                     createdAt: new Date()
                 },
                 {
                     id: Date.now().toString() + '_user',
                     sessionId: this.conversationId,
                     role: 'user' as MessageRole, 
                     content: `请帮我对以下内容进行总结：\n\n${content}`,
                     rawContent: `请帮我对以下内容进行总结：\n\n${content}`,
                     status: 'completed',
                     createdAt: new Date()
                 }
             ];

            let summaryContent = '';
            
            // 创建AI请求处理器
             const aiRequestHandler = new AiRequestHandler(
                 summaryMessages,
                 [], // 空工具列表
                 this.conversationId,
                 (type: string, data: any) => {
                     if (type === 'chunk' && data?.content) {
                         summaryContent += data.content;
                         // 流式展示总结生成过程
                         this.callback('summary_chunk', {
                             content: data.content,
                             accumulated: summaryContent
                         });
                     }
                 },
                 this.modelConfig,
                 this.configUrl, // configUrl
                 this.sessionId
             );

            // 发送总结请求
            await aiRequestHandler.chatCompletion();

            return summaryContent.trim() || `工具 ${toolName} 执行完成，内容长度: ${content.length} 字符`;
        } catch (error: any) {
            // 检查是否是用户中断错误
            if (error.message === 'Stream aborted by user' || error.name === 'AbortError') {
                debugLog('总结生成被用户中断');
                return `工具 ${toolName} 执行完成，内容长度: ${content.length} 字符`;
            }
            console.error('生成总结时出错:', error);
            // 返回简单的默认总结
            return `工具 ${toolName} 执行完成，内容长度: ${content.length} 字符`;
        }
    }


}